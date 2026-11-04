//! Explicit application-local deployment; never search arbitrary PATH directories.
use super::*;
use std::collections::BTreeMap;

pub struct Runtime<'a> {
    pub assets: &'a [PathBuf],
    pub search: &'a [PathBuf],
    pub environment: &'a BTreeMap<String, String>,
}

fn delay_imports(bytes: &[u8], pe: &goblin::pe::PE<'_>) -> Result<Vec<String>> {
    let Some(optional) = &pe.header.optional_header else {
        return Ok(vec![]);
    };
    let Some(directory) = optional.data_directories.get_delay_import_descriptor() else {
        return Ok(vec![]);
    };
    let offset = |rva: u32| -> Result<usize> {
        goblin::pe::utils::find_offset(
            rva as usize,
            &pe.sections,
            optional.windows_fields.file_alignment,
            &goblin::pe::options::ParseOptions::default(),
        )
        .context("Delay import RVA is outside file-backed sections")
    };
    let start = offset(directory.virtual_address)?;
    let size = directory.size as usize;
    ensure!(
        (32..=1024 * 1024).contains(&size),
        "Invalid delay import directory size"
    );
    let end = start
        .checked_add(size)
        .filter(|end| *end <= bytes.len())
        .context("Truncated delay import directory")?;
    let mut names = Vec::new();
    for descriptor in bytes[start..end].as_chunks::<32>().0 {
        if descriptor.iter().all(|b| *b == 0) {
            return Ok(names);
        }
        let attributes = u32::from_le_bytes(descriptor[0..4].try_into()?);
        ensure!(attributes & !1 == 0, "Unsupported delay import attributes");
        let raw = u32::from_le_bytes(descriptor[4..8].try_into()?);
        let rva = if attributes & 1 != 0 {
            raw
        } else {
            (raw as u64)
                .checked_sub(pe.image_base)
                .context("Invalid legacy delay import VA")?
                .try_into()?
        };
        let name = offset(rva)?;
        let data = bytes
            .get(name..name.saturating_add(1024).min(bytes.len()))
            .context("Truncated delayed DLL name")?;
        let length = data
            .iter()
            .position(|b| *b == 0)
            .context("Unterminated delayed DLL name")?;
        let name = std::str::from_utf8(&data[..length])?;
        ensure!(!name.is_empty(), "Empty delayed DLL name");
        names.push(name.to_owned());
    }
    bail!("Delay import directory has no terminator")
}

fn is_pe(path: &Path) -> Result<bool> {
    use std::io::Read;
    let mut magic = [0u8; 2];
    Ok(std::fs::File::open(path)?.read(&mut magic)? == 2 && magic == *b"MZ")
}

fn locate(directory: &Path, name: &str) -> Option<PathBuf> {
    fs::read_dir(directory)
        .ok()?
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

/// Reject incomplete provider runtime closure before publishing a development environment.
pub fn verify_runtime(metadata: &Environment, env: &Path) -> Result<()> {
    let bin = env.join("bin");
    if !bin.is_dir() {
        return Ok(());
    }
    let system = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .context("SystemRoot is unavailable")?;
    let mut search = vec![bin.clone(), metadata.compiler_bin.clone()];
    if let Some(vs) = metadata.process_environment.get("VSINSTALLDIR") {
        let redist = Path::new(vs).join("VC/Redist/MSVC");
        if redist.is_dir() {
            for entry in walkdir::WalkDir::new(redist) {
                let entry = entry?;
                if entry.file_type().is_dir()
                    && entry.file_name().to_string_lossy().ends_with("CRT")
                {
                    search.push(entry.path().to_owned());
                }
            }
        }
    }
    for entry in fs::read_dir(&bin)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() || !is_pe(&entry.path())? {
            continue;
        }
        let bytes = cpuv_core::binary::Data::open(&entry.path())?;
        let pe = goblin::pe::PE::parse(&bytes)?;
        let windows = system.join(
            if pe.header.coff_header.machine == 0x14c && cfg!(target_arch = "x86_64") {
                "SysWOW64"
            } else {
                "System32"
            },
        );
        let delayed = delay_imports(&bytes, &pe)?;
        for import in pe
            .libraries
            .iter()
            .copied()
            .chain(delayed.iter().map(String::as_str))
        {
            ensure!(!import.contains(['/', '\\']), "Invalid DLL import");
            let key = import.to_ascii_lowercase();
            if key.starts_with("api-ms-win-")
                || key.starts_with("ext-ms-win-")
                || windows.join(import).is_file()
            {
                continue;
            }
            let mut found = false;
            for directory in &search {
                if let Some(path) = locate(directory, import) {
                    cpuv_core::regular(&path)?;
                    let dependency = cpuv_core::binary::Data::open(&path)?;
                    if goblin::pe::PE::parse(&dependency)?
                        .header
                        .coff_header
                        .machine
                        == pe.header.coff_header.machine
                    {
                        found = true;
                        break;
                    }
                }
            }
            ensure!(
                found,
                "Incomplete runtime closure: {} imports missing {import}. Declare its provider dependency or install the missing runtime through the source build.install mapping",
                entry.path().display()
            );
        }
    }
    Ok(())
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
    export_with_runtime(
        metadata,
        env,
        binary,
        destination,
        &Runtime {
            assets,
            search: &[],
            environment: &BTreeMap::new(),
        },
    )
}

