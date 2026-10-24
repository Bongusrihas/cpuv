//! Build-system integration uses native package targets and real selected compilers.
use super::*;

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
    ensure!(
        source.join("CMakeLists.txt").is_file(),
        "No CMakeLists.txt; use cpuv run for another build system or a direct compiler command"
    );
    let (cmake, ninja) = source::build_tools(cli).await?;
    let mut paths = vec![
        env.join("bin"),
        cmake.parent().unwrap().to_owned(),
        ninja.parent().unwrap().to_owned(),
        tc.compiler_path.parent().unwrap().to_owned(),
    ];
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
    let path = std::env::join_paths(paths)?;
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
    if metadata.standard != "default" {
        let standard = metadata
            .standard
            .trim_start_matches("gnu++")
            .trim_start_matches("c++");
        configure.arg(format!("-DCMAKE_CXX_STANDARD={standard}"));
    }
    let status = configure.status().context("Configuring CMake project")?;
    if !status.success() {
        return Ok(status.code().unwrap_or(1));
    }
    let status = Command::new(&cmake)
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
        .args(if test { &[][..] } else { arguments })
        .status()?;
    if !status.success() || !test {
        return Ok(status.code().unwrap_or(1));
    }
    let ctest = cmake.with_file_name("ctest.exe");
    ensure!(
        ctest.is_file(),
        "Pinned CMake package does not include CTest"
    );
    Ok(Command::new(ctest)
        .envs(&tc.environment)
        .env("PATH", &path)
        .arg("--test-dir")
        .arg(&build)
        .args([
            "--output-on-failure",
            "--no-tests=error",
            "-C",
            configuration,
        ])
        .args(arguments)
        .status()?
        .code()
        .unwrap_or(1))
}
