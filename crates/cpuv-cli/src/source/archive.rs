use super::*;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

pub fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 128 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub async fn download(
    cache: &Path,
    spec: &Archive,
    offline: bool,
    limits: &cpuv_fetch::Limits,
) -> Result<PathBuf> {
    ensure!(
        spec.sha256.len() == 64 && spec.sha256.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid source/tool SHA-256"
    );
    let url = reqwest::Url::parse(&spec.url)?;
    ensure!(
        cpuv_fetch::policy::allowed_url(&url),
        "Source downloads require HTTPS, except loopback registries"
    );
    fs::create_dir_all(cache)?;
    let target = cache.join(spec.sha256.to_ascii_lowercase());
    if target.is_file() {
        ensure!(
            fs::metadata(&target)?.len() <= limits.archive_bytes,
            "Cached source/tool archive exceeds --max-download-mib"
        );
        ensure!(
            digest(&target)? == spec.sha256.to_ascii_lowercase(),
            "Cached source archive was modified: {}",
            target.display()
        );
        return Ok(target);
    }
    ensure!(!offline, "Source archive unavailable offline: {}", spec.url);
    let client = cpuv_fetch::policy::client(1, limits)?;
    let mut last_error = None;
    for attempt in 0..3 {
        let result: Result<()> = async {
            let mut response = client.get(url.clone()).send().await?.error_for_status()?;
            ensure!(
                response.content_length().unwrap_or(0) <= limits.archive_bytes,
                "Source/tool archive exceeds --max-download-mib"
            );
            let mut temporary = tempfile::NamedTempFile::new_in(cache)?;
            let mut total = 0u64;
            let mut hash = Sha256::new();
            while let Some(bytes) = response.chunk().await? {
                total += bytes.len() as u64;
                ensure!(
                    total <= limits.archive_bytes,
                    "Source/tool archive exceeds --max-download-mib"
                );
                temporary.write_all(&bytes)?;
                hash.update(&bytes);
            }
            ensure!(
                format!("{:x}", hash.finalize()) == spec.sha256.to_ascii_lowercase(),
                "Source archive checksum mismatch: {}",
                spec.url
            );
            temporary.as_file().sync_all()?;
            temporary.persist(&target).map_err(|error| error.error)?;
            Ok(())
        }
        .await;
        match result {
            Ok(()) => return Ok(target),
            Err(error) => last_error = Some(error),
        }
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(150 * (attempt + 1))).await;
        }
    }
    Err(last_error.unwrap())
}

fn destination(root: &Path, name: &Path, strip: usize) -> Result<Option<PathBuf>> {
    let safe = cpuv_store::safe_relative(name)?;
    let parts: Vec<_> = safe.components().collect();
    if parts.len() <= strip {
        return Ok(None);
    }
    Ok(Some(root.join(parts[strip..].iter().collect::<PathBuf>())))
}

fn selected(path: &Path, root: &Path, spec: &Archive) -> Result<bool> {
    let relative = path.strip_prefix(root)?;
    let mut included = spec.include.is_empty();
    for name in &spec.include {
        included |= relative.starts_with(cpuv_store::safe_relative(name)?);
    }
    for name in &spec.exclude {
        if relative.starts_with(cpuv_store::safe_relative(name)?) {
            return Ok(false);
        }
    }
    Ok(included)
}

pub fn extract(
    archive: &Path,
    spec: &Archive,
    root: &Path,
    limits: &cpuv_fetch::Limits,
) -> Result<()> {
    fs::create_dir_all(root)?;
    let mut total = 0u64;
    let mut count = 0usize;
    if spec.archive == "tar.gz" {
        let decoder = flate2::read::GzDecoder::new(File::open(archive)?);
        let mut tar = tar::Archive::new(decoder);
        for entry in tar.entries()? {
            let mut entry = entry?;
            let kind = entry.header().entry_type();
            total = total
                .checked_add(entry.size())
                .context("Archive size overflow")?;
            count += 1;
            ensure!(
                count as u64 <= limits.entries && total <= limits.expanded_bytes,
                "Expanded source/tool archive exceeds --max-extract-files or --max-extract-mib"
            );
            if kind.is_pax_global_extensions()
                || kind.is_pax_local_extensions()
                || kind.is_gnu_longname()
                || kind.is_gnu_longlink()
            {
                ensure!(
                    entry.size() <= 64 * 1024,
                    "Oversized source archive metadata header"
                );
                continue;
            }
            ensure!(
                kind.is_dir() || kind.is_file(),
                "Source archive contains a link or special file"
            );
            let Some(path) = destination(root, &entry.path()?, spec.strip_components)? else {
                continue;
            };
            if !selected(&path, root, spec)? {
                continue;
            }
            if kind.is_dir() {
                fs::create_dir_all(&path)?;
            } else {
                fs::create_dir_all(path.parent().unwrap())?;
                let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
                let written = std::io::copy(&mut entry, &mut output)?;
                ensure!(written == entry.size(), "Truncated source archive entry");
            }
        }
    } else if spec.archive == "zip" {
        let mut zip = zip::ZipArchive::new(File::open(archive)?)?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            ensure!(
                entry
                    .unix_mode()
                    .is_none_or(|mode| mode & 0o170000 != 0o120000),
                "ZIP symbolic links are not supported"
            );
            total = total
                .checked_add(entry.size())
                .context("Archive size overflow")?;
            ensure!(
                (index as u64) < limits.entries && total <= limits.expanded_bytes,
                "Expanded source/tool archive exceeds --max-extract-files or --max-extract-mib"
            );
            let name = entry.enclosed_name().context("Unsafe ZIP path")?;
            let Some(path) = destination(root, &name, spec.strip_components)? else {
                continue;
            };
            if !selected(&path, root, spec)? {
                continue;
            }
            if entry.is_dir() {
                fs::create_dir_all(&path)?;
            } else {
                fs::create_dir_all(path.parent().unwrap())?;
                let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
                let size = entry.size();
                ensure!(
                    std::io::copy(&mut entry, &mut output)? == size,
                    "Truncated ZIP entry"
                );
            }
        }
    } else {
        bail!(
            "Unsupported source archive format {}; use tar.gz or zip",
            spec.archive
        );
    }
    Ok(())
}

pub fn inventory(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root).sort_by_file_name() {
        let entry = entry?;
        ensure!(
            !entry.file_type().is_symlink(),
            "Source installations must not contain symbolic links"
        );
        if entry.file_type().is_file() {
            files.insert(
                cpuv_relocate::portable(entry.path().strip_prefix(root)?),
                digest(entry.path())?,
            );
        }
    }
    Ok(files)
}

pub fn verify(root: &Path, files: &BTreeMap<String, String>) -> Result<()> {
    for (relative, expected) in files {
        let path = root.join(cpuv_store::safe_relative(Path::new(relative))?);
        ensure!(
            fs::symlink_metadata(&path)?.is_file() && digest(&path)? == *expected,
            "Cached source output was modified: {}",
            path.display()
        );
    }
    Ok(())
}
