//! Immutable deployed artifacts permit locked offline restoration without build-tree reuse.
use super::*;

pub fn key(payload: &str) -> String {
    blake3::hash(payload.as_bytes()).to_hex().to_string()
}

pub fn location(cache: &Path, payload: &str, request: &str, compiler_identity: &str) -> PathBuf {
    cache.join("environments").join(
        blake3::hash(format!("{}:{request}:{compiler_identity}", key(payload)).as_bytes())
            .to_hex()
            .to_string(),
    )
}

fn snapshot_lock(path: &Path) -> Result<File> {
    cpuv_core::object_lock(
        &path
            .parent()
            .context("Artifact has no parent")?
            .join("locks"),
        path.file_name()
            .and_then(|p| p.to_str())
            .context("Invalid artifact key")?,
    )
}

fn verify_snapshot(path: &Path) -> Result<()> {
    cpuv_core::verify(path)?;
    cpuv_core::verify(&path.join("env"))?;
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("artifact.json"))?)?;
    ensure!(receipt["schema"] == 1, "Unsupported artifact receipt");
    let metadata: cpuv_relocate::Environment =
        serde_json::from_slice(&fs::read(path.join("env/manifest.json"))?)?;
    ensure!(
        metadata.lock_identity.is_some(),
        "Legacy artifact requires migration with cpuv sync"
    );
    Ok(())
}

/// Restore a damaged/missing secondary snapshot from an intact installed environment.
pub fn cached(
    root: &Path,
    cli: &Cli,
    manifest: &Manifest,
    tc: &Toolchain,
    payload: &str,
) -> Result<Option<PathBuf>> {
    let cache = cache_dir(cli)?;
    let path = location(
        &cache,
        payload,
        &request_hash(manifest)?,
        &cpuv_toolchain::identity(tc)?.portable_digest()?,
    );
    {
        let _guard = snapshot_lock(&path)?;
        if path.exists() {
            if verify_snapshot(&path).is_ok() {
                return Ok(Some(path));
            }
            cpuv_core::quarantine::preserve(&path, &cache.join("quarantine/environments"))?;
        }
    }
    let env = root.join(".cpuv/env");
    if cpuv_core::verify(&env).is_ok()
        && let Ok(metadata) = fs::read(env.join("manifest.json"))
            .map_err(anyhow::Error::from)
            .and_then(|data| Ok(serde_json::from_slice::<cpuv_relocate::Environment>(&data)?))
        && metadata.request_hash == request_hash(manifest)?
        && metadata.lock_identity.as_deref() == Some(cpuv_core::locks::identity(payload)?.as_str())
    {
        store(root, cli, payload, &env, tc)?;
        return Ok(Some(path));
    }
    Ok(None)
}

pub fn verify_deployed(env: &Path) -> Result<cpuv_relocate::Environment> {
    cpuv_core::verify(env)?;
    let metadata: cpuv_relocate::Environment =
        serde_json::from_slice(&fs::read(env.join("manifest.json"))?)?;
    metadata
        .tool_identity
        .verify()
        .context("Compiler/SDK changed before artifact publication")?;
    cpuv_shim::bundle::verify_runtime(&metadata, env)?;
    Ok(metadata)
}

