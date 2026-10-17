use anyhow::{Context, Result, bail, ensure};
use cpuv_solver::Package;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredFile {
    pub path: PathBuf,
    pub blake3: String,
    pub size: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPackage {
    pub package: Package,
    pub files: Vec<StoredFile>,
}
#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
    max_expanded_bytes: u64,
    max_entries: u64,
}
#[derive(Default, Debug, Serialize)]
pub struct LinkStats {
    pub hardlinks: usize,
    pub copies: usize,
    pub deduplicated_paths: usize,
}

pub fn safe_relative(path: &Path) -> Result<PathBuf> {
    ensure!(!path.as_os_str().is_empty(), "Empty archive path");
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(s) => {
                let text = s.to_string_lossy();
                ensure!(
                    !text.contains([':', '\\']) && !text.ends_with([' ', '.']),
                    "Unsafe archive component {text}"
                );
                let base = text.split('.').next().unwrap_or("").to_ascii_uppercase();
                ensure!(
                    !matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                        && !(base.len() == 4
                            && (base.starts_with("COM") || base.starts_with("LPT"))
                            && base.as_bytes()[3].is_ascii_digit()),
                    "Windows device path {text}"
                );
                out.push(s);
            }
            Component::CurDir => (),
            _ => bail!("Archive path escapes package: {}", path.display()),
        }
    }
    ensure!(!out.as_os_str().is_empty(), "Empty archive path");
    Ok(out)
}

pub fn link_or_copy(from: &Path, to: &Path) -> Result<bool> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::hard_link(from, to) {
        Ok(()) => Ok(true),
        Err(_) => {
            fs::copy(from, to).with_context(|| format!("Materializing {}", to.display()))?;
            Ok(false)
        }
    }
}

fn pacman_metadata(path: &Path, kind: tar::EntryType) -> bool {
    // MinGW packages may register host pacman hooks outside the toolchain prefix.
    // These are installer metadata, never project payload or executable hooks.
    let ancestors = ["usr", "usr/share", "usr/share/libalpm"];
    (kind.is_dir() && ancestors.iter().any(|name| path == Path::new(name)))
        || ((kind.is_dir() || kind.is_file())
            && ["usr/share/libalpm/hooks", "usr/share/libalpm/scripts"]
                .iter()
                .any(|name| path.starts_with(name)))
}

fn digest_file(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut hash = blake3::Hasher::new();
    let mut buf = vec![0; 128 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hash.finalize().to_hex().to_string())
}

