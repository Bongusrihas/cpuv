//! Conservative, reference-aware collection of CPUv-owned immutable cache entries.
//! Provider databases, installed projects and recovery backups are never collected.
use super::*;

#[derive(Subcommand)]
pub enum Action {
    Inspect,
    /// Preview expired CPUv build logs. Recovery data, provider databases and generations are retained.
    Logs {
        #[arg(long)]
        apply: bool,
        #[arg(long, default_value_t = 30, value_parser=clap::value_parser!(u64).range(1..=36500))]
        older_than_days: u64,
    },
    /// Preview unused immutable objects. --apply performs the listed deletions.
    Prune {
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        quota_mib: Option<u64>,
    },
    /// Stop protecting a project which has intentionally been removed.
    Forget {
        project: PathBuf,
    },
}

fn log_name(path: &Path) -> bool {
    if path
        .extension()
        .is_none_or(|e| !e.eq_ignore_ascii_case("log"))
    {
        return false;
    }
    let name = path.file_stem().unwrap_or_default().to_string_lossy();
    [
        "cmake-", "ctest-", "meson-", "make-", "msbuild-", "version-",
    ]
    .iter()
    .any(|p| name.starts_with(p))
        || name.split_once('-').is_some_and(|(pid, time)| {
            !pid.is_empty()
                && !time.is_empty()
                && pid.bytes().chain(time.bytes()).all(|b| b.is_ascii_digit())
        })
}

fn prune_logs(cache: &Path, apply: bool, older_than_days: u64) -> Result<i32> {
    let age = std::time::Duration::from_secs(
        older_than_days
            .checked_mul(86400)
            .context("Log age overflow")?,
    );
    let mut directories = vec![(cache.to_owned(), PathBuf::from("catalog/logs"))];
    directories.extend(
        roots(cache)?
            .into_iter()
            .filter(|p| p.is_dir())
            .map(|p| (p, PathBuf::from(".cpuv/logs"))),
    );
    let mut selected = Vec::new();
    let mut bytes = 0u64;
    for (root, relative) in directories {
        cpuv_core::contained(&root, &relative)?;
        let directory = root.join(relative);
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if !log_name(&entry.path()) || !entry.file_type()?.is_file() {
                continue;
            }
            cpuv_core::regular(&entry.path())?;
            let metadata = entry.metadata()?;
            if metadata
                .modified()?
                .elapsed()
                .is_ok_and(|elapsed| elapsed >= age)
            {
                bytes = bytes.saturating_add(metadata.len());
                selected.push(entry.path());
            }
        }
    }
    selected.sort();
    selected.dedup();
    for path in &selected {
        if apply {
            cpuv_core::regular(path)?;
            fs::remove_file(path)?;
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"schema":1,"kind":"logs","dry_run":!apply,"older_than_days":older_than_days,"selected":selected,"selected_bytes":bytes,"note":"Only recognized CPUv log files are selected. Quarantine, generations, lock backups and interrupted transactions are retained."})
        )?
    );
    Ok(0)
}

pub fn lease(cache: &Path, exclusive: bool) -> Result<File> {
    fs::create_dir_all(cache)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache.join("maintenance.lock"))?;
    if exclusive {
        file.try_lock_exclusive()
    } else {
        FileExt::try_lock_shared(&file)
    }
    .context("Another operation is using the cache; retry after it finishes")?;
    Ok(file)
}

