//! Build-system integration uses native package targets and real selected compilers.
use super::*;

fn cmake_standard(standard: &str) -> Vec<OsString> {
    if standard == "default" {
        return [
            "-UCMAKE_CXX_STANDARD",
            "-UCMAKE_CXX_EXTENSIONS",
            "-UCMAKE_CXX_STANDARD_REQUIRED",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
    }
    let value = standard
        .trim_start_matches("gnu++")
        .trim_start_matches("c++");
    let value = if value == "03" { "98" } else { value };
    vec![
        format!("-DCMAKE_CXX_STANDARD={value}").into(),
        format!(
            "-DCMAKE_CXX_EXTENSIONS={}",
            if standard.starts_with("gnu++") {
                "ON"
            } else {
                "OFF"
            }
        )
        .into(),
        "-DCMAKE_CXX_STANDARD_REQUIRED=ON".into(),
    ]
}

async fn declared_tools(
    root: &Path,
    cli: &Cli,
    manifest: &Manifest,
) -> Result<(BTreeMap<String, PathBuf>, PathBuf)> {
    let mut tools = BTreeMap::new();
    for (name, artifact) in &manifest.tools {
        ensure!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)),
            "Invalid project tool name {name}"
        );
        cpuv_store::safe_relative(Path::new(name))?;
        tools.insert(
            name.clone(),
            source::managed::tool(name, artifact, cli).await?,
        );
    }
    let launchers = root
        .join(".cpuv/build/tool-launchers")
        .join(request_hash(manifest)?);
    fs::create_dir_all(&launchers)?;
    for (name, artifact) in &manifest.tools {
        let launcher = launchers.join(format!("{name}.cmd"));
        if let Some(interpreter) = &artifact.interpreter {
            let interpreter = tools
                .get(interpreter)
                .context("Declare the tool interpreter as a pinned [tools] entry")?;
            let executable = &tools[name];
            ensure!(
                !interpreter.to_string_lossy().contains(['%', '\r', '\n'])
                    && !executable.to_string_lossy().contains(['%', '\r', '\n']),
                "Unsupported launcher path"
            );
            write_atomic(
                &launcher,
                format!(
                    "@echo off\r\n\"{}\" \"{}\" %*\r\n",
                    interpreter.display(),
                    executable.display()
                )
                .as_bytes(),
            )?;
        } else if launcher.is_file() {
            fs::remove_file(launcher)?;
        }
    }
    Ok((tools, launchers))
}