fn install_state_format(env: &Path) -> Result<Option<u64>> {
    let path = env.join("install-state.json");
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<serde_json::Value>(&bytes)?[0]
            .as_u64()
            .context("Invalid installation state format")
            .map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn store(root: &Path, cli: &Cli, payload: &str, env: &Path, tc: &Toolchain) -> Result<()> {
    let metadata = verify_deployed(env)?;
    let cache = cache_dir(cli)?;
    let compiler_identity = cpuv_toolchain::identity_from_verified(tc, &metadata.tool_identity)?;
    ensure!(
        compiler_identity
            .files
            .iter()
            .all(|(path, hash)| metadata.tool_identity.files.get(path) == Some(hash)),
        "Artifact compiler contents disagree with the build identity"
    );
    let destination = location(
        &cache,
        payload,
        &metadata.request_hash,
        &compiler_identity.portable_digest()?,
    );
    ensure!(
        metadata.lock_identity.as_deref() == Some(cpuv_core::locks::identity(payload)?.as_str()),
        "Artifact environment does not match its dependency lock"
    );
    let _guard = snapshot_lock(&destination)?;
    if destination.exists() {
        let format = install_state_format(env)?;
        if verify_snapshot(&destination).is_ok()
            && install_state_format(&destination.join("env")).is_ok_and(|saved| saved == format)
        {
            return Ok(());
        }
        cpuv_core::quarantine::preserve(&destination, &cache.join("quarantine/environments"))?;
    }
    fs::create_dir_all(destination.parent().unwrap())?;
    let temporary = tempfile::Builder::new()
        .prefix("environment-")
        .tempdir_in(destination.parent().unwrap())?;
    cpuv_core::generations::copy_tree(env, &temporary.path().join("env"))?;
    fs::write(
        temporary.path().join("artifact.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema":1,"lock_identity":key(payload),"environment":root.join(".cpuv/env"),
            "cache":cache, "toolchain":tc, "compiler_identity":compiler_identity.portable_digest()?, "compiler_files":compiler_identity.files.keys().collect::<Vec<_>>(), "compiler_roots":compiler_identity.roots
        }))?,
    )?;
    metadata
        .tool_identity
        .verify()
        .context("Compiler/SDK changed during artifact publication")?;
    cpuv_core::seal(temporary.path())?;
    if let Err(error) = fs::rename(temporary.path(), &destination) {
        if !destination.is_dir() {
            return Err(error.into());
        }
        cpuv_core::verify(&destination)?;
    }
    Ok(())
}

