//! Explicit application-local deployment; never search arbitrary PATH directories.
use super::*;
use std::collections::BTreeMap;

fn locate(directory: &Path, name: &str) -> Option<PathBuf> {
    fs::read_dir(directory)
        .ok()?
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

fn copy_file(
    from: &Path,
    to: &Path,
    receipts: &mut BTreeMap<String, serde_json::Value>,
    stage: &Path,
) -> Result<()> {
    cpuv_core::regular(from)?;
    let digest = cpuv_core::digest(from)?;
    let relative = to.strip_prefix(stage)?;
    cpuv_core::safe_relative(relative)?;
    ensure!(
        !relative
            .to_string_lossy()
            .eq_ignore_ascii_case("cpuv-bundle.json"),
        "Asset conflicts with the deployment receipt"
    );
    let key = relative.to_string_lossy().to_ascii_lowercase();
    if let Some(previous) = receipts.get(&key) {
        ensure!(
            previous["blake3"] == digest,
            "Conflicting deployment file {}",
            relative.display()
        );
        return Ok(());
    }
    fs::create_dir_all(to.parent().context("Missing deployment parent")?)?;
    fs::copy(from, to)?;
    ensure!(
        cpuv_core::digest(to)? == digest,
        "Deployment copy failed verification"
    );
    receipts.insert(
        key,
        serde_json::json!({"path":relative,"source":from,"blake3":digest}),
    );
    Ok(())
}

pub fn export(
    metadata: &Environment,
    env: &Path,
    binary: &Path,
    destination: &Path,
    assets: &[PathBuf],
) -> Result<()> {
    ensure!(
        !destination.exists(),
        "Bundle destination already exists; select a new directory"
    );
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix("cpuv-bundle-")
        .tempdir_in(parent)?;
    let stage = temporary.path();
    let mut receipt = BTreeMap::new();
    let executable = stage.join(binary.file_name().context("Executable must name a file")?);
    copy_file(binary, &executable, &mut receipt, stage)?;
    let mut pending = vec![executable.clone()];
    for asset in assets {
        let name = asset
            .file_name()
            .context("Asset must name a file or directory")?;
        if asset.is_file() {
            let output = stage.join(name);
            copy_file(asset, &output, &mut receipt, stage)?;
            if output
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("dll") || s.eq_ignore_ascii_case("exe"))
            {
                pending.push(output);
            }
        } else {
            let inventory = cpuv_core::inventory(asset)?;
            for relative in inventory.keys() {
                let output = stage.join(name).join(relative);
                copy_file(&asset.join(relative), &output, &mut receipt, stage)?;
                if output
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("dll") || s.eq_ignore_ascii_case("exe"))
                {
                    pending.push(output);
                }
            }
        }
    }
    let main_bytes = fs::read(&executable)?;
    let main = goblin::pe::PE::parse(&main_bytes)
        .context("Bundle entry must be a Windows PE executable")?;
    let machine = main.header.coff_header.machine;
    ensure!(
        !main.is_lib,
        "Bundle entry must be an executable, not a DLL"
    );
    let mut search = vec![env.join("bin"), metadata.compiler_bin.clone()];
    if let Some(parent) = binary.parent() {
        search.push(parent.to_owned());
    }
    if let Some(vs) = metadata.process_environment.get("VSINSTALLDIR") {
        let base = Path::new(vs).join("VC/Redist/MSVC");
        let architecture = match machine {
            0x8664 => "x64",
            0x14c => "x86",
            0xaa64 => "arm64",
            _ => bail!("Unsupported bundle architecture"),
        };
        let mut versions: Vec<_> = fs::read_dir(base)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        versions.sort();
        versions.reverse();
        if let Some(version) = versions.into_iter().find(|v| v.join(architecture).is_dir()) {
            for entry in fs::read_dir(version.join(architecture))? {
                let entry = entry?;
                if entry.file_type()?.is_dir()
                    && entry.file_name().to_string_lossy().ends_with(".CRT")
                {
                    search.push(entry.path());
                }
            }
        }
    }
    let system = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .context("SystemRoot is unavailable")?
        .join(if machine == 0x14c && cfg!(target_arch = "x86_64") {
            "SysWOW64"
        } else {
            "System32"
        });
    let mut seen = BTreeSet::new();
    let mut system_imports = BTreeSet::new();
    while let Some(path) = pending.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let bytes = fs::read(&path)?;
        let pe = goblin::pe::PE::parse(&bytes)?;
        ensure!(
            pe.header.coff_header.machine == machine,
            "Bundle architecture mismatch in {}",
            path.display()
        );
        ensure!(
            pe.header
                .optional_header
                .as_ref()
                .is_none_or(|h| h.data_directories.get_delay_import_descriptor().is_none()),
            "Delay-load imports in {} need a provider runtime plan; automatic export is not supported for this binary",
            path.display()
        );
        for import in pe.libraries {
            cpuv_core::safe_relative(Path::new(import))?;
            ensure!(!import.contains(['/', '\\']), "Invalid DLL import name");
            let key = import.to_ascii_lowercase();
            let source = search.iter().find_map(|dir| locate(dir, import));
            if let Some(source) = source {
                let output = stage.join(import);
                copy_file(&source, &output, &mut receipt, stage)?;
                pending.push(output);
            } else if let Some(local) =
                locate(path.parent().unwrap_or(stage), import).or_else(|| locate(stage, import))
            {
                pending.push(local);
            } else if key.starts_with("api-ms-win-")
                || key.starts_with("ext-ms-win-")
                || system.join(import).is_file()
            {
                system_imports.insert(key);
            } else {
                bail!(
                    "Missing runtime DLL {import}, imported by {}; provide it with --asset or install its dependency in this profile",
                    path.display()
                );
            }
        }
    }
    for entry in walkdir::WalkDir::new(env).sort_by_file_name() {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if entry.file_type().is_file()
            && (name.starts_with("license") || name.starts_with("copying") || name == "copyright")
        {
            copy_file(
                entry.path(),
                &stage.join("licenses").join(entry.path().strip_prefix(env)?),
                &mut receipt,
                stage,
            )?;
        }
    }
    fs::write(
        stage.join("cpuv-bundle.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema":1, "executable":executable.file_name(), "machine":machine,
            "files":receipt, "windows_imports":system_imports,
            "runtime_assets":"Only explicitly supplied assets and PE import dependencies are included; dynamic loading requires explicit assets.",
            "license_notice":"Preserve upstream license files and redistribution terms when distributing this directory."
        }))?,
    )?;
    fs::rename(stage, destination).context("Publishing complete application bundle")?;
    Ok(())
}