fn stage(command: &mut Command, root: &Path, name: &str) -> Result<std::process::ExitStatus> {
    let log = root.join(".cpuv/logs").join(format!(
        "{name}-{}.log",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    eprintln!("Build log: {}", log.display());
    cpuv_core::process::run(command, &log, &cpuv_core::process::Limits::environment()?)
}

struct BuildContext<'a> {
    root: &'a Path,
    cli: &'a Cli,
    source: &'a Path,
    env: &'a Path,
    manifest: &'a Manifest,
    tc: &'a Toolchain,
    metadata: &'a cpuv_relocate::Environment,
}

async fn other(context: &BuildContext<'_>, test: bool, arguments: &[OsString]) -> Result<i32> {
    use source::native::System;
    let BuildContext {
        root,
        cli,
        source,
        env,
        manifest,
        tc,
        metadata,
    } = *context;
    let specification = manifest
        .build
        .as_ref()
        .context("Missing build declaration")?;
    let name = match specification.system {
        System::Meson => "meson",
        System::Make => "make",
        System::Msbuild => "msbuild",
        _ => bail!("Choose cmake, meson, make or msbuild for a project build"),
    };
    let build = root.join(".cpuv/build").join(name);
    fs::create_dir_all(&build)?;
    let output_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(".cpuv/build/build.lock"))?;
    output_lock
        .try_lock_exclusive()
        .context("Another build is using this profile")?;
    let (mut tools, launchers) = declared_tools(root, cli, manifest).await?;
    if specification.system == System::Meson && !tools.contains_key("ninja") {
        tools.insert(
            "ninja".into(),
            source::tools::selected("ninja", cli, &manifest.tools).await?,
        );
    }
    let mut paths = vec![launchers, env.join("bin")];
    paths.extend(
        tools
            .values()
            .filter_map(|p| p.parent().map(Path::to_owned)),
    );
    paths.push(tc.compiler_path.parent().unwrap().to_owned());
    if let Some(path) = tc
        .environment
        .get("PATH")
        .map(OsString::from)
        .or_else(|| std::env::var_os("PATH"))
    {
        paths.extend(std::env::split_paths(&path));
    }
    let path = manifest
        .runtime
        .clone()
        .unwrap_or_default()
        .path(source, &std::env::join_paths(paths)?)?;
    for artifact in manifest.tools.values() {
        if let Some(interpreter) = &artifact.interpreter {
            ensure!(
                tools.contains_key(interpreter),
                "Declare the interpreter as a pinned tool: {interpreter}"
            );
        }
    }
    let command = |name: &str| {
        let executable = tools.get(name).cloned().unwrap_or_else(|| name.into());
        let mut value = if let Some(interpreter) = manifest
            .tools
            .get(name)
            .and_then(|t| t.interpreter.as_ref())
        {
            let mut value = Command::new(
                tools
                    .get(interpreter)
                    .cloned()
                    .unwrap_or_else(|| interpreter.into()),
            );
            value.arg(executable);
            value
        } else {
            Command::new(executable)
        };
        value
            .envs(&tc.environment)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PATH", &path)
            .current_dir(source)
            .env_remove("CPPFLAGS")
            .env_remove("CFLAGS")
            .env_remove("CXXFLAGS")
            .env_remove("LDFLAGS");
        if let Some(ninja) = tools.get("ninja") {
            value.env("NINJA", ninja);
        }
        value
    };
    let jobs = cli
        .build_jobs
        .map(usize::from)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from));
    let configuration = &manifest.project.configuration;
    if specification.system == System::Meson {
        ensure!(source.join("meson.build").is_file(), "No meson.build");
        let machine = build.join("cpuv-native.ini");
        let quote = |p: &Path| cpuv_relocate::portable(p).replace('\'', "\\'");
        let compiler = root.join(".cpuv/bin").join(if tc.driver == "msvc" {
            "cl.exe"
        } else {
            "g++.exe"
        });
        fs::write(
            &machine,
            format!(
                "[binaries]\nc = '{}'\ncpp = '{}'\n[properties]\ncmake_prefix_path = ['{}']\n",
                quote(&tc.c_compiler),
                quote(&compiler),
                quote(env)
            ),
        )?;
        let mut setup = command("meson");
        setup
            .arg("setup")
            .arg(&build)
            .arg(source)
            .arg("--native-file")
            .arg(machine)
            .arg(format!(
                "--buildtype={}",
                if configuration == "Debug" {
                    "debug"
                } else {
                    "release"
                }
            ));
        if build.join("meson-private/coredata.dat").is_file() {
            setup.arg("--reconfigure");
        }
        for (key, value) in &specification.variables {
            setup.arg(format!("-D{key}={value}"));
        }
        setup.args(&specification.arguments);
        let status = stage(&mut setup, root, "meson-configure")?;
        if !status.success() {
            return Ok(status.code().unwrap_or(1));
        }
        let mut compile = command("meson");
        compile
            .args(["compile", "-C"])
            .arg(&build)
            .arg("-j")
            .arg(jobs.to_string());
        if !test {
            compile.args(arguments);
        }
        let status = stage(&mut compile, root, "meson-build")?;
        if !status.success() || !test {
            return Ok(status.code().unwrap_or(1));
        }
        let tests: serde_json::Value =
            serde_json::from_slice(&fs::read(build.join("meson-info/intro-tests.json"))?)?;
        ensure!(
            tests.as_array().is_some_and(|t| !t.is_empty()),
            "Meson project declares no tests"
        );
        let mut run = command("meson");
        if let Some(runtime) = &manifest.runtime {
            run.envs(runtime.variables(source, &path)?);
        }
        run.args(["test", "--no-rebuild", "--print-errorlogs", "-C"])
            .arg(&build)
            .args(&specification.test_arguments)
            .args(arguments);
        return Ok(stage(&mut run, root, "meson-test")?.code().unwrap_or(1));
    }
    let copied = build.join("source");
    source::native::sync_project_copy(source, &copied, &build.join("source-files.json"))?;
    if specification.system == System::Make {
        let default = if tc.driver == "msvc" {
            "nmake"
        } else {
            "mingw32-make"
        };
        let mut make = command(if tools.contains_key("make") {
            "make"
        } else {
            default
        });
        if tc.driver != "msvc" {
            make.arg(format!("-j{jobs}"));
        }
        let compiler = root.join(".cpuv/bin").join(if tc.driver == "msvc" {
            "cl.exe"
        } else {
            "g++.exe"
        });
        make.current_dir(&copied)
            .arg(format!("CXX=\"{}\"", cpuv_relocate::portable(&compiler)))
            .arg(format!(
                "CC=\"{}\"",
                cpuv_relocate::portable(&tc.c_compiler)
            ));
        if let Some(project) = &specification.project {
            cpuv_store::safe_relative(project)?;
            make.arg("-f").arg(project);
        }
        for (key, value) in &specification.variables {
            ensure!(
                !["CC", "CXX"].contains(&key.as_str()),
                "The profile owns CC/CXX"
            );
            make.arg(format!("{key}={value}"));
        }
        if test {
            if let Some(runtime) = &manifest.runtime {
                make.envs(runtime.variables(source, &path)?);
            }
            if specification.test_arguments.is_empty() {
                make.arg("test");
            } else {
                make.args(&specification.test_arguments);
            }
        } else {
            make.args(&specification.arguments);
        }
        make.args(arguments);
        return Ok(stage(&mut make, root, "make")?.code().unwrap_or(1));
    }
    ensure!(
        tc.driver == "msvc",
        "MSBuild needs the MSVC or clang-cl adapter"
    );
    let project = specification
        .project
        .as_ref()
        .context("MSBuild needs build.project")?;
    cpuv_store::safe_relative(project)?;
    let mut msbuild = if tools.contains_key("msbuild") {
        command("msbuild")
    } else if let Some(vs) = tc.environment.get("VSINSTALLDIR") {
        let mut value = Command::new(Path::new(vs).join("MSBuild/Current/Bin/MSBuild.exe"));
        value.envs(&tc.environment).env("PATH", &path);
        value
    } else {
        command("msbuild")
    };
    let flags = cpuv_relocate::compiler_args(metadata, env, &["/nologo".into()])?;
    let divider = flags
        .iter()
        .position(|s| s == "/link")
        .unwrap_or(flags.len());
    let encode = |args: &[OsString]| {
        args.iter()
            .map(|a| cpuv_core::arguments::quote(&a.to_string_lossy(), true))
            .collect::<Vec<_>>()
            .join(" ")
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let properties = build.join("cpuv.props");
    fs::write(
        &properties,
        format!(
            "<Project xmlns=\"http://schemas.microsoft.com/developer/msbuild/2003\"><ItemDefinitionGroup><ClCompile><AdditionalOptions>{} %(AdditionalOptions)</AdditionalOptions></ClCompile><Link><AdditionalOptions>{} %(AdditionalOptions)</AdditionalOptions></Link></ItemDefinitionGroup></Project>",
            encode(&flags[..divider]),
            encode(if divider < flags.len() {
                &flags[divider + 1..]
            } else {
                &[]
            })
        ),
    )?;
    let toolset = if tc.abi.compiler_family == "clang" {
        "ClangCL".to_owned()
    } else {
        format!("v{}", tc.abi.stdlib_release)
    };
    msbuild
        .current_dir(&copied)
        .arg(copied.join(project))
        .arg(format!("/m:{jobs}"))
        .arg(format!("/p:Configuration={configuration}"))
        .arg(format!("/p:PlatformToolset={toolset}"))
        .arg(format!(
            "/p:Platform={}",
            match tc.abi.arch.as_str() {
                "x86" => "Win32",
                "aarch64" => "ARM64",
                _ => "x64",
            }
        ))
        .arg(format!(
            "/p:CLToolPath={}",
            tc.compiler_path.parent().unwrap().display()
        ))
        .arg(format!(
            "/p:CLToolExe={}",
            tc.compiler_path.file_name().unwrap().to_string_lossy()
        ))
        .arg(format!(
            "/p:ForceImportBeforeCppTargets={}",
            properties.display()
        ));
    if let Some(directory) = tc.environment.get("VCTOOLSINSTALLDIR") {
        let version = Path::new(directory.trim_end_matches(['\\', '/']))
            .file_name()
            .context("Missing selected Microsoft toolset version")?;
        msbuild.arg(format!("/p:VCToolsVersion={}", version.to_string_lossy()));
    }
    if let Some(version) = tc.environment.get("WINDOWSSDKVERSION") {
        msbuild.arg(format!(
            "/p:WindowsTargetPlatformVersion={}",
            version.trim_end_matches(['\\', '/'])
        ));
    }
    let linker = Path::new(&tc.linker);
    msbuild
        .arg(format!(
            "/p:LinkToolPath={}",
            linker
                .parent()
                .context("Linker has no directory")?
                .display()
        ))
        .arg(format!(
            "/p:LinkToolExe={}",
            linker
                .file_name()
                .context("Linker has no file name")?
                .to_string_lossy()
        ));
    for (key, value) in &specification.variables {
        msbuild.arg(format!("/p:{key}={value}"));
    }
    if test {
        ensure!(
            !specification.test_arguments.is_empty(),
            "MSBuild tests need explicit build.test_arguments"
        );
        if let Some(runtime) = &manifest.runtime {
            msbuild.envs(runtime.variables(source, &path)?);
        }
        msbuild.args(&specification.test_arguments);
    } else {
        msbuild.args(&specification.arguments);
    }
    msbuild.args(arguments);
    Ok(stage(&mut msbuild, root, "msbuild")?.code().unwrap_or(1))
}

