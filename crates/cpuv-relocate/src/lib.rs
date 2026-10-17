//! Relocation and explicit package-metadata-based compiler integration.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub mod inspect;
pub mod linking;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub schema: u32,
    pub compiler: PathBuf,
    pub compiler_bin: PathBuf,
    pub abi_hash: String,
    pub standard: String,
    pub include_dirs: Vec<String>,
    pub libraries: Vec<String>,
    pub defines: Vec<String>,
    pub cflags: Vec<String>,
    pub include_map: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub link_model: u32,
    #[serde(default)]
    pub link_flags: Vec<String>,
    #[serde(default)]
    pub library_dirs: Vec<String>,
    #[serde(default)]
    pub managed_binaries: bool,
    #[serde(default)]
    pub dependency_origin: String,
    #[serde(default)]
    pub source_arch: String,
    #[serde(default)]
    pub source_flags: Vec<String>,
}

pub fn msys_host_bin(compiler_bin: &Path) -> Option<PathBuf> {
    let bin = compiler_bin.parent()?.parent()?.join("usr/bin");
    bin.join("pacman.exe").is_file().then_some(bin)
}

fn expand(s: &str, vars: &BTreeMap<String, String>) -> String {
    let mut result = s.to_owned();
    for _ in 0..12 {
        let old = result.clone();
        for (name, value) in vars {
            result = result.replace(&format!("${{{name}}}"), value);
        }
        if old == result {
            break;
        }
    }
    result
}

