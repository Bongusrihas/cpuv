use super::*;

pub struct Tools {
    pub cmake: PathBuf,
    pub ninja: PathBuf,
    pub conan: PathBuf,
    pub platform_references: Vec<String>,
}

/// Resolve an override before touching the default archive cache.
pub(crate) async fn selected(
    name: &str,
    cli: &Cli,
    artifacts: &BTreeMap<String, managed::Artifact>,
) -> Result<PathBuf> {
    let minimum = match name {
        "cmake" => "3.20",
        "ninja" => "1.10",
        "conan" => "2.33",
        _ => bail!("Unknown bootstrap tool {name}"),
    };
    if let Some(artifact) = artifacts.get(name) {
        ensure!(
            artifact.interpreter.is_none(),
            "{name} must be a native executable; interpreter launchers are available for specialist tools"
        );
        ensure!(
            cpuv_solver::version_cmp(&artifact.version, minimum) != std::cmp::Ordering::Less,
            "Build tool {name} is older than required {minimum}"
        );
        let executable = managed::tool(name, artifact, cli).await?;
        let output = Command::new(&executable)
            .args(if artifact.version_arguments.is_empty() {
                vec!["--version".to_owned()]
            } else {
                artifact.version_arguments.clone()
            })
            .bounded_output()?;
        ensure!(
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .any(|v| v == artifact.version),
            "Build tool {name} did not report declared version {}",
            artifact.version
        );
        return Ok(executable);
    }
    let registry: Registry = toml::from_str(include_str!("../../../../build-tools.toml"))?;
    let executable = get(
        name,
        minimum,
        &registry,
        &cache_dir(cli)?.join("catalog"),
        cli.offline,
        &cli.transfer_limits(),
    )
    .await?;
    verify_version(name, &registry.tools[name], &executable)?;
    Ok(executable)
}

pub(crate) async fn selected_build_tools(
    cli: &Cli,
    artifacts: &BTreeMap<String, managed::Artifact>,
) -> Result<(PathBuf, PathBuf)> {
    tokio::try_join!(
        selected("cmake", cli, artifacts),
        selected("ninja", cli, artifacts)
    )
}

pub(super) async fn selected_tools(
    cli: &Cli,
    artifacts: &BTreeMap<String, managed::Artifact>,
) -> Result<Tools> {
    let (cmake, ninja, conan) = tokio::try_join!(
        selected("cmake", cli, artifacts),
        selected("ninja", cli, artifacts),
        selected("conan", cli, artifacts)
    )?;
    let registry: Registry = toml::from_str(include_str!("../../../../build-tools.toml"))?;
    let mut platform_references = Vec::new();
    for name in ["cmake", "ninja", "conan"] {
        if !artifacts.contains_key(name)
            && let Some(package) = &registry.tools[name].conan_package
        {
            platform_references.push(format!("{package}/{}", registry.tools[name].version));
        }
    }
    Ok(Tools {
        cmake,
        ninja,
        conan,
        platform_references,
    })
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
    let parent = cache.join("tools");
    let lock_key = key.clone();
    let _guard =
        tokio::task::spawn_blocking(move || cpuv_core::object_lock(&parent, &lock_key)).await??;
    cpuv_core::contained(cache, &PathBuf::from("tools").join(&key))?;
    let directory = cache.join("tools").join(&key);
    let manifest = directory.join("files.json");
    if directory.exists() {
        let valid: Result<()> = (|| {
            cpuv_core::regular(&manifest)?;
            let files: BTreeMap<String, String> = serde_json::from_slice(&fs::read(&manifest)?)?;
            archive::verify(&directory.join("prefix"), &files)?;
            cpuv_core::regular(&directory.join("prefix").join(&executable))
        })();
        if valid.is_ok() {
            return Ok(directory.join("prefix").join(executable));
        }
        cpuv_core::quarantine::preserve(&directory, &cache.join("quarantine/tools"))?;
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

fn verify_version(name: &str, spec: &Tool, executable: &Path) -> Result<()> {
    // Windows 11 ARM64 can execute the verified x64 bootstrap tools through emulation.
    // Test execution instead of assuming that CPUv and every build tool share an architecture.
    let output = Command::new(executable)
        .arg("--version")
        .bounded_output()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture(cache: &Path, bytes: &[u8]) -> Registry {
        use sha2::{Digest, Sha256};
        let mut tar = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, "tool.exe", bytes).unwrap();
        let tar = tar.into_inner().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&tar).unwrap();
        let bytes = encoder.finish().unwrap();
        let hash = format!("{:x}", Sha256::digest(&bytes));
        fs::create_dir_all(cache.join("archives")).unwrap();
        fs::write(cache.join("archives").join(&hash), bytes).unwrap();
        Registry {
            tools: BTreeMap::from([(
                "fixture".into(),
                Tool {
                    version: "3.99".into(),
                    executable: "tool.exe".into(),
                    conan_package: None,
                    source: Archive {
                        url: "http://127.0.0.1:1/tool.tar.gz".into(),
                        sha256: hash,
                        archive: "tar.gz".into(),
                        strip_components: 0,
                        include: vec![],
                        exclude: vec![],
                    },
                },
            )]),
        }
    }

    #[tokio::test]
    async fn missing_malformed_and_changed_tool_receipts_repair_offline() {
        let root = tempfile::tempdir().unwrap();
        let registry = fixture(root.path(), b"verified tool");
        let limits = cpuv_fetch::Limits::default();
        let executable = get("fixture", "1", &registry, root.path(), true, &limits)
            .await
            .unwrap();
        let directory = executable.parent().unwrap().parent().unwrap();
        for failure in ["malformed", "missing", "changed", "unexpected"] {
            match failure {
                "malformed" => fs::write(directory.join("files.json"), "{").unwrap(),
                "missing" => fs::remove_file(directory.join("files.json")).unwrap(),
                "changed" => fs::write(&executable, "changed tool").unwrap(),
                _ => fs::write(directory.join("prefix/extra.dll"), "unexpected").unwrap(),
            }
            get("fixture", "1", &registry, root.path(), true, &limits)
                .await
                .unwrap();
            assert_eq!(fs::read(&executable).unwrap(), b"verified tool");
        }
        assert_eq!(
            fs::read_dir(root.path().join("quarantine/tools"))
                .unwrap()
                .count(),
            4
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn custom_bootstrap_works_without_default_archives() {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("catalog");
        let executable =
            PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/cmd.exe");
        let registry = fixture(&cache, &fs::read(executable).unwrap());
        let tool = &registry.tools["fixture"];
        let artifact = managed::Artifact {
            version: tool.version.clone(),
            executable: PathBuf::from("tool.exe"),
            interpreter: None,
            conan_package: None,
            version_arguments: vec!["/d".into(), "/c".into(), "echo 3.99".into()],
            url: tool.source.url.clone(),
            sha256: tool.source.sha256.clone(),
            archive: "tar.gz".into(),
            strip_components: 0,
        };
        let cli = Cli::parse_from([
            OsString::from("cpuv"),
            "--cache-dir".into(),
            root.path().as_os_str().into(),
            "--offline".into(),
            "build".into(),
        ]);
        let selected = selected("cmake", &cli, &BTreeMap::from([("cmake".into(), artifact)]))
            .await
            .unwrap();
        assert!(selected.is_file());
        assert_eq!(
            fs::read_dir(cache.join("archives"))
                .unwrap()
                .filter(|e| e.as_ref().unwrap().file_type().unwrap().is_file())
                .count(),
            1
        );
    }
}
