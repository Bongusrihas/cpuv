//! Verified, independently copied environment history. No cache hardlinks are retained.
use super::*;

pub fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(source) {
        let entry = entry?;
        no_reparse(entry.path())?;
        let relative = entry.path().strip_prefix(source)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        safe_relative(relative)?;
        let output = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(output)?;
        } else {
            regular(entry.path())?;
            fs::create_dir_all(output.parent().context("Missing copy destination")?)?;
            fs::copy(entry.path(), &output)?;
            ensure!(
                digest(entry.path())? == digest(&output)?,
                "File changed during snapshot copy"
            );
        }
    }
    Ok(())
}

pub fn retain(root: &Path) -> Result<Option<String>> {
    let env = root.join(".cpuv/env");
    if !env.join("files.json").is_file() {
        return Ok(None);
    }
    // Corruption must remain repairable; never register damaged content as a generation.
    if verify(&env).is_err() {
        return Ok(None);
    }
    let mut identity = blake3::Hasher::new();
    identity.update(&fs::read(env.join("files.json"))?);
    let request = if env.join("request.toml").is_file() {
        env.join("request.toml")
    } else {
        root.join("cpuv.toml")
    };
    if request.is_file() {
        identity.update(&fs::read(&request)?);
    }
    // A user may already have edited/merged the lock. Retain the installed resolution.
    let installed = env.join("installed-lock.toml");
    let lock = if installed.is_file() {
        let payload = fs::read_to_string(&installed)?;
        if locks::read(&root.join("cpuv.lock"))
            .ok()
            .is_some_and(|p| locks::identity(&p).ok() == locks::identity(&payload).ok())
        {
            root.join("cpuv.lock")
        } else {
            installed
        }
    } else {
        root.join("cpuv.lock")
    };
    let toolchain = if env.join("toolchain-state.json").is_file() {
        env.join("toolchain-state.json")
    } else {
        root.join(".cpuv/toolchain.json")
    };
    for path in [&lock, &toolchain] {
        if path.is_file() {
            identity.update(&fs::read(path)?);
        }
    }
    let key = identity.finalize().to_hex().to_string();
    let parent = root.join(".cpuv/generations");
    fs::create_dir_all(&parent)?;
    let target = parent.join(&key);
    if !target.exists() {
        let stage = tempfile::Builder::new()
            .prefix("generation-")
            .tempdir_in(&parent)?;
        copy_tree(&env, &stage.path().join("env"))?;
        for (input, output) in [
            ("cpuv.toml", "cpuv.toml"),
            ("cpuv.lock", "cpuv.lock"),
            (".cpuv/toolchain.json", "toolchain.json"),
            (".cpuv/activate.ps1", "activate.ps1"),
        ] {
            if root.join(input).is_file() {
                fs::copy(root.join(input), stage.path().join(output))?;
            }
        }
        if request.is_file() {
            fs::copy(&request, stage.path().join("cpuv.toml"))?;
        }
        if lock.is_file() {
            fs::copy(&lock, stage.path().join("cpuv.lock"))?;
        }
        if toolchain.is_file() {
            fs::copy(&toolchain, stage.path().join("toolchain.json"))?;
        }
        if root.join(".cpuv/bin").is_dir() {
            copy_tree(&root.join(".cpuv/bin"), &stage.path().join("bin"))?;
        }
        fs::write(
            stage.path().join("generation.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema":1, "identity":key, "project":root
            }))?,
        )?;
        seal(stage.path())?;
        fs::rename(stage.path(), &target)?;
    }
    verify(&target)?;
    Ok(Some(key))
}

pub fn restore(root: &Path, identity: &str) -> Result<()> {
    ensure!(
        identity.len() == 64 && identity.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid generation identity"
    );
    let _guard = lease(root, true)?;
    let source = root.join(".cpuv/generations").join(identity);
    verify(&source)?;
    verify(&source.join("env"))?;
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(source.join("generation.json"))?)?;
    ensure!(
        receipt["project"]
            .as_str()
            .is_some_and(|p| Path::new(p) == root),
        "This generation belongs to a different project location"
    );
    retain(root)?;
    let stage = tempfile::Builder::new()
        .prefix("stage-restore-generation-")
        .tempdir_in(root.join(".cpuv"))?;
    copy_tree(&source, stage.path())?;
    let mut targets = vec![
        (".cpuv/env", "env"),
        ("cpuv.toml", "cpuv.toml"),
        (".cpuv/toolchain.json", "toolchain.json"),
        (".cpuv/activate.ps1", "activate.ps1"),
        (".cpuv/bin", "bin"),
    ];
    ensure!(
        stage.path().join("cpuv.lock").is_file() || !root.join("cpuv.lock").exists(),
        "Generation has no lock; select a generation with a lock before replacing a locked environment"
    );
    if stage.path().join("cpuv.lock").is_file() {
        targets.push(("cpuv.lock", "cpuv.lock"));
    }
    commit(root, stage, &targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generations_retain_installed_locks_after_a_user_lock_edit() {
        let root = tempfile::tempdir().unwrap();
        let env = root.path().join(".cpuv/env");
        fs::create_dir_all(&env).unwrap();
        let installed = "version=1\npackages=[]\n[dependencies]\n";
        fs::write(env.join("installed-lock.toml"), installed).unwrap();
        seal(&env).unwrap();
        fs::write(
            root.path().join("cpuv.lock"),
            "version=1\npackages=[]\nrepository='changed'\n[dependencies]\n",
        )
        .unwrap();
        let key = retain(root.path()).unwrap().unwrap();
        assert_eq!(
            fs::read_to_string(
                root.path()
                    .join(".cpuv/generations")
                    .join(key)
                    .join("cpuv.lock")
            )
            .unwrap(),
            installed
        );
    }
    #[test]
    fn generation_keeps_the_installed_request_when_the_user_edits_the_manifest() {
        let root = tempfile::tempdir().unwrap();
        let env = root.path().join(".cpuv/env");
        fs::create_dir_all(&env).unwrap();
        fs::write(env.join("request.toml"), "installed").unwrap();
        seal(&env).unwrap();
        fs::write(root.path().join("cpuv.toml"), "edited").unwrap();
        let key = retain(root.path()).unwrap().unwrap();
        assert_eq!(
            fs::read_to_string(
                root.path()
                    .join(".cpuv/generations")
                    .join(key)
                    .join("cpuv.toml")
            )
            .unwrap(),
            "installed"
        );
    }
}
