use super::*;

pub struct Tools {
    pub cmake: PathBuf,
    pub ninja: PathBuf,
    pub conan: PathBuf,
    pub platform_references: Vec<String>,
}

pub(super) async fn get(
    name: &str,
    minimum: &str,
    registry: &Registry,
    cache: &Path,
    offline: bool,
    limits: &cpuv_fetch::Limits,
) -> Result<PathBuf> {
    ensure!(
        cfg!(any(target_arch = "x86_64", target_arch = "aarch64")),
        "Pinned bootstrap tools require an x64 host or Windows ARM64 with x64 execution support"
    );
    let spec = registry
        .tools
        .get(name)
        .with_context(|| format!("No pinned bootstrap entry for build tool {name}"))?;
    ensure!(
        cpuv_solver::version_cmp(&spec.version, minimum) != std::cmp::Ordering::Less,
        "Build tool {name} is older than required"
    );
    let executable = cpuv_store::safe_relative(Path::new(&spec.executable))?;
    let key = blake3::hash(&serde_json::to_vec(spec)?)
        .to_hex()
        .to_string();
    fs::create_dir_all(cache.join("tools"))?;
    let guard = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache.join("tools").join(format!("{key}.lock")))?;
    guard.lock_exclusive()?;
    cpuv_core::contained(cache, &PathBuf::from("tools").join(&key))?;
    let directory = cache.join("tools").join(&key);
    let manifest = directory.join("files.json");
    if manifest.is_file() {
        let files: BTreeMap<String, String> = serde_json::from_slice(&fs::read(manifest)?)?;
        if archive::verify(&directory.join("prefix"), &files).is_ok() {
            return Ok(directory.join("prefix").join(executable));
        }
        let quarantine = cache.join("quarantine/tools");
        fs::create_dir_all(&quarantine)?;
        let target = quarantine.join(format!(
            "{key}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::rename(&directory, &target)?;
        eprintln!(
            "Preserved changed tool contents at {}; restoring its verified archive",
            target.display()
        );
    }
    let downloaded =
        archive::download(&cache.join("archives"), &spec.source, offline, limits).await?;
    eprintln!(
        "Preparing pinned {name} {} from its verified archive...",
        spec.version
    );
    let parent = cache.join("tools");
    fs::create_dir_all(&parent)?;
    let temporary = tempfile::Builder::new()
        .prefix("tool-")
        .tempdir_in(&parent)?;
    archive::extract(
        &downloaded,
        &spec.source,
        &temporary.path().join("prefix"),
        limits,
    )?;
    ensure!(
        temporary.path().join("prefix").join(&executable).is_file(),
        "Tool archive did not contain {name}"
    );
    fs::write(
        temporary.path().join("files.json"),
        serde_json::to_vec(&archive::inventory(&temporary.path().join("prefix"))?)?,
    )?;
    if let Err(error) = fs::rename(temporary.path(), &directory) {
        if !directory.join("files.json").is_file() {
            return Err(error).context("Committing bootstrapped tool");
        }
        // Another profile may have committed the same pinned tool concurrently.
        let files: BTreeMap<String, String> =
            serde_json::from_slice(&fs::read(directory.join("files.json"))?)?;
        archive::verify(&directory.join("prefix"), &files)?;
        ensure!(
            directory.join("prefix").join(&executable).is_file(),
            "Committed tool archive did not contain {name}"
        );
    }
    eprintln!("Prepared {name} {} in cpuv's cache", spec.version);
    Ok(directory.join("prefix").join(executable))
}

pub async fn prepare(
    registry: &Registry,
    cache: &Path,
    offline: bool,
    limits: &cpuv_fetch::Limits,
) -> Result<Tools> {
    // Independent tool downloads use the same verified archive cache.
    let (cmake, ninja, conan) = tokio::try_join!(
        get("cmake", "3.20", registry, cache, offline, limits),
        get("ninja", "1.10", registry, cache, offline, limits),
        get("conan", "2.33", registry, cache, offline, limits)
    )?;
    let mut platform_references = Vec::new();
    for (name, executable) in [("cmake", &cmake), ("ninja", &ninja), ("conan", &conan)] {
        let spec = &registry.tools[name];
        verify_version(name, spec, executable)?;
        if let Some(package) = &spec.conan_package {
            let reference = format!("{package}/{}", spec.version);
            reference_name(&reference)?;
            platform_references.push(reference);
        }
    }
    Ok(Tools {
        cmake,
        ninja,
        conan,
        platform_references,
    })
}

fn verify_version(name: &str, spec: &Tool, executable: &Path) -> Result<()> {
    // Windows 11 ARM64 can execute the verified x64 bootstrap tools through emulation.
    // Test execution instead of assuming that CPUv and every build tool share an architecture.
    let output = Command::new(executable)
        .arg("--version")
        .output()
        .with_context(|| format!("Running pinned {name}; these tools require x64 execution support (native x64 or Windows 11 ARM64 emulation)"))?;
    ensure!(
        output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .any(|token| token == spec.version),
        "Bootstrapped {name} did not report declared version {}",
        spec.version
    );
    Ok(())
}

pub async fn build_tools(
    registry: &Registry,
    cache: &Path,
    offline: bool,
    limits: &cpuv_fetch::Limits,
) -> Result<(PathBuf, PathBuf)> {
    let (cmake, ninja) = tokio::try_join!(
        get("cmake", "3.20", registry, cache, offline, limits),
        get("ninja", "1.10", registry, cache, offline, limits)
    )?;
    verify_version("cmake", &registry.tools["cmake"], &cmake)?;
    verify_version("ninja", &registry.tools["ninja"], &ninja)?;
    Ok((cmake, ninja))
}
