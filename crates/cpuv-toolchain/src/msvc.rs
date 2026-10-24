use super::*;
use std::{collections::BTreeMap, fs};

pub fn installations() -> Vec<PathBuf> {
    let Some(base) = std::env::var_os("ProgramFiles(x86)") else {
        return vec![];
    };
    let executable = PathBuf::from(base).join("Microsoft Visual Studio/Installer/vswhere.exe");
    let Ok(output) = Command::new(executable)
        .args([
            "-all",
            "-prerelease",
            "-products",
            "*",
            "-property",
            "installationPath",
        ])
        .output()
    else {
        return vec![];
    };
    if !output.status.success() {
        return vec![];
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| PathBuf::from(l.trim()))
        .collect()
}

pub fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for installation in installations() {
        if let Ok(versions) = fs::read_dir(installation.join("VC/Tools/MSVC")) {
            for version in versions.flatten() {
                for target in ["x64", "x86", "arm64"] {
                    let path = version
                        .path()
                        .join("bin/Hostx64")
                        .join(target)
                        .join("cl.exe");
                    if path.is_file() {
                        paths.push(path);
                    }
                }
            }
        }
    }
    paths.sort();
    paths.reverse();
    paths
}

pub fn environment(compiler: &Path) -> Result<BTreeMap<String, String>> {
    let selected = compiler.ancestors().find(|path| path.join("VC/Auxiliary/Build/vcvarsall.bat").is_file()).map(Path::to_owned)
        .or_else(|| installations().into_iter().next()).context("Microsoft C++ Build Tools and Windows SDK are required; install them or run from a developer terminal")?;
    let script = selected.join("VC/Auxiliary/Build/vcvarsall.bat");
    let target = match compiler
        .parent()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
    {
        Some("x86") => "amd64_x86",
        Some("arm64") => "amd64_arm64",
        _ => "amd64",
    };
    let version = compiler
        .ancestors()
        .find(|path| {
            path.parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == "MSVC")
        })
        .and_then(Path::file_name)
        .and_then(|s| s.to_str());
    let version_option = if let Some(version) = version {
        ensure!(
            !version.is_empty() && version.bytes().all(|c| c.is_ascii_digit() || c == b'.'),
            "Invalid MSVC toolset directory"
        );
        format!(" -vcvars_ver={version}")
    } else {
        String::new()
    };
    let path = script.to_string_lossy();
    ensure!(
        !path.contains(['"', '&', '|', '<', '>', '^', '%', '!', '\n', '\r']),
        "Unsupported metacharacter in Visual Studio setup path"
    );
    let mut setup = Command::new("cmd.exe");
    setup.args(["/d", "/s", "/c"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        setup.raw_arg(format!(
            "\"\"{path}\" {target}{version_option} >nul && set\""
        ));
    }
    #[cfg(not(windows))]
    setup.arg(format!(
        "\"\"{path}\" {target}{version_option} >nul && set\""
    ));
    let output = setup.output()?;
    ensure!(
        output.status.success(),
        "Microsoft developer environment initialization failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut environment = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.to_ascii_uppercase();
            if [
                "PATH",
                "INCLUDE",
                "LIB",
                "LIBPATH",
                "WINDOWSSDKDIR",
                "WINDOWSSDKVERSION",
                "VCTOOLSINSTALLDIR",
                "VSINSTALLDIR",
            ]
            .contains(&key.as_str())
            {
                environment.insert(key, value.to_owned());
            }
        }
    }
    ensure!(
        environment.contains_key("INCLUDE") && environment.contains_key("LIB"),
        "Missing Microsoft SDK include/library environment"
    );
    Ok(environment)
}

pub fn probe(path: PathBuf, flags: &[std::ffi::OsString]) -> Result<Toolchain> {
    let environment = environment(&path)?;
    let temporary = tempfile::tempdir()?;
    let source = temporary.path().join("probe.cpp");
    fs::write(
        &source,
        "#include <yvals_core.h>\nCPUV_VERSION _MSC_FULL_VER\nCPUV_LANG _MSVC_LANG\nCPUV_STL _MSVC_STL_VERSION\n#ifdef _M_ARM64\nCPUV_ARCH aarch64\n#elif defined(_M_X64)\nCPUV_ARCH x86_64\n#else\nCPUV_ARCH x86\n#endif\n",
    )?;
    let output = Command::new(&path)
        .envs(&environment)
        .args(["/nologo", "/EP", "/TP"])
        .args(flags)
        .arg(source)
        .current_dir(temporary.path())
        .output()?;
    ensure!(
        output.status.success(),
        "Microsoft compiler/SDK probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    let value = |name: &str| -> Result<String> {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(name))
            .map(|v| v.trim().to_owned())
            .with_context(|| format!("Missing compiler observation {name}"))
    };
    let full = value("CPUV_VERSION ")?;
    ensure!(
        full.len() >= 9 && full.bytes().all(|b| b.is_ascii_digit()),
        "Invalid Microsoft compiler version"
    );
    let clang = path
        .file_stem()
        .is_some_and(|s| s.eq_ignore_ascii_case("clang-cl"));
    let family = if clang { "clang" } else { "msvc" };
    let version = if clang {
        let output = Command::new(&path).arg("--version").output()?;
        Regex::new(r"clang version (\d+\.\d+(?:\.\d+)?)")?
            .captures(&String::from_utf8_lossy(&output.stdout))
            .map(|c| c[1].to_owned())
            .context("Clang did not report its version")?
    } else {
        format!("{}.{}.{}", &full[..2], &full[2..4], &full[4..])
    };
    let arch = value("CPUV_ARCH ")?;
    let abi = AbiTuple {
        schema: "cpuv-abi-v2".into(),
        os: "windows".into(),
        arch: arch.clone(),
        target_triple: format!(
            "{}-pc-windows-msvc",
            if arch == "x86" { "i686" } else { &arch }
        ),
        compiler_family: family.into(),
        compiler_abi_major: if clang {
            version.split('.').next().unwrap().into()
        } else {
            full[..3].into()
        },
        stdlib: "msvc".into(),
        stdlib_release: value("CPUV_STL ")?,
        glibcxx_cxx11_abi: "not-applicable".into(),
        crt_runtime: "msvc".into(),
        thread_model: "win32".into(),
        exception_model: "seh".into(),
        cxx_std_ceiling: "compiler-default".into(),
        build_type: "release".into(),
    };
    let linker = path.with_file_name(if clang { "lld-link.exe" } else { "link.exe" });
    let linker = if linker.is_file() {
        linker
    } else {
        std::env::split_paths(&std::ffi::OsString::from(&environment["PATH"]))
            .map(|p| p.join("link.exe"))
            .find(|p| p.is_file())
            .context("Microsoft linker not found")?
    };
    let abi_hash = blake3::hash(&serde_json::to_vec(&abi)?).to_hex()[..32].to_owned();
    Ok(Toolchain {
        compiler_path: path.clone(),
        c_compiler: path.clone(),
        driver: "msvc".into(),
        environment,
        sysroot: path.parent().context("No compiler directory")?.to_owned(),
        gcc_version: version,
        lto_compression: vec![],
        abi,
        abi_hash,
        provider: None,
        linker: linker.to_string_lossy().into_owned(),
        observed_abi_options: BTreeMap::from([
            ("__cplusplus".into(), value("CPUV_LANG ")?),
            ("__STRICT_ANSI__".into(), "1".into()),
        ]),
        diagnostics: vec![],
    })
}
