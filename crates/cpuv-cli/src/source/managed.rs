//! Versioned archive manifests for side-by-side compilers and specialist build tools.
use super::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub version: String,
    pub executable: PathBuf,
    #[serde(default)]
    pub interpreter: Option<String>,
    #[serde(default)]
    pub conan_package: Option<String>,
    #[serde(default)]
    pub version_arguments: Vec<String>,
    pub url: String,
    pub sha256: String,
    pub archive: String,
    #[serde(default = "super::one")]
    pub strip_components: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompilerManifest {
    pub name: String,
    pub target: String,
    pub compiler: Artifact,
}

pub async fn tool(name: &str, artifact: &Artifact, cli: &Cli) -> Result<PathBuf> {
    let spec = Tool {
        version: artifact.version.clone(),
        executable: cpuv_relocate::portable(&artifact.executable),
        conan_package: None,
        source: Archive {
            url: artifact.url.clone(),
            sha256: artifact.sha256.clone(),
            archive: artifact.archive.clone(),
            strip_components: artifact.strip_components,
            include: vec![],
            exclude: vec![],
        },
    };
    let mut registry = Registry {
        tools: BTreeMap::new(),
    };
    registry.tools.insert(name.into(), spec);
    tools::get(
        name,
        "0",
        &registry,
        &cache_dir(cli)?.join("catalog"),
        cli.offline,
        &cli.transfer_limits(),
    )
    .await
}

pub async fn compiler(manifest: &Path, cli: &Cli, select: bool) -> Result<PathBuf> {
    let specification: CompilerManifest = toml::from_str(&fs::read_to_string(manifest)?)
        .context("Invalid compiler archive manifest")?;
    ensure!(
        !specification.name.is_empty()
            && specification
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
        "Invalid toolchain name"
    );
    let compiler = tool(&specification.name, &specification.compiler, cli).await?;
    let tc = cpuv_toolchain::probe(Some(&compiler))?;
    ensure!(
        tc.abi.target_triple == specification.target,
        "Compiler target differs from its manifest"
    );
    ensure!(
        tc.gcc_version == specification.compiler.version,
        "Compiler version differs from its manifest"
    );
    let evidence = cpuv_toolchain::doctor(&tc, &[])?;
    let receipts = cache_dir(cli)?.join("toolchains/receipts");
    fs::create_dir_all(&receipts)?;
    let receipt = receipts.join(format!("{}.json", specification.compiler.sha256));
    cpuv_core::atomic_write(
        &receipt,
        &serde_json::to_vec_pretty(&serde_json::json!({
            "name":specification.name, "version":specification.compiler.version,
            "target":specification.target, "archive_sha256":specification.compiler.sha256,
            "compiler":compiler, "doctor":evidence
        }))?,
    )?;
    // A single archive can expose several target drivers; retain each receipt independently.
    cpuv_core::atomic_write(
        &receipts.join(format!(
            "{}-{}.json",
            specification.compiler.sha256,
            blake3::hash(cpuv_relocate::portable(&specification.compiler.executable).as_bytes())
                .to_hex()
        )),
        &fs::read(&receipt)?,
    )?;
    if select {
        super::super::toolchains::select(&compiler)?;
    }
    println!("{}", serde_json::to_string_pretty(&evidence)?);
    Ok(compiler)
}

pub fn cache_path(cache: &Path, artifact: &Artifact) -> Result<PathBuf> {
    let tool = Tool {
        version: artifact.version.clone(),
        executable: cpuv_relocate::portable(&artifact.executable),
        conan_package: None,
        source: Archive {
            url: artifact.url.clone(),
            sha256: artifact.sha256.clone(),
            archive: artifact.archive.clone(),
            strip_components: artifact.strip_components,
            include: vec![],
            exclude: vec![],
        },
    };
    Ok(cache.join("catalog/tools").join(
        blake3::hash(&serde_json::to_vec(&tool)?)
            .to_hex()
            .to_string(),
    ))
}
