//! Shared filesystem integrity, reader leases and recoverable project transactions.
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
pub mod arguments;

pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let mut temp =
        tempfile::NamedTempFile::new_in(path.parent().context("Missing parent directory")?)?;
    temp.write_all(data)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("Reading {}", path.display()))?;
    let mut hash = blake3::Hasher::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hash.finalize().to_hex().to_string())
}

pub fn regular(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "Expected regular file: {}",
        path.display()
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            meta.file_attributes() & 0x400 == 0,
            "Reparse point is not permitted: {}",
            path.display()
        );
    }
    Ok(())
}

fn no_reparse(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        !meta.file_type().is_symlink(),
        "Symlink is not permitted: {}",
        path.display()
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            meta.file_attributes() & 0x400 == 0,
            "Reparse point is not permitted: {}",
            path.display()
        );
    }
    Ok(())
}
fn contained(root: &Path, relative: &Path) -> Result<()> {
    safe_relative(relative)?;
    let mut path = root.to_owned();
    for part in relative.components() {
        path.push(part);
        if path.try_exists()? {
            no_reparse(&path)?;
        }
    }
    Ok(())
}

pub fn safe_relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path.components().all(|c| matches!(c, Component::Normal(_))),
        "Unsafe relative path {}",
        path.display()
    );
    ensure!(
        !path.to_string_lossy().contains(':'),
        "Invalid relative path"
    );
    Ok(())
}

pub fn lease(root: &Path, exclusive: bool) -> Result<File> {
    fs::create_dir_all(root.join(".cpuv"))?;
    no_reparse(&root.join(".cpuv"))?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".cpuv/project.lock"))?;
    if exclusive {
        file.try_lock_exclusive()
    } else {
        FileExt::try_lock_shared(&file)
    }
    .context("Another cpuv operation is using this environment; retry when it finishes")?;
    if exclusive {
        recover(root)?;
    } else {
        let journal = root.join(".cpuv/transaction.json");
        if journal.exists() {
            let journal: Journal = serde_json::from_slice(&fs::read(journal)?)?;
            ensure!(
                journal.complete,
                "Interrupted environment update; run cpuv sync to recover before building"
            );
            validate_journal(root, &journal)?;
        }
    }
    Ok(file)
}

pub type Inventory = BTreeMap<PathBuf, String>;
pub fn inventory(root: &Path) -> Result<Inventory> {
    let mut entries = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root).sort_by_file_name() {
        let entry = entry?;
        no_reparse(entry.path())?;
        ensure!(
            !entry.file_type().is_symlink(),
            "Symlink in environment inventory"
        );
        if entry.file_type().is_file() {
            regular(entry.path())?;
            let relative = entry.path().strip_prefix(root)?;
            if relative == Path::new("files.json") {
                continue;
            }
            entries.insert(relative.to_owned(), digest(entry.path())?);
        }
    }
    Ok(entries)
}
pub fn seal(root: &Path) -> Result<()> {
    atomic_write(
        &root.join("files.json"),
        &serde_json::to_vec_pretty(&inventory(root)?)?,
    )
}
pub fn verify(root: &Path) -> Result<()> {
    let expected: Inventory = serde_json::from_slice(&fs::read(root.join("files.json"))?)?;
    ensure!(
        expected == inventory(root)?,
        "Environment files changed; run cpuv sync to repair"
    );
    Ok(())
}

