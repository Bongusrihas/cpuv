//! Discover and fingerprint the user's native Windows GCC installation.
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

mod msys;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AbiTuple {
    pub schema: String,
    pub os: String,
    pub arch: String,
    pub target_triple: String,
    pub compiler_family: String,
    pub compiler_abi_major: String,
    pub stdlib: String,
    pub stdlib_release: String,
    pub glibcxx_cxx11_abi: String,
    pub crt_runtime: String,
    pub thread_model: String,
    pub exception_model: String,
    pub cxx_std_ceiling: String,
    pub build_type: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provider {
    pub name: String,
    pub repository: String,
    pub package_prefix: String,
    pub archive_prefix: String,
    pub database: String,
}
impl Provider {
    fn msys2(name: &str, prefix: &str) -> Self {
        Self {
            name: name.into(),
            repository: format!("https://repo.msys2.org/mingw/{name}"),
            package_prefix: prefix.into(),
            archive_prefix: name.into(),
            database: format!("{name}.db"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toolchain {
    pub compiler_path: PathBuf,
    pub sysroot: PathBuf,
    pub gcc_version: String,
    pub lto_compression: Vec<String>,
    pub abi: AbiTuple,
    pub abi_hash: String,
    #[serde(default)]
    pub provider: Option<Provider>,
    #[serde(default)]
    pub linker: String,
    #[serde(default)]
    pub observed_abi_options: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

/// GCC's Windows driver does not accept Rust's verbatim Win32 path prefix.
pub fn driver_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{unc}"));
    }
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text))
}
fn is_project_shim(path: &Path) -> bool {
    path.components()
        .any(|part| part.as_os_str().eq_ignore_ascii_case(".cpuv"))
}
fn locate(requested: &Path) -> Result<PathBuf> {
    let requested = requested.to_string_lossy();
    let requested = Path::new(requested.trim_matches('"'));
    if requested.is_file() {
        ensure!(
            !is_project_shim(requested),
            "Select the real GCC executable, not a .cpuv compiler shim"
        );
        return Ok(driver_path(requested.canonicalize()?));
    }
    if requested.components().count() == 1
        && let Some(path) = std::env::var_os("PATH")
    {
        for dir in std::env::split_paths(&path) {
            for name in [requested.to_owned(), requested.with_extension("exe")] {
                let candidate = dir.join(name);
                if candidate.is_file() && !is_project_shim(&candidate) {
                    return Ok(driver_path(candidate.canonicalize()?));
                }
            }
        }
    }
    bail!(
        "Compiler not found: {}. Put g++ on PATH or set CPUV_CXX to its executable",
        requested.display()
    )
}
pub fn discover(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return locate(path);
    }
    for key in ["CPUV_CXX", "CXX"] {
        if let Some(path) = std::env::var_os(key) {
            ensure!(!path.is_empty(), "{key} is empty");
            return locate(Path::new(&path)).with_context(|| format!("Resolving {key}"));
        }
    }
    if let Ok(path) = locate(Path::new("g++")) {
        return Ok(path);
    }
    msys::discover()
}
fn field(text: &str, prefix: &str) -> Option<String> {
    text.lines()
        .find_map(|s| s.trim().strip_prefix(prefix))
        .map(|s| s.trim().to_owned())
}
fn macro_value(text: &str, name: &str) -> Option<String> {
    field(text, &format!("#define {name} "))
}
pub fn parse_verbose(verbose: &str, macros: &str, compiler: PathBuf) -> Result<Toolchain> {
    ensure!(
        !macros.contains("#define __clang__"),
        "This release requires GNU GCC; the selected g++ is Clang"
    );
    ensure!(
        macro_value(macros, "__GNUC__").is_some(),
        "Selected compiler did not identify itself as GCC"
    );
    let target = field(verbose, "Target: ").context("GCC did not report its target triple")?;
    ensure!(
        target.contains("mingw") || target.contains("cygwin") || target.ends_with("windows-gnu"),
        "Target {target} is not a native Windows GCC target"
    );
    let threads = field(verbose, "Thread model: ").unwrap_or_else(|| "unknown".into());
    let version = Regex::new(r"gcc version (\d+\.\d+(?:\.\d+)?)")?
        .captures(verbose)
        .map(|captures| captures[1].to_owned())
        .unwrap_or_else(|| {
            format!(
                "{}.{}.{}",
                macro_value(macros, "__GNUC__").unwrap_or_default(),
                macro_value(macros, "__GNUC_MINOR__").unwrap_or_else(|| "0".into()),
                macro_value(macros, "__GNUC_PATCHLEVEL__").unwrap_or_else(|| "0".into())
            )
        });
    let major = version.split('.').next().unwrap().to_owned();
    let pointer = macro_value(macros, "__SIZEOF_POINTER__");
    let arch = if macro_value(macros, "__i386__").is_some()
        || pointer.as_deref() == Some("4")
            && (target.starts_with('i') || target.starts_with("x86_64"))
    {
        "x86"
    } else if target.starts_with("x86_64") || macro_value(macros, "__x86_64__").is_some() {
        "x86_64"
    } else if target.starts_with("aarch64") {
        "aarch64"
    } else if target.starts_with('i')
        && macro_value(macros, "__SIZEOF_POINTER__").as_deref() == Some("4")
    {
        "x86"
    } else {
        target.split('-').next().unwrap_or("unknown")
    }
    .to_owned();
    let crt = if macro_value(macros, "_UCRT").is_some()
        || verbose.contains("--with-default-msvcrt=ucrt")
        || verbose.contains("--with-sysroot=/ucrt64")
    {
        "ucrt"
    } else if target.contains("cygwin") {
        "cygwin"
    } else if macro_value(macros, "__MSVCRT__").is_some()
        || verbose.contains("--with-default-msvcrt=msvcrt")
        || verbose.contains("--prefix=/mingw64")
        || verbose.contains("--prefix=/mingw32")
    {
        "msvcrt"
    } else {
        "unknown"
    };
    let exceptions = if macro_value(macros, "__SEH__").is_some() {
        "seh"
    } else if macro_value(macros, "__USING_SJLJ_EXCEPTIONS__").is_some() {
        "sjlj"
    } else if macro_value(macros, "__GCC_HAVE_DWARF2_CFI_ASM").is_some() {
        "dwarf2"
    } else {
        "unknown"
    };
    let stdlib_release = macro_value(macros, "_GLIBCXX_RELEASE").unwrap_or_else(|| major.clone());
    let dual = macro_value(macros, "_GLIBCXX_USE_CXX11_ABI").unwrap_or_else(|| "unknown".into());
    let is_msys2 = verbose.contains("MSYS2") || verbose.contains("msys2.org");
    let provider =
        if is_msys2 && arch == "x86_64" && threads == "posix" && exceptions == "seh" && dual == "1"
        {
            match crt {
                "ucrt" => Some(Provider::msys2("ucrt64", "mingw-w64-ucrt-x86_64-")),
                // Managed binaries intentionally start with one validated ecosystem.
                _ => None,
            }
        } else {
            None
        };
    let abi = AbiTuple {
        schema: "cpuv-abi-v1".into(),
        os: "windows".into(),
        arch,
        target_triple: target,
        compiler_family: "gcc".into(),
        compiler_abi_major: major,
        stdlib: "libstdc++".into(),
        stdlib_release,
        glibcxx_cxx11_abi: dual,
        crt_runtime: crt.into(),
        thread_model: threads,
        exception_model: exceptions.into(),
        cxx_std_ceiling: "compiler-default".into(),
        build_type: "release".into(),
    };
    let abi_hash = blake3::hash(&serde_json::to_vec(&abi)?).to_hex()[..32].to_owned();
    let sysroot = compiler
        .parent()
        .and_then(Path::parent)
        .context("Compiler has no installation root")?
        .to_owned();
    Ok(Toolchain {
        compiler_path: compiler,
        sysroot,
        gcc_version: version,
        lto_compression: field(verbose, "Supported LTO compression algorithms: ")
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        abi,
        abi_hash,
        provider,
        linker: "selected by GCC".into(),
        observed_abi_options: [
            "__GXX_ABI_VERSION",
            "__EXCEPTIONS",
            "__GXX_RTTI",
            "__BIGGEST_ALIGNMENT__",
            "__SSE2__",
            "__AVX__",
            "__AVX2__",
            "_GLIBCXX_DEBUG",
            "__cplusplus",
            "__STRICT_ANSI__",
        ]
        .into_iter()
        .map(|name| {
            (
                name.into(),
                macro_value(macros, name).unwrap_or_else(|| "not defined".into()),
            )
        })
        .collect(),
        diagnostics: Vec::new(),
    })
}
impl Toolchain {
    pub fn runtime_tag(&self) -> String {
        format!(
            "{}-{}-{}",
            self.abi.crt_runtime, self.abi.thread_model, self.abi.exception_model
        )
    }
    pub fn default_standard(&self) -> String {
        match self.abi.cxx_std_ceiling.as_str() {
            "gnu++26" | "gnu++23" | "gnu++20" => "gnu++20".into(),
            value if value.starts_with("gnu++") => value.into(),
            _ => "default".into(),
        }
    }
}
fn compiler_command(path: &Path) -> Result<Command> {
    let mut command = Command::new(path);
    let mut paths = vec![
        path.parent()
            .context("Compiler has no directory")?
            .to_owned(),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    command.env("PATH", std::env::join_paths(paths)?);
    Ok(command)
}
pub fn probe(explicit: Option<&Path>) -> Result<Toolchain> {
    probe_with_flags(explicit, &[])
}

pub fn probe_with_flags(
    explicit: Option<&Path>,
    flags: &[std::ffi::OsString],
) -> Result<Toolchain> {
    ensure!(cfg!(windows), "cpuv currently supports Windows hosts");
    let path = discover(explicit)?;
    let target = compiler_command(&path)?.arg("-dumpmachine").output()?;
    ensure!(target.status.success(), "Could not query the GCC target");
    let mut command = compiler_command(&path)?;
    command.args(flags);
    command.args([
        "-v",
        "-dM",
        "-E",
        "-x",
        "c++",
        "-include",
        "bits/c++config.h",
    ]);
    if String::from_utf8_lossy(&target.stdout).contains("mingw") {
        command.args(["-include", "_mingw.h"]);
    }
    let mut output = command
        .arg("-")
        .stdin(Stdio::null())
        .output()
        .context("Executing GCC probe")?;
    let private_headers = output.status.success();
    if !private_headers {
        // Private header names vary across older and independently built GCCs.
        output = compiler_command(&path)?
            .args(flags)
            .args(["-v", "-dM", "-E", "-x", "c++", "-"])
            .stdin(Stdio::null())
            .output()?;
    }
    ensure!(
        output.status.success(),
        "Compiler probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let verbose = format!(
        "Target: {}\n{}",
        String::from_utf8_lossy(&target.stdout).trim(),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut tc = parse_verbose(
        &verbose,
        &String::from_utf8_lossy(&output.stdout),
        path.clone(),
    )?;
    if !private_headers {
        tc.diagnostics.push("Private runtime headers could not be probed; native compilation remains available, managed binaries require known runtime metadata".into());
    }
    for standard in [
        "gnu++26", "gnu++23", "gnu++20", "gnu++17", "gnu++14", "gnu++11", "gnu++98",
    ] {
        if compiler_command(&path)?
            .args(flags)
            .args([
                "-x",
                "c++",
                "-fsyntax-only",
                &format!("-std={standard}"),
                "-",
            ])
            .stdin(Stdio::null())
            .output()?
            .status
            .success()
        {
            tc.abi.cxx_std_ceiling = standard.into();
            break;
        }
    }
    tc.abi_hash = blake3::hash(&serde_json::to_vec(&tc.abi)?).to_hex()[..32].to_owned();
    if let Ok(output) = compiler_command(&path)?.arg("-print-prog-name=ld").output() {
        let linker = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !linker.is_empty() {
            tc.linker = linker;
        }
    }
    Ok(tc)
}

/// Compile, link and execute a C++98-compatible program with the actual driver.
pub fn doctor(tc: &Toolchain, flags: &[std::ffi::OsString]) -> Result<serde_json::Value> {
    let temporary = tempfile::Builder::new().prefix("cpuv-doctor-").tempdir()?;
    let source = temporary.path().join("probe.cpp");
    let binary = temporary.path().join("probe.exe");
    std::fs::write(
        &source,
        "#include <iostream>\nint main(){std::cout << \"cpuv-toolchain-ok\";return 0;}\n",
    )?;
    let output = compiler_command(&tc.compiler_path)?
        .args(flags)
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .current_dir(temporary.path())
        .output()
        .context("Starting the compiler/linker check")?;
    ensure!(
        output.status.success(),
        "Selected GCC could not compile/link a native C++ program with these flags:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = compiler_command(&binary)?.env("PATH", {
        let mut paths = vec![tc.compiler_path.parent().unwrap().to_owned()];
        if let Some(path) = std::env::var_os("PATH") { paths.extend(std::env::split_paths(&path)); }
        std::env::join_paths(paths)?
    }).current_dir(temporary.path()).output().context("The compiled program cannot run on this Windows host; check target architecture and runtime DLL availability")?;
    ensure!(
        output.status.success() && output.stdout == b"cpuv-toolchain-ok",
        "Compiled program failed to run (status {}); check architecture, runtime DLLs and selected ABI",
        output.status
    );
    Ok(
        serde_json::json!({"compiler":tc.compiler_path,"target":tc.abi.target_triple,"linker":tc.linker,"flags":flags,"compile_link_run":"passed","universal_abi_guarantee":false}),
    )
}