pub async fn execute(root: &Path, cli: &Cli, test: bool, arguments: &[OsString]) -> Result<i32> {
    let _guard = cpuv_core::lease(root, false)?;
    let source = profiles::project_root(root)?;
    let manifest = read_manifest(root)?;
    ensure!(
        manifest.dependencies.is_empty() || root.join("cpuv.lock").is_file(),
        "Run cpuv lock to resolve this profile first"
    );
    let env = root.join(".cpuv/env");
    let metadata: cpuv_relocate::Environment =
        serde_json::from_slice(&fs::read(env.join("manifest.json"))?)?;
    super::maintenance::validate_request(root, &metadata)?;
    metadata.tool_identity.verify()?;
    cpuv_core::verify(&env)?;
    let tc: Toolchain = serde_json::from_slice(&fs::read(root.join(".cpuv/toolchain.json"))?)?;
    let specification = manifest.build.clone().unwrap_or_default();
    let mut supplied = specification.clone();
    supplied
        .arguments
        .extend(arguments.iter().map(|a| a.to_string_lossy().into_owned()));
    source::native::validate_build(&supplied)?;
    if specification.system != source::native::System::Cmake {
        return other(
            &BuildContext {
                root,
                cli,
                source: &source,
                env: &env,
                manifest: &manifest,
                tc: &tc,
                metadata: &metadata,
            },
            test,
            arguments,
        )
        .await;
    }
    ensure!(
        source.join("CMakeLists.txt").is_file(),
        "No CMakeLists.txt; use cpuv run for another build system or a direct compiler command"
    );
    let (cmake, ninja) = source::tools::selected_build_tools(cli, &manifest.tools).await?;
    let (tools, launchers) = declared_tools(root, cli, &manifest).await?;
    let mut paths = vec![
        launchers,
        env.join("bin"),
        cmake.parent().unwrap().to_owned(),
        ninja.parent().unwrap().to_owned(),
        tc.compiler_path.parent().unwrap().to_owned(),
    ];
    paths.extend(
        tools
            .values()
            .filter_map(|p| p.parent().map(Path::to_owned)),
    );
    if let Some(host) = cpuv_relocate::msys_host_bin(&metadata.compiler_bin) {
        paths.push(host);
    }
    if let Some(path) = tc
        .environment
        .get("PATH")
        .map(OsString::from)
        .or_else(|| std::env::var_os("PATH"))
    {
        paths.extend(std::env::split_paths(&path));
    }
    let path = manifest
        .runtime
        .clone()
        .unwrap_or_default()
        .path(&source, &std::env::join_paths(paths)?)?;
    let build = root.join(".cpuv/build/cmake");
    fs::create_dir_all(&build)?;
    let output_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(".cpuv/build/build.lock"))?;
    output_lock
        .try_lock_exclusive()
        .context("Another build is using this profile")?;
    ensure!(
        source.join("CMakeLists.txt").is_file(),
        "No CMakeLists.txt; use cpuv run for another build system or a direct compiler command"
    );
    let configuration = metadata.configuration.as_str();
    let mut configure = Command::new(&cmake);
    configure
        .envs(&tc.environment)
        .env_remove("CPPFLAGS")
        .env_remove("CFLAGS")
        .env_remove("CXXFLAGS")
        .env_remove("LDFLAGS")
        .env("PATH", &path)
        .current_dir(&source)
        .arg("-S")
        .arg(&source)
        .arg("-B")
        .arg(&build)
        .args(["-G", "Ninja", "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON"])
        .arg(format!(
            "-DCMAKE_MAKE_PROGRAM={}",
            cpuv_relocate::portable(&ninja)
        ))
        .arg(format!("-DCMAKE_BUILD_TYPE={configuration}"))
        .arg(format!(
            "-DCMAKE_C_COMPILER={}",
            cpuv_relocate::portable(&tc.c_compiler)
        ))
        .arg(format!(
            "-DCMAKE_CXX_COMPILER={}",
            cpuv_relocate::portable(&tc.compiler_path)
        ))
        .arg(format!(
            "-DCMAKE_PREFIX_PATH={}",
            cpuv_relocate::portable(&env)
        ));
    let toolchain = if env.join("vcpkg-toolchain.cmake").is_file() {
        env.join("vcpkg-toolchain.cmake")
    } else {
        env.join("conan_toolchain.cmake")
    };
    if toolchain.is_file() {
        configure.arg("--toolchain").arg(toolchain);
    }
    configure.args(cmake_standard(&metadata.standard));
    for (name, value) in &specification.variables {
        ensure!(
            ![
                "CMAKE_C_COMPILER",
                "CMAKE_CXX_COMPILER",
                "CMAKE_TOOLCHAIN_FILE",
                "CMAKE_MAKE_PROGRAM"
            ]
            .contains(&name.as_str()),
            "Compiler/profile setting {name} is managed by cpuv"
        );
        configure.arg(format!("-D{name}={value}"));
    }
    configure.args(&specification.arguments);
    let status = stage(&mut configure, root, "cmake-configure")?;
    if !status.success() {
        return Ok(status.code().unwrap_or(1));
    }
    let mut compile = Command::new(&cmake);
    compile
        .envs(&tc.environment)
        .env_remove("CPPFLAGS")
        .env_remove("CFLAGS")
        .env_remove("CXXFLAGS")
        .env_remove("LDFLAGS")
        .env("PATH", &path)
        .current_dir(&source)
        .arg("--build")
        .arg(&build)
        .arg("--config")
        .arg(configuration)
        .arg("--parallel")
        .arg(cli.build_jobs.map_or_else(
            || {
                std::thread::available_parallelism()
                    .map_or(1, usize::from)
                    .to_string()
            },
            |j| j.to_string(),
        ))
        .args(if test { &[][..] } else { arguments });
    let status = stage(&mut compile, root, "cmake-build")?;
    if !status.success() || !test {
        return Ok(status.code().unwrap_or(1));
    }
    let ctest = cmake.with_file_name("ctest.exe");
    ensure!(
        ctest.is_file(),
        "Pinned CMake package does not include CTest"
    );
    let mut run = Command::new(ctest);
    run.envs(&tc.environment)
        .env("PATH", &path)
        .arg("--test-dir")
        .arg(&build)
        .args([
            "--output-on-failure",
            "--no-tests=error",
            "-C",
            configuration,
        ])
        .args(&specification.test_arguments)
        .args(arguments);
    if let Some(runtime) = &manifest.runtime {
        run.envs(runtime.variables(&source, &path)?);
    }
    Ok(stage(&mut run, root, "ctest")?.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[tokio::test]
    async fn declared_interpreter_tool_runs_from_verified_offline_archive() {
        use sha2::{Digest, Sha256};
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("cache");
        fs::create_dir_all(cache.join("catalog/archives")).unwrap();
        let interpreter = fs::read(
            PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/cscript.exe"),
        )
        .unwrap();
        let mut tar = tar::Builder::new(Vec::new());
        for (name, data) in [
            ("cscript.exe", interpreter.as_slice()),
            (
                "generator.js",
                b"WScript.Echo(\"pinned-generator\");\r\n".as_slice(),
            ),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, data).unwrap();
        }
        let bytes = zstd::stream::encode_all(tar.into_inner().unwrap().as_slice(), 1).unwrap();
        let digest = format!("{:x}", Sha256::digest(&bytes));
        fs::write(cache.join("catalog/archives").join(&digest), bytes).unwrap();
        let artifact = |executable: &str, interpreter: Option<&str>| source::managed::Artifact {
            version: "1".into(),
            executable: executable.into(),
            interpreter: interpreter.map(String::from),
            conan_package: None,
            version_arguments: vec![],
            url: "http://127.0.0.1:1/tool.tar.zst".into(),
            sha256: digest.clone(),
            archive: "tar.zst".into(),
            strip_components: 0,
        };
        let mut manifest: Manifest = toml::from_str("[project]\nname='test'\nversion='1'\n[toolchain]\ntarget='test'\ncompiler='test'\nabi='test'\n").unwrap();
        manifest.tools = BTreeMap::from([
            ("interpreter".into(), artifact("cscript.exe", None)),
            (
                "generator".into(),
                artifact("generator.js", Some("interpreter")),
            ),
        ]);
        let cli = Cli::parse_from([
            OsString::from("cpuv"),
            "--cache-dir".into(),
            cache.into_os_string(),
            "--offline".into(),
            "info".into(),
        ]);
        let (_, launchers) = declared_tools(root.path(), &cli, &manifest).await.unwrap();
        use cpuv_core::process::CommandOutput;
        let output = Command::new(launchers.join("generator.cmd"))
            .bounded_output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("pinned-generator"));
    }
    #[test]
    fn cmake_standard_preserves_iso_extensions_and_clears_owned_defaults() {
        let iso = cmake_standard("c++03");
        assert!(iso.contains(&OsString::from("-DCMAKE_CXX_STANDARD=98")));
        assert!(iso.contains(&OsString::from("-DCMAKE_CXX_EXTENSIONS=OFF")));
        assert!(cmake_standard("gnu++20").contains(&OsString::from("-DCMAKE_CXX_EXTENSIONS=ON")));
        assert!(cmake_standard("default").contains(&OsString::from("-UCMAKE_CXX_STANDARD")));
    }
}
