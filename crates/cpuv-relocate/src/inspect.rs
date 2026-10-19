//! Evidence about binary format and packaged symbol edges, never an ABI proof.
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Serialize)]
struct Binary {
    path: String,
    machine: u16,
    imports: Vec<(String, String)>,
    exports: BTreeSet<String>,
}

pub fn environment(env: &Path, arch: &str) -> Result<serde_json::Value> {
    environment_with_runtime_policy(env, arch, true)
}

pub fn environment_with_runtime_policy(
    env: &Path,
    arch: &str,
    require_ucrt: bool,
) -> Result<serde_json::Value> {
    environment_for_runtime(env, arch, if require_ucrt { "ucrt" } else { "unknown" })
}

pub fn environment_for_runtime(env: &Path, arch: &str, runtime: &str) -> Result<serde_json::Value> {
    ensure!(
        ["ucrt", "msvcrt", "unknown"].contains(&runtime),
        "Unsupported Windows runtime {runtime}"
    );
    let expected = match arch {
        "x86_64" => 0x8664,
        "x86" => 0x14c,
        "aarch64" => 0xaa64,
        _ => anyhow::bail!("No binary inspector for target architecture {arch}"),
    };
    let mut binaries = Vec::new();
    let mut archives = 0;
    let mut objects = 0;
    let mut unknown_archive_members = 0;
    let mut non_archive_files = Vec::new();
    let mut external = BTreeSet::new();
    for entry in walkdir::WalkDir::new(env).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let direct_exe = ext == "exe" && path.parent() == Some(env.join("bin").as_path());
        if ["dll", "pyd"].contains(&ext.as_str()) || direct_exe {
            let data = fs::read(path)?;
            let pe = goblin::pe::PE::parse(&data)
                .with_context(|| format!("Invalid PE binary {}", path.display()))?;
            ensure!(
                pe.header.coff_header.machine == expected,
                "Binary architecture mismatch in {}: machine {:x}, expected {:x}",
                path.display(),
                pe.header.coff_header.machine,
                expected
            );
            let imports: Vec<_> = pe
                .imports
                .iter()
                .map(|import| (import.dll.to_ascii_lowercase(), import.name.to_string()))
                .collect();
            // Inspect direct package imports; OS implementation dependencies are
            // outside this check. GCC providers cannot reuse an opposing CRT.
            ensure!(
                !imports.iter().any(|(dll, _)| {
                    (runtime == "ucrt" && dll == "msvcrt.dll")
                        || (runtime == "msvcrt"
                            && (dll == "ucrtbase.dll" || dll.starts_with("api-ms-win-crt-")))
                        || (runtime != "unknown"
                            && (dll.starts_with("vcruntime") || dll.starts_with("msvcp")))
                }),
                "Package binary imports an unsupported C/C++ runtime: {}",
                path.display()
            );
            binaries.push(Binary {
                path: super::portable(path.strip_prefix(env)?),
                machine: pe.header.coff_header.machine,
                imports,
                exports: pe
                    .exports
                    .iter()
                    .filter_map(|export| export.name.map(str::to_owned))
                    .collect(),
            });
        } else if ext == "a" || ext == "lib" {
            let data = fs::read(path)?;
            if !data.starts_with(b"!<arch>\n") && !data.starts_with(b"!<thin>\n") {
                let relative = path.strip_prefix(env)?;
                let library_payload = relative.parent().is_some_and(|parent| {
                    parent.components().any(|component| {
                        ["lib", "lib32", "lib64", "libs"]
                            .iter()
                            .any(|name| component.as_os_str().eq_ignore_ascii_case(name))
                    })
                });
                ensure!(
                    !library_payload,
                    "Invalid library archive {}: expected archive magic",
                    path.display()
                );
                non_archive_files.push(super::portable(relative));
                continue;
            }
            let archive = goblin::archive::Archive::parse(&data)
                .with_context(|| format!("Unsupported library archive {}", path.display()))?;
            archives += 1;
            for member in archive.members() {
                let bytes = archive.extract(member, &data)?;
                let machine = if bytes.starts_with(&[0, 0, 0xff, 0xff]) && bytes.len() >= 8 {
                    Some(u16::from_le_bytes([bytes[6], bytes[7]]))
                } else if bytes.len() >= 20 {
                    let value = u16::from_le_bytes([bytes[0], bytes[1]]);
                    [0x8664, 0x14c, 0xaa64].contains(&value).then_some(value)
                } else {
                    None
                };
                if let Some(machine) = machine {
                    ensure!(
                        machine == expected,
                        "COFF architecture mismatch in {} member {member}",
                        path.display()
                    );
                    objects += 1;
                } else {
                    unknown_archive_members += 1;
                }
            }
        }
    }
    let mut by_name = BTreeMap::<String, usize>::new();
    for (index, binary) in binaries.iter().enumerate() {
        let name = Path::new(&binary.path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        // Only the runtime bin directory defines the primary DLL search set.
        if Path::new(&binary.path).parent() == Some(Path::new("bin")) {
            by_name.insert(name, index);
        }
    }
    let mut checked_symbols = 0;
    for binary in &binaries {
        for (dll, symbol) in &binary.imports {
            if let Some(index) = by_name.get(dll) {
                // Goblin represents ordinal imports as ORDINAL n, not a name.
                if !symbol.starts_with("ORDINAL ") {
                    ensure!(
                        binaries[*index].exports.contains(symbol),
                        "Packaged DLL {dll} is missing imported symbol {symbol}, required by {}",
                        binary.path
                    );
                    checked_symbols += 1;
                }
            } else {
                external.insert(dll.clone());
            }
        }
    }
    Ok(
        serde_json::json!({"compatibility_filter_only": true, "binary_compatibility_guaranteed": false,
        "target_architecture": arch, "pe_binaries": binaries, "library_archives": archives,
        "coff_members_checked": objects, "unclassified_archive_members": unknown_archive_members,
        "non_archive_files_skipped": non_archive_files,
        "packaged_symbol_edges_checked": checked_symbols, "external_dlls_not_proven": external,
        "not_proven": ["CPU instruction requirements", "structure packing", "library feature macros", "debug iterator ABI", "RTTI/exceptions across interfaces", "external and ordinal exports", "LTO plugin compatibility"]}),
    )
}