/// Rebase generated metadata only; binaries and user source/data are never rewritten.
pub fn restore(
    root: &Path,
    cli: &Cli,
    manifest: &Manifest,
    tc: &Toolchain,
    payload: &str,
    source: &Path,
) -> Result<tempfile::TempDir> {
    // Exported transfers are immutable input too; locking is needed only for shared cache objects.
    let cache = cache_dir(cli)?;
    let _guard = if source.parent() == Some(cache.join("environments").as_path()) {
        Some(snapshot_lock(source)?)
    } else {
        None
    };
    verify_snapshot(source)?;
    let compiler_identity = cpuv_toolchain::identity(tc)?;
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(source.join("artifact.json"))?)?;
    ensure!(
        receipt["schema"] == 1 && receipt["lock_identity"] == key(payload),
        "Artifact does not match the selected lock"
    );
    ensure!(
        receipt["compiler_identity"] == compiler_identity.portable_digest()?,
        "Offline artifact requires different compiler/SDK contents"
    );
    let old_env = receipt["environment"]
        .as_str()
        .context("Artifact lacks its original prefix")?;
    let old_cache = receipt["cache"]
        .as_str()
        .context("Artifact lacks its cache prefix")?;
    let new_env = root.join(".cpuv/env");
    let new_cache = cache_dir(cli)?;
    let original: cpuv_relocate::Environment =
        serde_json::from_slice(&fs::read(source.join("env/manifest.json"))?)?;
    ensure!(
        original.lock_identity.as_deref() == Some(cpuv_core::locks::identity(payload)?.as_str()),
        "Artifact environment does not match its dependency lock"
    );
    let mut mappings = vec![
        (PathBuf::from(old_env), new_env.clone()),
        (PathBuf::from(old_cache), new_cache.clone()),
        (original.compiler.clone(), tc.compiler_path.clone()),
        (
            original.compiler_bin.clone(),
            tc.compiler_path.parent().unwrap().to_owned(),
        ),
    ];
    if let Some(saved) = receipt.get("toolchain") {
        let saved: Toolchain = serde_json::from_value(saved.clone())?;
        mappings.extend([
            (saved.c_compiler, tc.c_compiler.clone()),
            (PathBuf::from(saved.linker), PathBuf::from(&tc.linker)),
            (saved.sysroot, tc.sysroot.clone()),
        ]);
    } else if let Some(parent) = original.compiler_bin.parent() {
        mappings.push((parent.to_owned(), tc.sysroot.clone()));
    }
    let roots: Vec<PathBuf> = serde_json::from_value(
        receipt
            .get("compiler_roots")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    mappings.extend(roots.into_iter().zip(compiler_identity.roots.clone()));
    let mut paths = Vec::new();
    for (from, to) in mappings {
        paths.push((cpuv_relocate::portable(&from), cpuv_relocate::portable(&to)));
        paths.push((
            from.to_string_lossy().to_string(),
            to.to_string_lossy().to_string(),
        ));
    }
    paths.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    let replace = |text: String| rebase(&text, &paths);
    let stage = tempfile::Builder::new()
        .prefix("stage-restore-artifact-")
        .tempdir_in(root.join(".cpuv"))?;
    let env = stage.path().join("env");
    cpuv_core::generations::copy_tree(&source.join("env"), &env)?;
    for entry in walkdir::WalkDir::new(&env) {
        let entry = entry?;
        if entry.file_type().is_file()
            && (entry
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("cmake"))
                || cpuv_relocate::linking::is_pkg_config_module(entry.path().strip_prefix(&env)?))
        {
            let text = fs::read_to_string(entry.path())?;
            fs::write(entry.path(), replace(text))?;
        }
    }
    let mut metadata: cpuv_relocate::Environment =
        serde_json::from_slice(&fs::read(env.join("manifest.json"))?)?;
    ensure!(
        metadata.request_hash == request_hash(manifest)?,
        "Offline artifact request does not match this manifest"
    );
    let old_identity = metadata.tool_identity;
    let mut identity = compiler_identity;
    ensure!(
        receipt["compiler_identity"] == identity.portable_digest()?,
        "Compiler/SDK changed during offline restoration"
    );
    let compiler_files: Vec<PathBuf> = serde_json::from_value(
        receipt
            .get("compiler_files")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    let compiler_roots: Vec<PathBuf> = serde_json::from_value(
        receipt
            .get("compiler_roots")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    for (path, hash) in old_identity.files {
        if compiler_files.contains(&path) {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(old_cache) {
            identity.files.insert(new_cache.join(relative), hash);
        }
    }
    for path in old_identity.roots {
        if compiler_roots.contains(&path) {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(old_cache) {
            identity.roots.push(new_cache.join(relative));
        }
    }
    identity.verify()?;
    metadata.tool_identity = identity;
    metadata.compiler = tc.compiler_path.clone();
    metadata.compiler_bin = tc.compiler_path.parent().unwrap().to_owned();
    metadata.process_environment = tc.environment.clone();
    maintenance::record_toolchain(&env, tc)?;
    for values in [
        &mut metadata.include_dirs,
        &mut metadata.library_dirs,
        &mut metadata.cflags,
        &mut metadata.link_items,
        &mut metadata.link_flags,
    ] {
        for value in values {
            *value = replace(value.clone());
        }
    }
    fs::write(
        env.join("manifest.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    fs::write(env.join("request.toml"), toml::to_string_pretty(manifest)?)?;
    fs::write(
        env.join("restoration.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema":1,"lock_identity":key(payload),"original_environment":old_env,"restored_environment":new_env
        }))?,
    )?;
    cpuv_core::seal(&env)?;
    fs::write(
        stage.path().join("cpuv.toml"),
        toml::to_string_pretty(manifest)?,
    )?;
    cpuv_core::locks::write(
        root,
        &stage.path().join("cpuv.lock"),
        payload,
        &request_hash(manifest)?,
    )?;
    stage_auxiliary(stage.path(), root, tc)?;
    Ok(stage)
}

pub fn commit(root: &Path, stage: tempfile::TempDir) -> Result<()> {
    cpuv_core::commit(
        root,
        stage,
        &[
            (".cpuv/env", "env"),
            ("cpuv.toml", "cpuv.toml"),
            ("cpuv.lock", "cpuv.lock"),
            (".cpuv/toolchain.json", "toolchain.json"),
            (".cpuv/activate.ps1", "activate.ps1"),
            (".cpuv/bin", "bin"),
        ],
    )
}

// Perform one substitution pass: a destination nested under an old prefix must not be rebased twice.
fn rebase(text: &str, paths: &[(String, String)]) -> String {
    let mut output = String::new();
    let mut index = 0;
    while index < text.len() {
        if let Some((from, to)) = paths
            .iter()
            .find(|(from, _)| !from.is_empty() && text[index..].starts_with(from))
        {
            output.push_str(to);
            index += from.len();
        } else {
            let next = text[index..].chars().next().unwrap();
            output.push(next);
            index += next.len_utf8();
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn damaged_snapshots_are_preserved_and_rebuilt_from_the_active_environment() {
        let root = tempfile::tempdir().unwrap();
        let compiler = root.path().join("compiler.exe");
        fs::write(&compiler, "compiler").unwrap();
        let tc: Toolchain = serde_json::from_value(serde_json::json!({
            "compiler_path":compiler,"c_compiler":compiler,"driver":"msvc","environment":{},"sysroot":root.path(),
            "gcc_version":"19.51","lto_compression":[],"abi_hash":"abi","provider":null,"linker":compiler,"observed_abi_options":{},"diagnostics":[],
            "abi":{"schema":"test","os":"windows","arch":"x86_64","target_triple":"test","compiler_family":"msvc","compiler_abi_major":"195","stdlib":"msvc","stdlib_release":"145","glibcxx_cxx11_abi":"none","crt_runtime":"msvc","thread_model":"win32","exception_model":"seh","cxx_std_ceiling":"default","build_type":"release"}
        })).unwrap();
        let manifest: Manifest = toml::from_str("[project]\nname='test'\nversion='1'\nstd='default'\n[toolchain]\ntarget='test'\ncompiler='msvc@19.51'\nabi='msvc'\n").unwrap();
        let env = root.path().join(".cpuv/env");
        fs::create_dir_all(&env).unwrap();
        fs::write(env.join("manifest.json"),serde_json::to_vec(&serde_json::json!({
            "schema":1,"compiler":compiler,"compiler_bin":root.path(),"abi_hash":"abi","standard":"default","request_hash":request_hash(&manifest).unwrap(),
            "tool_identity":cpuv_toolchain::identity(&tc).unwrap(),"include_dirs":[],"libraries":[],"defines":[],"cflags":[],"include_map":{}
        })).unwrap()).unwrap();
        let payload = "version=1\npackages=[]\n[dependencies]\n";
        maintenance::bind_lock(&env, payload).unwrap();
        let cache = root.path().join("cache");
        let cli = Cli::parse_from([
            OsString::from("cpuv"),
            "--cache-dir".into(),
            cache.clone().into_os_string(),
            "--offline".into(),
            "sync".into(),
        ]);
        store(root.path(), &cli, payload, &env, &tc).unwrap();
        let cache = cpuv_toolchain::driver_path(cache.canonicalize().unwrap());
        let snapshot = location(
            &cache,
            payload,
            &request_hash(&manifest).unwrap(),
            &cpuv_toolchain::identity(&tc)
                .unwrap()
                .portable_digest()
                .unwrap(),
        );
        for path in ["unexpected.txt", "artifact.json", "env/manifest.json"] {
            fs::write(snapshot.join(path), "damaged").unwrap();
            assert_eq!(
                cached(root.path(), &cli, &manifest, &tc, payload).unwrap(),
                Some(snapshot.clone())
            );
            verify_snapshot(&snapshot).unwrap();
            cpuv_core::verify(&env).unwrap();
        }
        assert_eq!(
            fs::read_dir(cache.join("quarantine/environments"))
                .unwrap()
                .count(),
            3
        );
        let guard = snapshot_lock(&snapshot).unwrap();
        let other = snapshot.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _lock = snapshot_lock(&other).unwrap();
            sender.send(()).unwrap();
        });
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_millis(100))
                .is_err()
        );
        drop(guard);
        receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        worker.join().unwrap();
    }
    #[test]
    fn changed_compiler_is_rejected_before_an_artifact_is_published() {
        let root = tempfile::tempdir().unwrap();
        let compiler = root.path().join("compiler.exe");
        fs::write(&compiler, "old compiler").unwrap();
        let tc:Toolchain=serde_json::from_value(serde_json::json!({
            "compiler_path":compiler,"c_compiler":compiler,"driver":"msvc","environment":{},"sysroot":root.path(),
            "gcc_version":"19.51","lto_compression":[],"abi_hash":"abi","provider":null,"linker":compiler,"observed_abi_options":{},"diagnostics":[],
            "abi":{"schema":"test","os":"windows","arch":"x86_64","target_triple":"test","compiler_family":"msvc","compiler_abi_major":"195","stdlib":"msvc","stdlib_release":"145","glibcxx_cxx11_abi":"none","crt_runtime":"msvc","thread_model":"win32","exception_model":"seh","cxx_std_ceiling":"default","build_type":"release"}
        })).unwrap();
        let identity = cpuv_toolchain::identity(&tc).unwrap();
        let env = root.path().join("env");
        fs::create_dir_all(&env).unwrap();
        fs::write(env.join("manifest.json"),serde_json::to_vec(&serde_json::json!({
            "schema":1,"compiler":compiler,"compiler_bin":root.path(),"abi_hash":"abi","standard":"default","tool_identity":identity,
            "include_dirs":[],"libraries":[],"defines":[],"cflags":[],"include_map":{}
        })).unwrap()).unwrap();
        cpuv_core::seal(&env).unwrap();
        fs::write(&compiler, "new compiler").unwrap();
        let cache = root.path().join("cache");
        let cli = Cli::parse_from([
            std::ffi::OsString::from("cpuv"),
            "--cache-dir".into(),
            cache.clone().into_os_string(),
            "info".into(),
        ]);
        assert!(store(root.path(), &cli, "locked", &env, &tc).is_err());
        assert!(!cache.join("environments").exists());
    }
    #[test]
    fn artifact_cache_separates_compiler_contents_and_requests() {
        let root = Path::new("cache");
        assert_ne!(
            location(root, "lock", "request", "compiler-a"),
            location(root, "lock", "request", "compiler-b")
        );
        assert_ne!(
            location(root, "lock", "request", "compiler-a"),
            location(root, "lock", "other-request", "compiler-a")
        );
        assert_eq!(
            location(root, "lock", "request", "compiler-a"),
            location(root, "lock", "request", "compiler-a")
        );
    }
    #[test]
    fn relocated_compiler_and_nested_cache_prefixes_are_rewritten_once() {
        let mappings = vec![
            (
                "C:/old/tool/bin/g++.exe".into(),
                "C:/old/cache/new/tool/bin/g++.exe".into(),
            ),
            ("C:/old/cache".into(), "C:/old/cache/new".into()),
            ("C:/old/tool".into(), "D:/tool".into()),
        ];
        assert_eq!(
            rebase(
                "set(CXX C:/old/tool/bin/g++.exe)\nset(INCLUDE C:/old/tool/include)\nset(LIB C:/old/cache/lib)",
                &mappings
            ),
            "set(CXX C:/old/cache/new/tool/bin/g++.exe)\nset(INCLUDE D:/tool/include)\nset(LIB C:/old/cache/new/lib)"
        );
    }
}
