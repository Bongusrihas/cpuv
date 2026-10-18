//! Remove only the invoked application and identifiable per-user companions.
use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Args, Debug)]
pub struct Options {
    /// Preview file and PATH changes without modifying the installation.
    #[arg(long)]
    pub dry_run: bool,
}

fn same_path(a: &Path, b: &Path) -> bool {
    let normalize = |p: &Path| {
        cpuv_toolchain::driver_path(p.canonicalize().unwrap_or_else(|_| p.to_owned()))
            .to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    normalize(a) == normalize(b)
}

fn regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file() && !metadata.is_symlink())
}

struct Plan {
    executable: PathBuf,
    files: Vec<PathBuf>,
    path_dirs: Vec<PathBuf>,
    shared_dirs: Vec<PathBuf>,
    skipped: Vec<PathBuf>,
}

fn plan() -> Result<Plan> {
    let executable = cpuv_toolchain::driver_path(std::env::current_exe()?.canonicalize()?);
    let directory = executable
        .parent()
        .context("Executable has no installation directory")?;
    let known: Vec<PathBuf> = [
        std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join(".local/bin")),
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("cpuv/bin")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut directories = vec![directory.to_owned()];
    let mut files = vec![executable.clone()];
    let mut skipped = Vec::new();
    // A custom/portable invocation affects that installation only. Do not scan
    // PATH or execute arbitrary other binaries to identify them.
    if known.iter().any(|p| same_path(p, directory)) {
        let digest = Sha256::digest(fs::read(&executable)?);
        for candidate in known {
            if same_path(&candidate, directory) {
                continue;
            }
            let other = candidate.join("cpuv.exe");
            if !regular(&other) {
                continue;
            }
            if Sha256::digest(fs::read(&other)?) == digest {
                directories.push(candidate);
                files.push(other);
            } else {
                skipped.push(other);
            }
        }
    }
    for directory in &directories {
        let shim = directory.join("cpuv-shim.exe");
        if regular(&shim) {
            let bytes = fs::read(&shim)?;
            if bytes.starts_with(b"MZ")
                && bytes
                    .windows(b"cpuv-shim:".len())
                    .any(|p| p == b"cpuv-shim:")
                && bytes
                    .windows(b"No cpuv environment".len())
                    .any(|p| p == b"No cpuv environment")
            {
                files.push(shim);
            }
        }
        let launcher = directory.join("cpuv.cmd");
        if regular(&launcher)
            && fs::read_to_string(&launcher).is_ok_and(|s| s.contains("rem cpuv command launcher"))
        {
            files.push(launcher);
        }
    }
    let mut path_dirs = Vec::new();
    let mut shared_dirs = Vec::new();
    for directory in directories {
        let shared = fs::read_dir(&directory)?.try_fold(false, |found, entry| -> Result<bool> {
            let path = entry?.path();
            Ok(found
                || (path
                    .file_name()
                    .is_none_or(|name| name != ".cpuv-self-update.lock")
                    && !(regular(&path) && files.iter().any(|owned| same_path(owned, &path)))))
        })?;
        if shared {
            shared_dirs.push(directory);
        } else {
            path_dirs.push(directory);
        }
    }
    Ok(Plan {
        executable,
        files,
        path_dirs,
        shared_dirs,
        skipped,
    })
}

fn restore(files: &[(PathBuf, PathBuf)], stages: Vec<tempfile::TempDir>) -> Result<()> {
    let mut errors = Vec::new();
    for (original, backup) in files.iter().rev() {
        if let Err(error) = fs::rename(backup, original) {
            errors.push(format!("{}: {error}", original.display()));
        }
    }
    if !errors.is_empty() {
        let recovery: Vec<_> = stages
            .into_iter()
            .map(|stage| stage.keep().display().to_string())
            .collect();
        bail!(
            "Uninstall restoration was incomplete: {}. Preserved backups: {}",
            errors.join("; "),
            recovery.join(", ")
        );
    }
    Ok(())
}

pub fn run(options: &Options) -> Result<i32> {
    ensure!(
        cfg!(windows),
        "Application uninstall currently supports Windows"
    );
    let plan = plan()?;
    for path in &plan.files {
        println!(
            "{} {}",
            if options.dry_run {
                "Would remove"
            } else {
                "Removing"
            },
            path.display()
        );
    }
    for directory in &plan.shared_dirs {
        println!("Keeping shared PATH directory: {}", directory.display());
    }
    for path in &plan.skipped {
        eprintln!("Keeping a different cpuv executable: {}", path.display());
    }
    for directory in &plan.path_dirs {
        println!(
            "{} user PATH entry: {}",
            if options.dry_run {
                "Would remove"
            } else {
                "Removing"
            },
            directory.display()
        );
    }
    if options.dry_run {
        return Ok(0);
    }

    let mut installation_dirs: Vec<_> = plan
        .files
        .iter()
        .map(|file| file.parent().unwrap().to_owned())
        .collect();
    installation_dirs.sort();
    installation_dirs.dedup();
    let mut guards = Vec::new();
    for directory in installation_dirs {
        let guard = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join(".cpuv-self-update.lock"))?;
        guard
            .try_lock_exclusive()
            .context("Another uninstall or self update is in progress")?;
        guards.push(guard);
    }
    let mut stages = Vec::new();
    let mut staged = Vec::new();
    let preparation: Result<()> = (|| {
        for file in plan.files.iter().filter(|file| *file != &plan.executable) {
            let stage = tempfile::Builder::new()
                .prefix(".cpuv-uninstall-")
                .tempdir_in(file.parent().unwrap())?;
            let backup = stage.path().join(file.file_name().unwrap());
            stages.push(stage);
            fs::rename(file, &backup).with_context(|| {
                format!(
                    "Preparing removal of {}; close running cpuv processes and retry",
                    file.display()
                )
            })?;
            staged.push((file.clone(), backup));
        }
        Ok(())
    })();
    if let Err(error) = preparation {
        restore(&staged, stages)?;
        return Err(error);
    }
    #[cfg(windows)]
    let path_edit = match crate::install::windows::remove_from_user_path(&plan.path_dirs) {
        Ok(edit) => edit,
        Err(error) => {
            restore(&staged, stages)?;
            return Err(error);
        }
    };
    // self-replace provides native Windows deletion of the running image after
    // this process exits. No PowerShell or Rust installation is needed.
    if let Err(error) = self_replace::self_delete() {
        #[cfg(windows)]
        let path_result = path_edit.map_or(Ok(()), |edit| edit.restore());
        let file_result = restore(&staged, stages);
        #[cfg(windows)]
        path_result.context("Restoring user PATH after failed uninstall")?;
        file_result?;
        return Err(error).context("Scheduling removal of the running cpuv executable");
    }
    for stage in stages {
        let path = stage.keep();
        // Only remove the known files we staged; never recursively remove an
        // installation directory, which may contain unrelated tools or data.
        for (_, backup) in staged
            .iter()
            .filter(|(_, file)| file.parent() == Some(path.as_path()))
        {
            if let Err(error) = fs::remove_file(backup) {
                eprintln!("Cleanup pending at {}: {error}", backup.display());
            }
        }
        if let Err(error) = fs::remove_dir(&path) {
            eprintln!("Retained cleanup directory {}: {error}", path.display());
        }
    }
    println!("cpuv uninstalled. Project environments and package caches were preserved.");
    println!("Open a new terminal to refresh PATH.");
    Ok(0)
}