/// Every file relevant to the selected compiler is compared by content before use.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolIdentity {
    pub files: Inventory,
    #[serde(default)]
    pub roots: Vec<PathBuf>,
}
impl ToolIdentity {
    pub fn capture(paths: impl IntoIterator<Item = PathBuf>) -> Result<Self> {
        let mut files = BTreeMap::new();
        for path in paths {
            regular(&path)?;
            let hash = digest(&path)?;
            files.insert(path, hash);
        }
        ensure!(
            !files.is_empty(),
            "Tool identity must include at least the compiler"
        );
        Ok(Self {
            files,
            roots: Vec::new(),
        })
    }
    pub fn capture_with_roots(
        paths: impl IntoIterator<Item = PathBuf>,
        roots: Vec<PathBuf>,
    ) -> Result<Self> {
        let mut identity = Self::capture(paths)?;
        identity.roots = roots;
        Ok(identity)
    }
    pub fn verify(&self) -> Result<()> {
        ensure!(
            !self.files.is_empty(),
            "Legacy toolchain identity; run cpuv sync"
        );
        for root in &self.roots {
            no_reparse(root)?;
            let mut actual = std::collections::BTreeSet::new();
            for entry in walkdir::WalkDir::new(root) {
                let entry = entry?;
                no_reparse(entry.path())?;
                if entry.file_type().is_file() {
                    actual.insert(entry.path().to_owned());
                }
            }
            let expected: std::collections::BTreeSet<_> = self
                .files
                .keys()
                .filter(|p| p.starts_with(root))
                .cloned()
                .collect();
            ensure!(
                actual == expected,
                "Toolchain SDK file set changed at {}; run cpuv sync",
                root.display()
            );
        }
        for (path, expected) in &self.files {
            regular(path).with_context(|| {
                format!("Toolchain changed at {}; run cpuv sync", path.display())
            })?;
            ensure!(
                digest(path)? == *expected,
                "Toolchain changed at {}; run cpuv sync to rebuild matching dependencies",
                path.display()
            );
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Entry {
    target: PathBuf,
    source: PathBuf,
    existed: bool,
}
#[derive(Serialize, Deserialize)]
struct Journal {
    stage: PathBuf,
    entries: Vec<Entry>,
    complete: bool,
}

fn validate_journal(root: &Path, journal: &Journal) -> Result<PathBuf> {
    safe_relative(&journal.stage)?;
    ensure!(
        journal.stage.components().count() == 1
            && journal.stage.to_string_lossy().starts_with("stage-"),
        "Invalid recovery stage"
    );
    for entry in &journal.entries {
        contained(root, &entry.target)?;
        safe_relative(&entry.source)?;
    }
    let stage = root.join(".cpuv").join(&journal.stage);
    if stage.exists() {
        no_reparse(&stage)?;
        for entry in &journal.entries {
            contained(&stage, &entry.source)?;
        }
        ensure!(
            stage.canonicalize()?.parent() == Some(root.join(".cpuv").canonicalize()?.as_path()),
            "Recovery stage escapes project"
        );
    }
    Ok(stage)
}

/// Caller holds the exclusive lease. A journal is retained on every failed restore.
pub fn recover(root: &Path) -> Result<()> {
    let journal_path = root.join(".cpuv/transaction.json");
    if !journal_path.exists() {
        return Ok(());
    }
    let journal: Journal = serde_json::from_slice(&fs::read(&journal_path)?)?;
    let stage = validate_journal(root, &journal)?;
    ensure!(
        journal.complete || stage.is_dir(),
        "Recovery stage is missing; transaction journal retained for manual recovery"
    );
    let mut failures = Vec::new();
    if !journal.complete {
        for (i, entry) in journal.entries.iter().enumerate().rev() {
            let backup = stage.join(format!("backup-{i}"));
            let target = root.join(&entry.target);
            let restore: Result<()> = (|| {
                if backup.exists() {
                    if target.exists() {
                        fs::rename(&target, stage.join(format!("uncommitted-{i}")))?;
                    }
                    fs::rename(&backup, &target)?;
                } else if !entry.existed && !stage.join(&entry.source).exists() && target.exists() {
                    fs::rename(&target, stage.join(format!("uncommitted-{i}")))?;
                }
                Ok(())
            })();
            if let Err(error) = restore {
                failures.push(format!("{}: {error:#}", target.display()));
            }
        }
    }
    ensure!(
        failures.is_empty(),
        "Incomplete transaction recovery: {}. Backups retained at {}",
        failures.join("; "),
        stage.display()
    );
    fs::remove_file(&journal_path)?;
    if stage.exists() {
        fs::remove_dir_all(stage)?;
    }
    Ok(())
}

/// Stage lives within .cpuv. All old artifacts survive until a durable commit record.
pub fn commit(root: &Path, stage: tempfile::TempDir, targets: &[(&str, &str)]) -> Result<()> {
    commit_with(root, stage, targets, |_| Ok(()))
}
fn commit_with(
    root: &Path,
    stage: tempfile::TempDir,
    targets: &[(&str, &str)],
    mut checkpoint: impl FnMut(usize) -> Result<()>,
) -> Result<()> {
    let entries: Vec<_> = targets
        .iter()
        .map(|(target, source)| Entry {
            target: (*target).into(),
            source: (*source).into(),
            existed: root.join(target).exists(),
        })
        .collect();
    for entry in &entries {
        ensure!(
            stage.path().join(&entry.source).exists(),
            "Missing staged artifact {}",
            entry.source.display()
        );
    }
    let mut journal = Journal {
        stage: stage.path().file_name().context("No stage name")?.into(),
        entries,
        complete: false,
    };
    ensure!(
        stage.path().canonicalize()?.parent() == Some(root.join(".cpuv").canonicalize()?.as_path()),
        "Transaction stage must be in this project's .cpuv directory"
    );
    validate_journal(root, &journal)?;
    let journal_path = root.join(".cpuv/transaction.json");
    ensure!(!journal_path.exists(), "Recover previous transaction first");
    checkpoint(0)?;
    atomic_write(&journal_path, &serde_json::to_vec(&journal)?)?;
    let stage = stage.keep();
    let result: Result<()> = (|| {
        checkpoint(1)?;
        for (i, entry) in journal.entries.iter().enumerate() {
            let target = root.join(&entry.target);
            checkpoint(2 + i * 4)?;
            if entry.existed {
                fs::rename(&target, stage.join(format!("backup-{i}")))?;
            }
            checkpoint(3 + i * 4)?;
            checkpoint(4 + i * 4)?;
            fs::rename(stage.join(&entry.source), &target)?;
            checkpoint(5 + i * 4)?;
        }
        checkpoint(2 + journal.entries.len() * 4)?;
        journal.complete = true;
        atomic_write(&journal_path, &serde_json::to_vec(&journal)?)?;
        Ok(())
    })();
    if let Err(error) = result {
        return match recover(root) {
            Ok(()) => Err(error).context("Update failed; previous project restored"),
            Err(recovery) => Err(error).context(format!("{recovery:#}")),
        };
    }
    if let Err(error) = checkpoint(3 + journal.entries.len() * 4).and_then(|_| recover(root)) {
        eprintln!("warning: update committed; cleanup pending: {error:#}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventory_detects_changes_additions_and_missing_files() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("manifest.json"), "good").unwrap();
        seal(root.path()).unwrap();
        verify(root.path()).unwrap();
        fs::write(root.path().join("manifest.json"), "bad").unwrap();
        assert!(verify(root.path()).is_err());
    }
    #[test]
    fn leases_exclude_writers_but_allow_multiple_readers() {
        let root = tempfile::tempdir().unwrap();
        let first = lease(root.path(), false).unwrap();
        let second = lease(root.path(), false).unwrap();
        assert!(lease(root.path(), true).is_err());
        drop(first);
        drop(second);
        assert!(lease(root.path(), true).is_ok());
    }
    #[test]
    fn interrupted_swap_restores_backup() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".cpuv")).unwrap();
        let stage = tempfile::Builder::new()
            .prefix("stage-")
            .tempdir_in(root.path().join(".cpuv"))
            .unwrap();
        fs::write(stage.path().join("backup-0"), "old").unwrap();
        fs::write(root.path().join("cpuv.lock"), "new").unwrap();
        let journal = Journal {
            stage: stage.path().file_name().unwrap().into(),
            complete: false,
            entries: vec![Entry {
                target: "cpuv.lock".into(),
                source: "cpuv.lock".into(),
                existed: true,
            }],
        };
        atomic_write(
            &root.path().join(".cpuv/transaction.json"),
            &serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        let _ = stage.keep();
        recover(root.path()).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("cpuv.lock")).unwrap(),
            "old"
        );
        recover(root.path()).unwrap();
    }
    #[test]
    fn compiler_content_change_is_detected() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("compiler");
        fs::write(&path, "old").unwrap();
        let identity = ToolIdentity::capture([path.clone()]).unwrap();
        fs::write(&path, "new").unwrap();
        assert!(identity.verify().is_err());
    }
    #[test]
    fn failures_at_each_swap_restore_all_previous_artifacts() {
        for failure in 0..11 {
            let root = tempfile::tempdir().unwrap();
            let _guard = lease(root.path(), true).unwrap();
            fs::write(root.path().join("manifest"), "old manifest").unwrap();
            fs::write(root.path().join("lock"), "old lock").unwrap();
            let stage = tempfile::Builder::new()
                .prefix("stage-")
                .tempdir_in(root.path().join(".cpuv"))
                .unwrap();
            fs::write(stage.path().join("manifest"), "new manifest").unwrap();
            fs::write(stage.path().join("lock"), "new lock").unwrap();
            assert!(
                commit_with(
                    root.path(),
                    stage,
                    &[("manifest", "manifest"), ("lock", "lock")],
                    |point| {
                        ensure!(point != failure, "injected failure");
                        Ok(())
                    }
                )
                .is_err()
            );
            assert_eq!(
                fs::read_to_string(root.path().join("manifest")).unwrap(),
                "old manifest"
            );
            assert_eq!(
                fs::read_to_string(root.path().join("lock")).unwrap(),
                "old lock"
            );
            assert!(!root.path().join(".cpuv/transaction.json").exists());
        }
    }
    #[test]
    #[cfg(windows)]
    fn failed_restore_retains_journal_and_backup_until_file_is_released() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".cpuv")).unwrap();
        let stage = tempfile::Builder::new()
            .prefix("stage-")
            .tempdir_in(root.path().join(".cpuv"))
            .unwrap();
        fs::write(stage.path().join("backup-0"), "old").unwrap();
        let target = root.path().join("cpuv.lock");
        fs::write(&target, "new").unwrap();
        let journal = Journal {
            stage: stage.path().file_name().unwrap().into(),
            complete: false,
            entries: vec![Entry {
                target: "cpuv.lock".into(),
                source: "cpuv.lock".into(),
                existed: true,
            }],
        };
        atomic_write(
            &root.path().join(".cpuv/transaction.json"),
            &serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        let stage = stage.keep();
        let held = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .unwrap();
        assert!(recover(root.path()).is_err());
        assert!(stage.join("backup-0").exists());
        assert!(root.path().join(".cpuv/transaction.json").exists());
        drop(held);
        recover(root.path()).unwrap();
        assert_eq!(fs::read_to_string(target).unwrap(), "old");
    }
    #[test]
    fn sdk_file_additions_change_the_captured_identity() {
        let directory = tempfile::tempdir().unwrap();
        let header = directory.path().join("a.h");
        fs::write(&header, "header").unwrap();
        let identity =
            ToolIdentity::capture_with_roots([header], vec![directory.path().to_owned()]).unwrap();
        identity.verify().unwrap();
        fs::write(directory.path().join("b.h"), "new header").unwrap();
        assert!(identity.verify().is_err());
    }
    #[test]
    #[ignore = "Subprocess fixture invoked by crash_recovery_at_every_transaction_boundary"]
    fn transaction_crash_helper() {
        let root = PathBuf::from(
            std::env::var_os("CPUV_TEST_CRASH_ROOT").expect("Run the parent crash-recovery test"),
        );
        let boundary: usize = std::env::var("CPUV_TEST_CRASH_BOUNDARY")
            .unwrap()
            .parse()
            .unwrap();
        let _guard = lease(&root, true).unwrap();
        let stage = tempfile::Builder::new()
            .prefix("stage-")
            .tempdir_in(root.join(".cpuv"))
            .unwrap();
        fs::create_dir(stage.path().join("env")).unwrap();
        fs::write(stage.path().join("env/marker"), "new").unwrap();
        fs::write(stage.path().join("manifest"), "new").unwrap();
        fs::write(stage.path().join("lock"), "new").unwrap();
        commit_with(
            &root,
            stage,
            &[
                (".cpuv/env", "env"),
                ("cpuv.toml", "manifest"),
                ("cpuv.lock", "lock"),
            ],
            |point| {
                if point == boundary {
                    std::process::exit(77);
                }
                Ok(())
            },
        )
        .unwrap();
        panic!("Crash boundary was never reached");
    }

    #[test]
    fn crash_recovery_at_every_transaction_boundary() {
        for initializing in [false, true] {
            for boundary in 0..=15 {
                let root = tempfile::tempdir().unwrap();
                if !initializing {
                    fs::create_dir_all(root.path().join(".cpuv/env")).unwrap();
                    for path in [".cpuv/env/marker", "cpuv.toml", "cpuv.lock"] {
                        fs::write(root.path().join(path), "old").unwrap();
                    }
                }
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "tests::transaction_crash_helper",
                        "--nocapture",
                    ])
                    .env("CPUV_TEST_CRASH_ROOT", root.path())
                    .env("CPUV_TEST_CRASH_BOUNDARY", boundary.to_string())
                    .output()
                    .unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(77),
                    "boundary {boundary}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                if boundary == 15 {
                    let reader = lease(root.path(), false).unwrap();
                    assert!(root.path().join(".cpuv/transaction.json").is_file());
                    drop(reader);
                }
                let guard = lease(root.path(), true).unwrap();
                let expected = if boundary == 15 { "new" } else { "old" };
                for path in [".cpuv/env/marker", "cpuv.toml", "cpuv.lock"] {
                    if initializing && boundary != 15 {
                        assert!(!root.path().join(path).exists());
                        continue;
                    }
                    assert_eq!(
                        fs::read_to_string(root.path().join(path)).unwrap(),
                        expected,
                        "{path}, boundary {boundary}"
                    );
                }
                assert!(!root.path().join(".cpuv/transaction.json").exists());
                recover(root.path()).unwrap();
                drop(guard);
            }
        }
    }
}
