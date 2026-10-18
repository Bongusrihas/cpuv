use super::*;

pub struct Tools {
    pub cmake: PathBuf,
    pub ninja: PathBuf,
    pub conan: PathBuf,
}

async fn get(
    name: &str,
    minimum: &str,
    registry: &Registry,
    cache: &Path,
    offline: bool,
    limits: &cpuv_fetch::Limits,
) -> Result<PathBuf> {
    ensure!(
        cfg!(target_arch = "x86_64"),
        "Automatic build-tool bootstrap currently needs x64 cpuv"
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
    let directory = cache.join("tools").join(&key);
    let manifest = directory.join("files.json");
    if manifest.is_file() {
        let files: BTreeMap<String, String> = serde_json::from_slice(&fs::read(manifest)?)?;
        archive::verify(&directory.join("prefix"), &files)?;
        return Ok(directory.join("prefix").join(executable));
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
    Ok(Tools {
        cmake,
        ninja,
        conan,
    })
}