fn replace_private(path: &Path, text: &str) -> Result<()> {
    // Break the hardlink before changing project-specific metadata.
    let tmp = path.with_extension("cpuv-new");
    fs::write(&tmp, text)?;
    fs::remove_file(path)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn index_environment(
    env: &Path,
    final_env: &Path,
    compiler: PathBuf,
    abi_hash: String,
    standard: String,
) -> Result<Environment> {
    let includes: BTreeSet<String> = ["include".into()].into_iter().collect();
    for entry in walkdir::WalkDir::new(env).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().is_some_and(|x| x == "pc") {
            let text = fs::read_to_string(path)?;
            // Relocate common MSYS2 prefixes, preserving variable-based definitions.
            let pc_dir = path.parent().unwrap();
            let depth = pc_dir.strip_prefix(env)?.components().count();
            let prefix = format!("${{pcfiledir}}/{}", vec![".."; depth].join("/"));
            let normalized = text
                .lines()
                .map(|line| {
                    if line.starts_with("prefix=") {
                        format!("prefix={prefix}")
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            replace_private(path, &normalized)?;
        } else if path.extension().is_some_and(|x| x == "cmake") {
            let text = fs::read_to_string(path)?;
            if ["/ucrt64/", "/mingw64/", "/mingw32/"]
                .iter()
                .any(|prefix| text.contains(prefix))
            {
                let mut normalized = text;
                for source in ["/ucrt64/", "/mingw64/", "/mingw32/"] {
                    normalized = normalized.replace(source, &format!("{}/", portable(final_env)));
                }
                replace_private(path, &normalized)?;
            }
        }
    }
    let libraries: Vec<String> = vec![];
    let mut include_map = BTreeMap::new();
    // Header inventory is diagnostic only; it never selects libraries.
    for include in &includes {
        for entry in fs::read_dir(env.join(include))? {
            let entry = entry?;
            include_map.insert(
                entry.file_name().to_string_lossy().to_string(),
                libraries.clone(),
            );
        }
    }
    let metadata = Environment {
        schema: 1,
        compiler_bin: compiler.parent().context("No compiler bin")?.to_owned(),
        compiler,
        abi_hash,
        standard,
        include_dirs: includes.into_iter().collect(),
        libraries,
        defines: vec![],
        cflags: vec![],
        include_map,
        link_model: 2,
        link_flags: vec![],
        library_dirs: vec!["lib".into()],
        managed_binaries: false,
        dependency_origin: "native".into(),
        source_arch: String::new(),
        source_flags: vec![],
    };
    fs::write(
        env.join("manifest.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    fs::write(
        env.join("include_map.json"),
        serde_json::to_vec_pretty(&metadata.include_map)?,
    )?;
    fs::write(env.join("cpuv.specs"), generate_specs(&metadata))?;
    Ok(metadata)
}

pub fn portable(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
}

pub fn generate_specs(env: &Environment) -> String {
    if !env.cflags.is_empty()
        || !env.link_flags.is_empty()
        || env.library_dirs.iter().any(|path| path != "lib")
    {
        return "*cc1plus:\n+ %ecpuv experimental specs cannot represent this link plan; use the cpuv compiler shim\n\n".into();
    }
    let includes = env
        .include_dirs
        .iter()
        .map(|p| format!("-isystem %:getenv(CPUV_ENV /{p})"))
        .collect::<Vec<_>>()
        .join(" ");
    let defines = env
        .defines
        .iter()
        .map(|d| format!("-D{d}"))
        .collect::<Vec<_>>()
        .join(" ");
    // In a specs *lib rule, pass ld flags directly, without the driver's -Wl,.
    let libs = env
        .libraries
        .iter()
        .map(|l| format!("-l{l}"))
        .collect::<Vec<_>>()
        .join(" ");
    let link = if env.libraries.is_empty() {
        String::new()
    } else {
        // GCC's C++ driver may place its first -lstdc++ before *lib. Revisit it
        // inside the group for static archives introduced by this optional rule.
        format!(
            "*lib:\n+ %{{!nostdlib:%{{!nodefaultlibs:--start-group {libs} -lstdc++ --end-group}}}}\n\n"
        )
    };
    format!(
        "*cpp_options:\n+ {includes} {defines}\n\n*cc1plus:\n+ {includes} {defines}\n\n*link_libgcc:\n+ -L%:getenv(CPUV_ENV /lib)\n\n{link}"
    )
}

pub fn is_compiler_query(input: &[std::ffi::OsString]) -> bool {
    input.is_empty()
        || input.iter().any(|s| {
            let s = s.to_string_lossy();
            matches!(
                s.as_ref(),
                "--version"
                    | "--help"
                    | "-dumpmachine"
                    | "-dumpversion"
                    | "-dumpfullversion"
                    | "-###"
            ) || s == "-v" && input.len() == 1
                || s.starts_with("-print-")
                || s.starts_with("--help=")
        })
}

/// Check user, package, and explicit project options against the managed profile.
pub fn validate_profile(metadata: &Environment, input: &[std::ffi::OsString]) -> Result<()> {
    if metadata.managed_binaries && metadata.dependency_origin != "source" {
        let project_options = input
            .iter()
            .map(|value| value.to_string_lossy().into_owned());
        let metadata_options = metadata
            .cflags
            .iter()
            .chain(&metadata.link_flags)
            .cloned()
            .chain(metadata.defines.iter().map(|value| format!("-D{value}")));
        for arg in project_options.chain(metadata_options) {
            ensure!(
                !["-static", "-static-pie"].contains(&arg.as_str()) && !arg.contains("-Bstatic"),
                "Managed packages use public/shared pkg-config interfaces; static linkage requires a separate validated profile"
            );
            ensure!(
                ![
                    "-m32",
                    "-mx32",
                    "-fshort-wchar",
                    "-fshort-enums",
                    "-fno-exceptions",
                    "-fno-rtti"
                ]
                .contains(&arg.as_str())
                    && ![
                        "-fabi-version=",
                        "-fpack-struct",
                        "-mabi=",
                        "-D_GLIBCXX_DEBUG",
                        "-U_GLIBCXX_USE_CXX11_ABI"
                    ]
                    .iter()
                    .any(|prefix| arg.starts_with(prefix))
                    && !arg.contains("_GLIBCXX_USE_CXX11_ABI=0")
                    && !arg.contains("_GLIBCXX_DEBUG"),
                "ABI-changing option {arg} is not supported with managed UCRT64 binaries; rebuild dependencies with the same profile instead"
            );
        }
    }
    if metadata.managed_binaries && metadata.dependency_origin == "source" {
        let options: Vec<String> = input
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .chain(metadata.cflags.iter().cloned())
            .chain(metadata.link_flags.iter().cloned())
            .chain(metadata.defines.iter().map(|value| format!("-D{value}")))
            .collect();
        let normalize = |values: &[String]| {
            let mut result = Vec::new();
            let mut arguments = values.iter();
            while let Some(argument) = arguments.next() {
                if matches!(argument.as_str(), "-D" | "-U") {
                    if let Some(value) = arguments.next() {
                        result.push(format!("{argument}{value}"));
                    }
                } else {
                    result.push(argument.clone());
                }
            }
            result
        };
        let built_flags = normalize(&metadata.source_flags);
        for arg in normalize(&options) {
            let architecture_matches = (arg == "-m32" && metadata.source_arch == "x86")
                || (arg == "-m64" && metadata.source_arch == "x86_64");
            let changes_profile = [
                "-m32",
                "-m64",
                "-mx32",
                "-fabi-version=",
                "-fpack-struct",
                "-fshort-",
                "-fno-exceptions",
                "-fno-rtti",
                "-mabi=",
                "-D_GLIBCXX_",
                "-U_GLIBCXX_",
            ]
            .iter()
            .any(|prefix| arg.starts_with(prefix));
            ensure!(
                !changes_profile
                    || architecture_matches
                    || built_flags.iter().any(|flag| flag == &arg),
                "Option {arg} changes the source dependency profile; set CXXFLAGS and run cpuv sync to rebuild matching dependencies"
            );
        }
    }
    Ok(())
}

pub fn compiler_args(
    metadata: &Environment,
    env: &Path,
    input: &[std::ffi::OsString],
) -> Result<Vec<std::ffi::OsString>> {
    use std::ffi::OsString;
    ensure!(metadata.schema == 1, "Unsupported environment schema");
    ensure!(
        metadata.link_model == 2,
        "Legacy link metadata; run cpuv sync to regenerate a package-metadata link plan"
    );
    let mut out = input.to_vec();
    if is_compiler_query(input) {
        return Ok(out);
    }
    validate_profile(metadata, input)?;
    let compile_only = input.iter().any(|s| {
        matches!(
            s.to_str(),
            Some("-c" | "-E" | "-S" | "-M" | "-MM" | "-fsyntax-only")
        )
    });
    if !input
        .iter()
        .any(|s| s.to_string_lossy().starts_with("-std="))
        && metadata.standard != "default"
    {
        out.push(format!("-std={}", metadata.standard).into());
    }
    for dir in &metadata.include_dirs {
        out.push("-isystem".into());
        out.push(env.join(dir).into_os_string());
    }
    for d in &metadata.defines {
        out.push(format!("-D{d}").into());
    }
    out.extend(metadata.cflags.iter().map(OsString::from));
    if !compile_only
        && (!metadata.libraries.is_empty() || !metadata.link_flags.is_empty())
        && !input
            .iter()
            .any(|s| matches!(s.to_str(), Some("-nostdlib" | "-nodefaultlibs")))
    {
        for directory in &metadata.library_dirs {
            out.push("-L".into());
            out.push(env.join(directory).into_os_string());
        }
        let grouped = metadata.dependency_origin != "source" && metadata.managed_binaries;
        if grouped {
            out.push("-Wl,--start-group".into());
        }
        out.extend(
            metadata
                .libraries
                .iter()
                .map(|lib| OsString::from(format!("-l{lib}"))),
        );
        if grouped {
            out.push("-Wl,--end-group".into());
        }
        out.extend(metadata.link_flags.iter().map(OsString::from));
    }
    Ok(out)
}