pub fn export_with_runtime(
    metadata: &Environment,
    env: &Path,
    binary: &Path,
    destination: &Path,
    runtime: &Runtime<'_>,
) -> Result<()> {
    let assets = runtime.assets;
    ensure!(
        !destination.exists(),
        "Bundle destination already exists; select a new directory"
    );
    outside_inputs(destination, env, assets)?;
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
            if is_pe(&output)? {
                pending.push(output);
            }
        } else {
            for entry in walkdir::WalkDir::new(asset) {
                let entry = entry?;
                if entry.file_type().is_dir() {
                    let relative = entry.path().strip_prefix(asset)?;
                    if !relative.as_os_str().is_empty() {
                        cpuv_core::contained(asset, relative)?;
                    }
                    fs::create_dir_all(stage.join(name).join(relative))?;
                }
            }
            let inventory = cpuv_core::inventory_all(asset)?;
            for relative in inventory.keys() {
                let output = stage.join(name).join(relative);
                copy_file(&asset.join(relative), &output, &mut receipt, stage)?;
                if is_pe(&output)? {
                    pending.push(output);
                }
            }
        }
    }
    let main_bytes = cpuv_core::binary::Data::open(&executable)?;
    let main = goblin::pe::PE::parse(&main_bytes)
        .context("Bundle entry must be a Windows PE executable")?;
    let machine = main.header.coff_header.machine;
    ensure!(
        !main.is_lib,
        "Bundle entry must be an executable, not a DLL"
    );
    drop(main);
    drop(main_bytes);
    let mut search = vec![env.join("bin"), metadata.compiler_bin.clone()];
    search.extend_from_slice(runtime.search);
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
        let bytes = cpuv_core::binary::Data::open(&path)?;
        let pe = goblin::pe::PE::parse(&bytes)?;
        ensure!(
            pe.header.coff_header.machine == machine,
            "Bundle architecture mismatch in {}",
            path.display()
        );
        let delayed = delay_imports(&bytes, &pe)?;
        for import in pe
            .libraries
            .iter()
            .copied()
            .chain(delayed.iter().map(String::as_str))
        {
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
    if !runtime.environment.is_empty() {
        ensure!(
            !stage.join("cpuv-run.ps1").exists(),
            "Runtime asset conflicts with the environment launcher"
        );
        fs::write(
            stage.join("cpuv-run.ps1"),
            r#"$ErrorActionPreference = 'Stop'
$receipt = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'cpuv-bundle.json') -Raw | ConvertFrom-Json
$previous = @{}
$inheritedPath = [string]$env:PATH
$code = 1
try {
    foreach ($entry in $receipt.environment.PSObject.Properties) {
        $previous[$entry.Name] = [Environment]::GetEnvironmentVariable($entry.Name,'Process')
        $value = $entry.Value.Replace(('$'+'{root}'),$PSScriptRoot).Replace(('$'+'{path}'),$inheritedPath)
        [Environment]::SetEnvironmentVariable($entry.Name,$value,'Process')
    }
    & (Join-Path $PSScriptRoot $receipt.executable) @args
    $code = $LASTEXITCODE
} finally {
    foreach ($name in $previous.Keys) { [Environment]::SetEnvironmentVariable($name,$previous[$name],'Process') }
}
exit $code
"#,
        )?;
        receipt.insert(
            "cpuv-run.ps1".into(),
            serde_json::json!({
                "path":"cpuv-run.ps1", "blake3":cpuv_core::digest(&stage.join("cpuv-run.ps1"))?
            }),
        );
    }
    fs::write(
        stage.join("cpuv-bundle.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema":2,"executable":executable.file_name().and_then(|s|s.to_str()).context("Bundle executable name must be Unicode")?,"machine":machine,
            "files":receipt,"windows_imports":system_imports,"environment":runtime.environment,
            "runtime_assets":"Only explicitly supplied assets and PE import dependencies are included; dynamic loading requires explicit assets.",
            "license_notice":"Preserve upstream license files and redistribution terms when distributing this directory."
        }))?,
    )?;
    fs::rename(stage, destination).context("Publishing complete application bundle")?;
    Ok(())
}

fn outside_inputs(destination: &Path, env: &Path, assets: &[PathBuf]) -> Result<()> {
    let output = cpuv_core::resolved_destination(destination)?;
    for input in std::iter::once(env)
        .chain(assets.iter().map(PathBuf::as_path))
        .filter(|p| p.is_dir())
    {
        ensure!(
            !output.starts_with(input.canonicalize()?),
            "Bundle destination must be outside the dependency environment and asset directories"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundle_outputs_cannot_recurse_into_asset_or_environment_inputs() {
        let root = tempfile::tempdir().unwrap();
        let env = root.path().join("env");
        let asset = root.path().join("assets");
        fs::create_dir_all(&env).unwrap();
        fs::create_dir_all(&asset).unwrap();
        assert!(
            outside_inputs(
                &env.join("nested/output"),
                &env,
                std::slice::from_ref(&asset)
            )
            .is_err()
        );
        assert!(
            outside_inputs(
                &asset.join("nested/output"),
                &env,
                std::slice::from_ref(&asset)
            )
            .is_err()
        );
        assert!(!env.join("nested").exists() && !asset.join("nested").exists());
        outside_inputs(&root.path().join("application"), &env, &[asset]).unwrap();
    }
}