fn roots(cache: &Path) -> Result<Vec<PathBuf>> {
    match fs::read(cache.join("projects.json")) {
        Ok(data) => Ok(serde_json::from_slice(&data)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

pub fn register(cache: &Path, root: &Path) -> Result<()> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache.join("projects.lock"))?;
    lock.lock_exclusive()?;
    let mut roots = roots(cache)?;
    if !roots.iter().any(|p| p == root) {
        roots.push(root.to_owned());
        roots.sort();
        cpuv_core::atomic_write(
            &cache.join("projects.json"),
            &serde_json::to_vec_pretty(&roots)?,
        )?;
    }
    Ok(())
}

fn bytes(path: &Path) -> Result<u64> {
    walkdir::WalkDir::new(path)
        .into_iter()
        .try_fold(0u64, |size, e| {
            let entry = e?;
            if entry.file_type().is_file() {
                cpuv_core::regular(entry.path())?;
                Ok(size + entry.metadata()?.len())
            } else {
                ensure!(
                    !entry.file_type().is_symlink(),
                    "Cache entry contains a link"
                );
                Ok(size)
            }
        })
}

#[derive(Serialize)]
struct Entry {
    path: PathBuf,
    bytes: u64,
    protected: bool,
}

fn verified_object(path: &Path, directory: &str) -> Result<()> {
    if directory == "environments" {
        cpuv_core::verify(path)?;
        cpuv_core::verify(&path.join("env"))?;
        cpuv_core::regular(&path.join("env/manifest.json"))?;
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("artifact.json"))?)?;
        ensure!(receipt["schema"] == 1, "Unknown artifact receipt");
        for field in ["lock_identity", "compiler_identity"] {
            let hash = receipt[field]
                .as_str()
                .context("Missing artifact identity")?;
            ensure!(
                hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid artifact identity"
            );
        }
        Ok(())
    } else {
        source::verify_cache_object(path, directory == "catalog/source-inputs")
    }
}