impl Store {
    pub fn new(root: PathBuf) -> Result<Self> {
        for dir in ["packages", "blobs", "locks"] {
            fs::create_dir_all(root.join(dir))?;
        }
        Ok(Self {
            root,
            max_expanded_bytes: 64 * 1024 * 1024 * 1024,
            max_entries: 1_000_000,
        })
    }
    pub fn with_extraction_limits(mut self, bytes: u64, entries: u64) -> Self {
        self.max_expanded_bytes = bytes;
        self.max_entries = entries;
        self
    }
    pub fn package_dir(&self, p: &Package) -> PathBuf {
        self.root.join("packages").join(&p.sha256)
    }
    pub fn cached(&self, p: &Package) -> Result<Option<StoredPackage>> {
        let dir = self.package_dir(p);
        let path = dir.join("cpuv-pkg.json");
        if !path.exists() {
            return Ok(None);
        }
        let stored: StoredPackage = serde_json::from_slice(&fs::read(path)?)?;
        ensure!(
            stored.package == *p,
            "Cached package metadata disagrees with lock for {}",
            p.name
        );
        for f in &stored.files {
            let path = dir.join("payload").join(safe_relative(&f.path)?);
            ensure!(
                fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() && m.len() == f.size),
                "Cache is incomplete for {}; remove {} and sync again",
                p.name,
                dir.display()
            );
            ensure!(
                digest_file(&path)? == f.blake3,
                "Cached file was modified: {}. Hardlinked environment files must not be edited; use a fresh --cache-dir to recover",
                path.display()
            );
        }
        Ok(Some(stored))
    }

    pub fn ingest(&self, p: &Package, archive: &Path) -> Result<StoredPackage> {
        let archive_prefix = if p.name.starts_with("mingw-w64-ucrt-x86_64-") {
            "ucrt64"
        } else if p.name.starts_with("mingw-w64-x86_64-") {
            "mingw64"
        } else if p.name.starts_with("mingw-w64-i686-") {
            "mingw32"
        } else {
            bail!("Unsupported package namespace: {}", p.name)
        };
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("locks").join(format!("{}.lock", p.sha256)))?;
        lock.lock_exclusive()?;
        if let Some(cached) = self.cached(p)? {
            return Ok(cached);
        }
        let staging = tempfile::Builder::new()
            .prefix("extract-")
            .tempdir_in(self.root.join("packages"))?;
        let payload = staging.path().join("payload");
        fs::create_dir_all(&payload)?;
        let reader = zstd::stream::read::Decoder::new(File::open(archive)?)?;
        let mut tar = tar::Archive::new(reader);
        let mut links = Vec::new();
        let mut extracted = 0_u64;
        let mut count = 0;
        for entry in tar.entries()? {
            let mut entry = entry?;
            let raw = safe_relative(&entry.path()?)?;
            count += 1;
            extracted = extracted
                .checked_add(entry.size())
                .context("Archive size overflow")?;
            ensure!(
                count <= self.max_entries && extracted <= self.max_expanded_bytes,
                "Package extraction exceeds --max-extract-files or --max-extract-mib"
            );
            let kind = entry.header().entry_type();
            if raw
                .components()
                .next()
                .is_some_and(|x| x.as_os_str().to_string_lossy().starts_with('.'))
            {
                continue;
            }
            if pacman_metadata(&raw, kind) {
                continue;
            }
            let relative = raw.strip_prefix(archive_prefix).with_context(|| {
                format!(
                    "{} contains payload outside its selected prefix: {}",
                    p.name,
                    raw.display()
                )
            })?;
            if relative.as_os_str().is_empty() {
                continue;
            }
            let relative = safe_relative(relative)?;
            let to = payload.join(&relative);
            if kind.is_dir() {
                fs::create_dir_all(&to)?;
            } else if kind.is_file() {
                fs::create_dir_all(to.parent().unwrap())?;
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&to)
                    .context("Duplicate archive file")?;
                std::io::copy(&mut entry, &mut file)?;
            } else if kind.is_symlink() || kind.is_hard_link() {
                let target = entry
                    .link_name()?
                    .context("Archive link lacks target")?
                    .into_owned();
                let target = if kind.is_hard_link() {
                    target
                        .strip_prefix(archive_prefix)
                        .context("Hardlink leaves selected package prefix")?
                        .to_owned()
                } else {
                    relative.parent().unwrap_or(Path::new("")).join(target)
                };
                // Resolve lexical .. within the payload; never create OS symlinks.
                let mut normalized = PathBuf::new();
                for part in target.components() {
                    match part {
                        Component::Normal(s) => normalized.push(s),
                        Component::CurDir => (),
                        Component::ParentDir => ensure!(normalized.pop(), "Link escapes package"),
                        _ => bail!("Absolute archive link"),
                    }
                }
                links.push((relative, safe_relative(&normalized)?));
            } else {
                bail!("Unsupported special archive entry {}", raw.display());
            }
        }
        while !links.is_empty() {
            let old = links.len();
            let mut pending = Vec::new();
            for (to, target) in links {
                let source = payload.join(&target);
                if source.is_file() {
                    let dest = payload.join(to);
                    ensure!(!dest.exists(), "Duplicate archive link");
                    link_or_copy(&source, &dest)?;
                } else {
                    pending.push((to, target));
                }
            }
            ensure!(
                pending.len() < old,
                "Unresolved or cyclic archive symlinks in {}",
                p.name
            );
            links = pending;
        }
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&payload).sort_by_file_name() {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let digest = digest_file(path)?;
            let size = entry.metadata()?.len();
            let blob = self.root.join("blobs").join(&digest[..2]).join(&digest);
            fs::create_dir_all(blob.parent().unwrap())?;
            if !blob.exists() {
                match fs::hard_link(path, &blob) {
                    Ok(()) => (),
                    Err(e) if blob.exists() => {
                        let _ = e;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            ensure!(digest_file(&blob)? == digest, "Corrupted CAS blob {digest}");
            fs::remove_file(path)?;
            link_or_copy(&blob, path)?;
            files.push(StoredFile {
                path: path.strip_prefix(&payload)?.to_owned(),
                blake3: digest,
                size,
            });
        }
        let stored = StoredPackage {
            package: p.clone(),
            files,
        };
        let mut metadata = File::create(staging.path().join("cpuv-pkg.json"))?;
        metadata.write_all(&serde_json::to_vec_pretty(&stored)?)?;
        metadata.sync_all()?;
        drop(metadata);
        fs::rename(staging.path(), self.package_dir(p))
            .context("Publishing verified package to CAS")?;
        Ok(stored)
    }

    pub fn materialize(&self, packages: &[StoredPackage], env: &Path) -> Result<LinkStats> {
        fs::create_dir_all(env)?;
        for dir in ["include", "lib", "bin"] {
            fs::create_dir_all(env.join(dir))?;
        }
        let mut stats = LinkStats::default();
        let mut owners = std::collections::BTreeMap::<PathBuf, String>::new();
        for package in packages {
            let payload = self.package_dir(&package.package).join("payload");
            for file in &package.files {
                let relative = safe_relative(&file.path)?;
                // Windows path matching is case insensitive, even in a test on another OS.
                let key = PathBuf::from(relative.to_string_lossy().to_lowercase());
                if let Some(hash) = owners.get(&key) {
                    ensure!(
                        hash == &file.blake3,
                        "Package file collision at {}",
                        relative.display()
                    );
                    stats.deduplicated_paths += 1;
                    continue;
                }
                owners.insert(key, file.blake3.clone());
                if link_or_copy(&payload.join(&relative), &env.join(&relative))? {
                    stats.hardlinks += 1;
                } else {
                    stats.copies += 1;
                }
            }
        }
        Ok(stats)
    }
}
