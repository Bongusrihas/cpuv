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

pub fn store(root: &Path, cli: &Cli, payload: &str, env: &Path, tc: &Toolchain) -> Result<()> {
    cpuv_core::verify(env)?;
    let cache = cache_dir(cli)?;
    let metadata: cpuv_relocate::Environment =
        serde_json::from_slice(&fs::read(env.join("manifest.json"))?)?;
    metadata
        .tool_identity
        .verify()
        .context("Compiler/SDK changed before artifact publication")?;
    cpuv_shim::bundle::verify_runtime(&metadata, env)?;
    let compiler_identity = cpuv_toolchain::identity(tc)?;
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
    if destination.exists() {
        cpuv_core::verify(&destination)?;
        return Ok(());
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
    cpuv_core::verify(source)?;
    cpuv_core::verify(&source.join("env"))?;
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(source.join("artifact.json"))?)?;
    ensure!(
        receipt["schema"] == 1 && receipt["lock_identity"] == key(payload),
        "Artifact does not match the selected lock"
    );
    ensure!(
        receipt["compiler_identity"] == cpuv_toolchain::identity(tc)?.portable_digest()?,
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
    mappings.extend(roots.into_iter().zip(cpuv_toolchain::identity(tc)?.roots));
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
            && entry
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("cmake") || e.eq_ignore_ascii_case("pc"))
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
    let mut identity = cpuv_toolchain::identity(tc)?;
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