fn entries(cache: &Path) -> Result<(Vec<Entry>, bool)> {
    let mut retained = Vec::new();
    let mut uncertain = false;
    let mut by_request = BTreeMap::<String, Vec<PathBuf>>::new();
    // Parse each immutable artifact's request only once, independent of project/history count.
    if cache.join("environments").is_dir() {
        for entry in fs::read_dir(cache.join("environments"))? {
            let path = entry?.path();
            let metadata = fs::read(path.join("env/manifest.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
            if let Some(hash) = metadata.as_ref().and_then(|m| m["request_hash"].as_str()) {
                by_request.entry(hash.into()).or_default().push(path);
            }
        }
    }
    for root in roots(cache)? {
        if !root.is_dir() {
            uncertain = true;
            continue;
        }
        let mut environments = vec![root.join(".cpuv/env")];
        for history in [".cpuv/generations", ".cpuv/backups"] {
            if root.join(history).is_dir() {
                for entry in fs::read_dir(root.join(history))? {
                    environments.push(entry?.path().join("env"));
                }
            }
        }
        for env in environments {
            if !env.join("manifest.json").is_file() {
                continue;
            }
            let metadata: cpuv_relocate::Environment =
                serde_json::from_slice(&fs::read(env.join("manifest.json"))?)?;
            retained.extend(metadata.tool_identity.files.keys().cloned());
            // Match all immutable environment copies with this request, including old generations.
            retained.extend(
                by_request
                    .get(&metadata.request_hash)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            let request = env.join("request.toml");
            if request.is_file() {
                let manifest: Manifest = toml::from_str(&fs::read_to_string(request)?)?;
                for dependency in manifest.dependencies.values() {
                    if let Dependency::Detailed {
                        source: Some(spec), ..
                    } = dependency
                    {
                        let key = blake3::hash(&serde_json::to_vec(&spec.input)?)
                            .to_hex()
                            .to_string();
                        retained.push(cache.join("catalog/source-inputs").join(key));
                    }
                }
                // Build tools are required by subsequent project builds, even if they were not used by a package.
                let registry: source::Registry =
                    toml::from_str(include_str!("../../../build-tools.toml"))?;
                for tool in registry.tools.values() {
                    retained.push(
                        cache.join("catalog/tools").join(
                            blake3::hash(&serde_json::to_vec(tool)?)
                                .to_hex()
                                .to_string(),
                        ),
                    );
                }
                for artifact in manifest.tools.values() {
                    retained.push(source::managed::cache_path(cache, artifact)?);
                }
            }
        }
        // An interrupted transaction can still refer to the previous tool/cache closure.
        if root.join(".cpuv/transaction.json").is_file() {
            let journal: serde_json::Value =
                serde_json::from_slice(&fs::read(root.join(".cpuv/transaction.json"))?)?;
            if journal["complete"] != true {
                uncertain = true;
            }
        }
    }
    if cache.join("toolchains/receipts").is_dir() {
        for entry in fs::read_dir(cache.join("toolchains/receipts"))? {
            let receipt: serde_json::Value = serde_json::from_slice(&fs::read(entry?.path())?)?;
            if let Some(path) = receipt["compiler"].as_str() {
                retained.push(PathBuf::from(path));
            }
        }
    }
    let mut entries = Vec::new();
    for directory in ["environments", "catalog/tools", "catalog/source-inputs"] {
        if !cache.join(directory).is_dir() {
            continue;
        }
        for entry in fs::read_dir(cache.join(directory))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !entry.file_type()?.is_dir()
                || name.len() != 64
                || !name.bytes().all(|b| b.is_ascii_hexdigit())
            {
                continue;
            }
            let path = entry.path();
            cpuv_core::contained(cache, path.strip_prefix(cache)?)?;
            entries.push(Entry {
                bytes: bytes(&path)?,
                protected: uncertain
                    || verified_object(&path, directory).is_err()
                    || retained
                        .iter()
                        .any(|p| p.starts_with(&path) || path.starts_with(p)),
                path,
            });
        }
    }
    entries.sort_by_key(|e| fs::metadata(&e.path).and_then(|m| m.modified()).ok());
    Ok((entries, uncertain))
}

pub fn execute(action: &Action, cache: &Path) -> Result<i32> {
    let mutation = matches!(
        action,
        Action::Forget { .. }
            | Action::Prune { apply: true, .. }
            | Action::Logs { apply: true, .. }
    );
    let _lease = lease(cache, mutation)?;
    if let Action::Logs {
        apply,
        older_than_days,
    } = action
    {
        return prune_logs(cache, *apply, *older_than_days);
    }
    if let Action::Forget { project } = action {
        let project = cpuv_toolchain::driver_path(std::path::absolute(project)?);
        let mut roots = roots(cache)?;
        roots.retain(|p| p != &project);
        cpuv_core::atomic_write(
            &cache.join("projects.json"),
            &serde_json::to_vec_pretty(&roots)?,
        )?;
        println!("Removed the cache reference for {}", project.display());
        return Ok(0);
    }
    let (entries, uncertain) = entries(cache)?;
    let total = bytes(cache)?;
    let mut remaining = total;
    let mut selected = Vec::new();
    let protected_bytes: u64 = entries
        .iter()
        .filter(|e| e.protected)
        .map(|e| e.bytes)
        .sum();
    let missing_projects: Vec<_> = roots(cache)?.into_iter().filter(|p| !p.is_dir()).collect();
    if let Action::Prune { apply, quota_mib } = action {
        let quota = quota_mib
            .map(|m| m.saturating_mul(1024 * 1024))
            .unwrap_or(0);
        for entry in &entries {
            if entry.protected || remaining <= quota {
                continue;
            }
            if *apply {
                let target = entry.path.canonicalize()?;
                let base = cache.canonicalize()?;
                ensure!(
                    target.starts_with(&base) && target != base,
                    "Unsafe cache prune target"
                );
                // Only an identified immutable child is deleted, while holding the exclusive cache lease.
                fs::remove_dir_all(&target)?;
            }
            remaining = remaining.saturating_sub(entry.bytes);
            selected.push(&entry.path);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema":1,"cache":cache,"total_bytes":total,"remaining_bytes":remaining,
            "entries":entries,"selected":selected,"dry_run":!matches!(action,Action::Prune {apply:true,..}),
            "conservative_retention":uncertain,"projects":roots(cache)?,
            "protected_candidate_bytes":protected_bytes,"missing_projects":missing_projects,
            "size_accounting":"Logical file lengths, including repeated hardlinks; not physical allocated disk bytes.",
            "missing_project_remedy":"Use cpuv cache forget -- PROJECT only for a project intentionally removed.",
            "note":"Active generations, registered projects, recovery state and provisioned compilers are protected. Unowned or damaged objects are retained. Archives, store objects and provider-owned databases are retained; use provider maintenance for those. A quota cannot evict protected content."
        }))?
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_retention_is_preview_first_and_preserves_unowned_and_recovery_files() {
        let root = tempfile::tempdir().unwrap();
        let logs = root.path().join("catalog/logs");
        fs::create_dir_all(&logs).unwrap();
        let old = logs.join("123-456.log");
        fs::write(&old, "old log").unwrap();
        let stale = std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 86400);
        File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(stale)
            .unwrap();
        let user = logs.join("personal.log");
        fs::write(&user, "keep").unwrap();
        File::options()
            .write(true)
            .open(&user)
            .unwrap()
            .set_modified(stale)
            .unwrap();
        let recovery = root.path().join("quarantine");
        fs::create_dir_all(&recovery).unwrap();
        fs::write(recovery.join("123-456.log"), "keep").unwrap();
        execute(
            &Action::Logs {
                apply: false,
                older_than_days: 2,
            },
            root.path(),
        )
        .unwrap();
        assert!(old.exists());
        execute(
            &Action::Logs {
                apply: true,
                older_than_days: 2,
            },
            root.path(),
        )
        .unwrap();
        assert!(!old.exists());
        assert!(user.exists() && recovery.join("123-456.log").exists());
    }
    #[test]
    fn collection_requires_exclusive_access_and_preserves_missing_project_references() {
        let root = tempfile::tempdir().unwrap();
        let entry = root.path().join("environments").join("a".repeat(64));
        fs::create_dir_all(entry.join("env")).unwrap();
        fs::write(entry.join("env/manifest.json"), "{}").unwrap();
        fs::write(
            entry.join("artifact.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema": 1, "lock_identity": "c".repeat(64), "compiler_identity": "d".repeat(64)
            }))
            .unwrap(),
        )
        .unwrap();
        cpuv_core::seal(&entry.join("env")).unwrap();
        cpuv_core::seal(&entry).unwrap();
        let unrelated = root.path().join("environments").join("b".repeat(64));
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(unrelated.join("user-data"), "keep").unwrap();
        let reader = lease(root.path(), false).unwrap();
        assert!(lease(root.path(), true).is_err());
        drop(reader);
        let protected = || {
            entries(root.path())
                .unwrap()
                .0
                .into_iter()
                .find(|e| e.path == entry)
                .unwrap()
                .protected
        };
        assert!(!protected());
        register(root.path(), &root.path().join("missing-project")).unwrap();
        assert!(protected());
        execute(
            &Action::Prune {
                apply: true,
                quota_mib: None,
            },
            root.path(),
        )
        .unwrap();
        assert!(entry.is_dir());
        execute(
            &Action::Forget {
                project: root.path().join("missing-project"),
            },
            root.path(),
        )
        .unwrap();
        fs::write(entry.join("unexpected-file"), "retain for diagnosis").unwrap();
        execute(
            &Action::Prune {
                apply: true,
                quota_mib: None,
            },
            root.path(),
        )
        .unwrap();
        assert!(entry.is_dir());
        fs::remove_file(entry.join("unexpected-file")).unwrap();
        execute(
            &Action::Prune {
                apply: false,
                quota_mib: None,
            },
            root.path(),
        )
        .unwrap();
        assert!(entry.is_dir());
        execute(
            &Action::Prune {
                apply: true,
                quota_mib: None,
            },
            root.path(),
        )
        .unwrap();
        assert!(!entry.exists());
        assert!(unrelated.join("user-data").is_file());
    }
}
