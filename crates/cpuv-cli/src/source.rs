//! Community catalog backend: cpuv contains no per-library source recipes.
use super::*;
use std::collections::BTreeSet;
mod archive;
mod tools;
#[derive(Clone, Serialize, Deserialize)]
struct Archive {
    url: String,
    sha256: String,
    archive: String,
    #[serde(default = "one")]
    strip_components: usize,
    #[serde(default)]
    include: Vec<PathBuf>,
    #[serde(default)]
    exclude: Vec<PathBuf>,
}
fn one() -> usize {
    1
}
#[derive(Clone, Serialize, Deserialize)]
struct Tool {
    version: String,
    #[serde(flatten)]
    source: Archive,
    executable: String,
}
#[derive(Serialize, Deserialize)]
struct Registry {
    tools: BTreeMap<String, Tool>,
}
pub fn custom_profile_requested() -> bool {
    ["CPPFLAGS", "CFLAGS", "CXXFLAGS", "LDFLAGS"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Profile {
    cpp: Vec<String>,
    c: Vec<String>,
    cxx: Vec<String>,
    link: Vec<String>,
}

#[cfg(windows)]
fn split_flags(text: &str) -> Result<Vec<String>> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::{Foundation::LocalFree, UI::Shell::CommandLineToArgvW};
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let command = format!("cpuv-flags {text}");
    let wide: Vec<u16> = std::ffi::OsStr::new(&command)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut count = 0;
    let arguments = unsafe { CommandLineToArgvW(wide.as_ptr(), &mut count) };
    ensure!(!arguments.is_null(), "Invalid source build flags");
    let mut result = Vec::new();
    for index in 1..count as usize {
        let start = unsafe { *arguments.add(index) };
        let mut length = 0;
        while unsafe { *start.add(length) } != 0 {
            length += 1;
        }
        result.push(
            OsString::from_wide(unsafe { std::slice::from_raw_parts(start, length) })
                .to_string_lossy()
                .into_owned(),
        );
    }
    unsafe {
        LocalFree(arguments.cast());
    }
    Ok(result)
}
#[cfg(not(windows))]
fn split_flags(text: &str) -> Result<Vec<String>> {
    Ok(text.split_whitespace().map(str::to_owned).collect())
}

impl Profile {
    fn environment(previous: Option<&Self>) -> Result<Self> {
        let previous = previous.cloned().unwrap_or_default();
        let flags = |name, saved: Vec<String>| -> Result<Vec<String>> {
            match std::env::var(name) {
                Ok(value) => split_flags(&value),
                Err(std::env::VarError::NotPresent) => Ok(saved),
                Err(error) => Err(error.into()),
            }
        };
        Ok(Self {
            cpp: flags("CPPFLAGS", previous.cpp)?,
            c: flags("CFLAGS", previous.c)?,
            cxx: flags("CXXFLAGS", previous.cxx)?,
            link: flags("LDFLAGS", previous.link)?,
        })
    }
    fn compiler_flags(&self, standard: &str) -> Vec<OsString> {
        let mut flags: Vec<OsString> = self
            .cpp
            .iter()
            .chain(&self.cxx)
            .map(OsString::from)
            .collect();
        if standard != "default"
            && !flags
                .iter()
                .any(|s| s.to_string_lossy().starts_with("-std="))
        {
            flags.push(format!("-std={standard}").into());
        }
        flags
    }
}

#[derive(Serialize, Deserialize)]
struct CatalogLock {
    version: u32,
    backend: String,
    dependencies: BTreeMap<String, Dependency>,
    profile: Profile,
    gcc_version: String,
    abi: cpuv_toolchain::AbiTuple,
    packages: BTreeMap<String, String>,
    upstream_lock: String,
}

pub fn requests_locked(root: &Path, manifest: &Manifest, requests: &[String]) -> bool {
    let locked: Option<CatalogLock> = fs::read_to_string(root.join("cpuv.lock"))
        .ok()
        .and_then(|text| toml::from_str(&text).ok());
    locked.is_some_and(|lock| {
        lock.version == 3
            && lock.dependencies == manifest.dependencies
            && requests.iter().all(|request| {
                let (name, op, value) = cpuv_solver::split_requirement(request);
                manifest.dependencies.get(name).is_some_and(|dependency| {
                    dependency.version().is_ok_and(|constraint| {
                        op.is_empty() || constraint == format!("{op}{value}")
                    })
                })
            })
    })
}

fn standard(tc: &Toolchain) -> Result<String> {
    let value = tc
        .observed_abi_options
        .get("__cplusplus")
        .context("GCC did not report its C++ language mode")?;
    let year: u32 = value
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .parse()?;
    Ok(format!(
        "{}{}",
        if tc
            .observed_abi_options
            .get("__STRICT_ANSI__")
            .is_some_and(|v| v != "not defined")
        {
            ""
        } else {
            "gnu"
        },
        if year >= 202400 {
            26
        } else if year >= 202302 {
            23
        } else if year >= 202002 {
            20
        } else if year >= 201703 {
            17
        } else if year >= 201402 {
            14
        } else if year >= 201103 {
            11
        } else {
            98
        }
    ))
}

fn reference(name: &str, dependency: &Dependency) -> Result<String> {
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-.+".contains(&c)),
        "Invalid catalog package name {name}; use the exact ConanCenter name"
    );
    let version = dependency.version()?;
    ensure!(
        version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".*_+-<>=:".contains(&c)),
        "Invalid catalog version constraint"
    );
    Ok(format!(
        "{name}/{}",
        if version == "*" {
            "[*]".into()
        } else if version.starts_with(['<', '>']) {
            format!("[{version}]")
        } else {
            version.trim_start_matches('=').to_owned()
        }
    ))
}

fn command(executable: &Path, home: &Path, path: &std::ffi::OsStr) -> Command {
    let mut value = Command::new(executable);
    value
        .env("CONAN_HOME", home)
        .env("PATH", path)
        .env_remove("CC")
        .env_remove("CXX");
    value
}

fn run(command: &mut Command, log: &Path) -> Result<()> {
    use std::io::Write;
    let mut out = OpenOptions::new().create(true).append(true).open(log)?;
    writeln!(out, "command: {command:?}")?;
    let status = command
        .stdout(out.try_clone()?)
        .stderr(out)
        .status()
        .context("Starting the community package provider")?;
    if !status.success() {
        let text = fs::read_to_string(log)?;
        let tail = text
            .lines()
            .rev()
            .take(28)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "Community catalog install failed ({status}). Log: {}\n{tail}\nA package must exist in the catalog and support the selected compiler; cpuv does not guess missing build instructions",
            log.display()
        );
    }
    Ok(())
}

fn configure_profile(tc: &Toolchain, profile: &Profile, key: &str, jobs: usize) -> Result<String> {
    let arch = match tc.abi.arch.as_str() {
        "x86_64" => "x86_64",
        "x86" => "x86",
        "aarch64" => "armv8",
        value => bail!("Unsupported catalog architecture {value}"),
    };
    ensure!(
        tc.abi.crt_runtime != "cygwin",
        "The catalog GCC adapter currently supports native MinGW GCC, not Cygwin"
    );
    let c = tc.compiler_path.with_file_name(
        tc.compiler_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace("g++", "gcc"),
    );
    ensure!(
        c.is_file(),
        "Catalog builds need the matching gcc next to g++"
    );
    let cpp: Vec<_> = profile.cpp.iter().chain(&profile.cxx).cloned().collect();
    let cflags: Vec<_> = profile.cpp.iter().chain(&profile.c).cloned().collect();
    let conf = BTreeMap::from([
        (
            "tools.build:compiler_executables",
            serde_json::json!({"c":cpuv_relocate::portable(&c),"cpp":cpuv_relocate::portable(&tc.compiler_path)}),
        ),
        ("tools.build:cxxflags", serde_json::json!(cpp)),
        ("tools.build:cflags", serde_json::json!(cflags)),
        ("tools.build:exelinkflags", serde_json::json!(profile.link)),
        (
            "tools.build:sharedlinkflags",
            serde_json::json!(profile.link),
        ),
        ("tools.build:jobs", serde_json::json!(jobs)),
        (
            "tools.cmake.cmaketoolchain:generator",
            serde_json::json!("Ninja"),
        ),
        (
            "tools.cmake.cmaketoolchain:extra_variables",
            serde_json::json!({"CMAKE_CXX_FLAGS_RELEASE":"","CMAKE_C_FLAGS_RELEASE":""}),
        ),
        (
            "tools.info.package_id:confs",
            serde_json::json!(["user.cpuv:profile"]),
        ),
        ("user.cpuv:profile", serde_json::json!(key)),
    ]);
    let mut output = format!(
        "[settings]\nos=Windows\narch={arch}\ncompiler=gcc\ncompiler.version={}\ncompiler.libcxx={}\ncompiler.cppstd={}\nbuild_type=Release\n[conf]\n",
        tc.abi.compiler_abi_major,
        if tc.abi.glibcxx_cxx11_abi == "0" {
            "libstdc++"
        } else {
            "libstdc++11"
        },
        standard(tc)?
    );
    for (name, value) in conf {
        let encoded = match value {
            serde_json::Value::String(value) => value,
            value => serde_json::to_string(&value)?,
        };
        ensure!(
            !encoded.contains(['\n', '\r']),
            "Invalid newline in catalog build profile"
        );
        output.push_str(&format!("{name}={encoded}\n"));
    }
    Ok(output)
}

fn modules_for(node: &serde_json::Value, name: &str) -> Vec<String> {
    let mut modules = vec![
        node["cpp_info"]["root"]["properties"]["pkg_config_name"]
            .as_str()
            .unwrap_or(name)
            .to_owned(),
    ];
    if let Some(parts) = node["cpp_info"].as_object() {
        for (component, info) in parts {
            if let Some(aliases) = info["properties"]["pkg_config_aliases"].as_array() {
                modules.extend(
                    aliases
                        .iter()
                        .filter_map(|alias| alias.as_str().map(str::to_owned)),
                );
            }
            if component != "root" {
                modules.push(
                    info["properties"]["pkg_config_name"]
                        .as_str()
                        .map_or_else(|| format!("{name}-{component}"), str::to_owned),
                );
            }
        }
    }
    modules.retain(|module| module != "none");
    modules.sort();
    modules.dedup();
    modules
}

fn prepare_metadata(
    env: &Path,
    final_env: &Path,
    manifest: &Manifest,
    tc: &Toolchain,
    profile: &Profile,
    graph: &serde_json::Value,
) -> Result<BTreeMap<String, String>> {
    let mut metadata = cpuv_relocate::Environment {
        schema: 1,
        compiler: tc.compiler_path.clone(),
        compiler_bin: tc.compiler_path.parent().unwrap().to_owned(),
        abi_hash: tc.abi_hash.clone(),
        standard: manifest.project.std.clone(),
        include_dirs: vec![],
        libraries: vec![],
        defines: vec![],
        cflags: vec![],
        include_map: BTreeMap::new(),
        link_model: 2,
        link_flags: vec![],
        library_dirs: vec![],
        managed_binaries: false,
        dependency_origin: "source".into(),
        source_arch: tc.abi.arch.clone(),
        source_flags: profile.cpp.iter().chain(&profile.cxx).cloned().collect(),
    };
    let nodes = graph["graph"]["nodes"]
        .as_object()
        .context("Catalog returned no dependency graph")?;
    let mut packages = Vec::new();
    let mut versions = BTreeMap::new();
    let mut owned_modules = BTreeSet::new();
    let stage_path = cpuv_relocate::portable(env);
    for node in nodes
        .values()
        .filter(|n| n["context"] == "host" && n["package_folder"].is_string())
    {
        let reference = node["ref"]
            .as_str()
            .context("Catalog package has no reference")?;
        let (name, version) = reference
            .split_once('/')
            .context("Malformed catalog reference")?;
        versions.insert(
            name.to_owned(),
            version.split('#').next().unwrap().to_owned(),
        );
        let folder = Path::new(node["package_folder"].as_str().unwrap());
        ensure!(
            folder.canonicalize()?.starts_with(env.canonicalize()?),
            "Catalog deployment escapes staged environment"
        );
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(folder) {
            let entry = entry?;
            ensure!(
                !entry.file_type().is_symlink(),
                "Catalog deployment contains a symlink"
            );
            if entry.file_type().is_file() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "dll") {
                    let destination = env.join("bin").join(path.file_name().unwrap());
                    if destination.exists() {
                        ensure!(
                            archive::digest(&destination)? == archive::digest(path)?,
                            "Conflicting catalog runtime DLL {}",
                            path.display()
                        );
                    } else {
                        fs::copy(path, destination)?;
                    }
                }
                if !path.extension().is_some_and(|ext| ext == "pc") {
                    files.push(path.strip_prefix(env)?.to_owned());
                }
            }
        }
        for module in modules_for(node, name) {
            ensure!(
                !module.contains(['/', '\\', ':']),
                "Unsafe catalog module name"
            );
            let pc = env.join(format!("{module}.pc"));
            if pc.is_file() {
                ensure!(
                    owned_modules.insert(module.clone()),
                    "Duplicate catalog module {module}"
                );
                let text = fs::read_to_string(&pc)?
                    .replace(&stage_path, "${pcfiledir}")
                    .replace(&env.to_string_lossy().to_string(), "${pcfiledir}");
                fs::write(&pc, text)?;
                files.push(PathBuf::from(format!("{module}.pc")));
            }
        }
        packages.push(cpuv_relocate::linking::PackageFiles {
            name: cpuv_solver::normalize(name),
            files,
        });
    }
    cpuv_relocate::linking::apply(
        env,
        &mut metadata,
        &packages,
        &requirements(manifest)?,
        &manifest.link,
    )?;
    metadata.dependency_origin = "source".into();
    metadata
        .cflags
        .extend(profile.cpp.iter().chain(&profile.cxx).cloned());
    metadata.link_flags.extend(profile.link.clone());
    cpuv_relocate::validate_profile(&metadata, &[])?;
    let inspection =
        cpuv_relocate::inspect::environment_with_runtime_policy(env, &tc.abi.arch, false)?;
    fs::write(
        env.join("compatibility.json"),
        serde_json::to_vec_pretty(&inspection)?,
    )?;
    fs::write(
        env.join("manifest.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    fs::write(
        env.join("cpuv.specs"),
        "*cc1plus:\n+ %ecpuv catalog profiles require the compiler shim\n\n",
    )?;
    // Conan's generated CMake files and graph are local diagnostics. Keep their
    // references valid after the staged environment is committed.
    for entry in walkdir::WalkDir::new(env) {
        let entry = entry?;
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "cmake")
        {
            let text = fs::read_to_string(entry.path())?;
            fs::write(
                entry.path(),
                text.replace(&stage_path, &cpuv_relocate::portable(final_env))
                    .replace(
                        &env.to_string_lossy().to_string(),
                        &final_env.to_string_lossy(),
                    ),
            )?;
        }
    }
    Ok(versions)
}

pub async fn sync(
    root: &Path,
    cli: &Cli,
    manifest: &Manifest,
    base_tc: &Toolchain,
    refresh: bool,
) -> Result<()> {
    let started = Instant::now();
    let old_text = match fs::read_to_string(root.join("cpuv.lock")) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).context("Reading cpuv.lock"),
    };
    #[derive(Deserialize)]
    struct SavedProfile {
        version: u32,
        #[serde(default)]
        profile: Profile,
    }
    let saved: Option<SavedProfile> = old_text
        .as_deref()
        .map(toml::from_str)
        .transpose()
        .context("Invalid cpuv.lock")?;
    ensure!(
        saved
            .as_ref()
            .is_none_or(|lock| (1..=3).contains(&lock.version)),
        "Unsupported cpuv.lock version"
    );
    let old: Option<CatalogLock> = if saved.as_ref().is_some_and(|lock| lock.version == 3) {
        Some(toml::from_str(old_text.as_deref().unwrap()).context("Invalid catalog lock")?)
    } else {
        None
    };
    let profile = Profile::environment(if cli.reset_profile {
        None
    } else {
        saved.as_ref().map(|value| &value.profile)
    })?;
    let flags = profile.compiler_flags(&manifest.project.std);
    let tc = cpuv_toolchain::probe_with_flags(Some(&base_tc.compiler_path), &flags)?;
    let mut doctor_flags = flags.clone();
    doctor_flags.extend(profile.link.iter().map(OsString::from));
    cpuv_toolchain::doctor(&tc, &doctor_flags)?;
    let registry: Registry = toml::from_str(include_str!("../../../build-tools.toml"))?;
    let base = cache_dir(cli)?.join("catalog");
    fs::create_dir_all(&base)?;
    let key = blake3::hash(&serde_json::to_vec(&(
        1,
        &tc,
        &profile,
        &manifest.project.std,
        archive::digest(&tc.compiler_path)?,
    ))?)
    .to_hex()
    .to_string();
    let home = base.join("homes").join(&key);
    fs::create_dir_all(&home)?;
    let guard = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(home.join("cpuv.lock"))?;
    guard.try_lock_exclusive().context(
        "Another catalog operation is using this compiler/profile; retry when it finishes",
    )?;
    let tools = tools::prepare(&registry, &base, cli.offline, &cli.transfer_limits()).await?;
    let mut paths = vec![
        tc.compiler_path.parent().unwrap().to_owned(),
        tools.cmake.parent().unwrap().to_owned(),
        tools.ninja.parent().unwrap().to_owned(),
    ];
    if let Some(host) = cpuv_relocate::msys_host_bin(tc.compiler_path.parent().unwrap()) {
        paths.push(host);
    }
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    let path = std::env::join_paths(paths)?;
    let jobs = cli
        .build_jobs
        .map(usize::from)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from));
    let profile_path = home.join("cpuv-profile");
    fs::write(&profile_path, configure_profile(&tc, &profile, &key, jobs)?)?;
    let stage = tempfile::Builder::new()
        .prefix("stage-catalog-")
        .tempdir_in(root.join(".cpuv"))?;
    let env = stage.path().join("env");
    fs::create_dir_all(env.join("bin"))?;
    let graph_file = stage.path().join("graph.json");
    let upstream_lock = stage.path().join("conan.lock");
    let input_lock = stage.path().join("input.lock");
    let use_lock = !refresh
        && old
            .as_ref()
            .is_some_and(|lock| lock.version == 3 && lock.dependencies == manifest.dependencies);
    if use_lock {
        fs::write(&input_lock, &old.as_ref().unwrap().upstream_lock)?;
    }
    let log_dir = base.join("logs");
    fs::create_dir_all(&log_dir)?;
    let log = log_dir.join(format!(
        "{}-{}.log",
        std::process::id(),
        started.elapsed().as_nanos()
    ));
    let mut install = command(&tools.conan, &home, &path);
    // Validate provider manifests before executing cached recipes or deploying
    // cached binaries. Conan owns its package cache and its manifest format.
    run(
        command(&tools.conan, &home, &path).args(["cache", "check-integrity", "*:*", "-vwarning"]),
        &log,
    )?;
    install
        .args(["install", "-pr:h"])
        .arg(&profile_path)
        .arg("-pr:b")
        .arg(&profile_path)
        .arg(if cli.from_source {
            "--build=*"
        } else {
            "--build=missing"
        })
        .args(["-g", "PkgConfigDeps", "-d", "full_deploy", "-of"])
        .arg(&env)
        .args(["--format=json", "--out-file"])
        .arg(&graph_file)
        .arg("--lockfile-out")
        .arg(&upstream_lock)
        .args([
            "-cc",
            &format!("core.download:parallel={}", cli.concurrent_downloads),
            "-cc",
            "core:non_interactive=True",
        ])
        .current_dir(stage.path());
    if use_lock {
        install.arg("--lockfile").arg(&input_lock);
    } else {
        install.arg("--lockfile=");
    }
    if cli.offline {
        install.arg("--no-remote");
    }
    if refresh && !cli.offline {
        install.arg("--update");
    }
    for (name, dependency) in &manifest.dependencies {
        install.arg(format!("--requires={}", reference(name, dependency)?));
    }
    if manifest.dependencies.is_empty() {
        fs::write(stage.path().join("conanfile.txt"), "[requires]\n")?;
        install.arg(stage.path().join("conanfile.txt"));
    }
    eprintln!(
        "Resolving through ConanCenter using GCC {} ({}). Build log: {}",
        tc.gcc_version,
        tc.runtime_tag(),
        log.display()
    );
    run(&mut install, &log)?;
    run(
        command(&tools.conan, &home, &path).args(["cache", "check-integrity", "*:*", "-vwarning"]),
        &log,
    )?;
    let graph: serde_json::Value = serde_json::from_slice(&fs::read(&graph_file)?)?;
    let versions = prepare_metadata(
        &env,
        &root.join(".cpuv/env"),
        manifest,
        &tc,
        &profile,
        &graph,
    )?;
    let locked = CatalogLock {
        version: 3,
        backend: "conan".into(),
        dependencies: manifest.dependencies.clone(),
        profile,
        gcc_version: tc.gcc_version.clone(),
        abi: tc.abi.clone(),
        packages: versions,
        upstream_lock: fs::read_to_string(upstream_lock)?,
    };
    fs::write(
        env.join("catalog-profile.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"compiler":tc,"cache_key":key,"graph":graph}),
        )?,
    )?;
    let inventory = archive::inventory(&env)?;
    archive::verify(&env, &inventory)?;
    fs::write(
        env.join("catalog-files.json"),
        serde_json::to_vec_pretty(&inventory)?,
    )?;
    let mut manifest = manifest.clone();
    manifest.backend = Backend::Conan;
    manifest.toolchain = PinnedToolchain {
        target: tc.abi.target_triple.clone(),
        compiler: format!("gcc@{}", tc.gcc_version),
        abi: tc.runtime_tag(),
    };
    fs::write(
        stage.path().join("cpuv.toml"),
        toml::to_string_pretty(&manifest)?,
    )?;
    fs::write(
        stage.path().join("cpuv.lock"),
        toml::to_string_pretty(&locked)?,
    )?;
    fs::write(
        stage.path().join("toolchain.json"),
        serde_json::to_vec_pretty(&tc)?,
    )?;
    fs::write(stage.path().join("activate.ps1"), activation(root, &tc)?)?;
    install_shim(root)?;
    commit(root, stage)?;
    fs::write(
        root.join(".cpuv/last-sync.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"backend":"conan","packages":locked.packages.len(),"total_ms":started.elapsed().as_secs_f64()*1000.0}),
        )?,
    )?;
    eprintln!(
        "Ready from community catalog: {} packages",
        locked.packages.len()
    );
    Ok(())
}

pub fn info(root: &Path) -> Result<()> {
    let lock: CatalogLock = toml::from_str(&fs::read_to_string(root.join("cpuv.lock"))?)
        .context("Legacy source lock; run cpuv sync to migrate to the community catalog")?;
    println!(
        "Backend: ConanCenter\nCompiler recorded by lock: GCC {}",
        lock.gcc_version
    );
    for (name, version) in lock.packages {
        println!("{name} {version}");
    }
    Ok(())
}

fn commit(root: &Path, stage: tempfile::TempDir) -> Result<()> {
    let targets = [
        (".cpuv/env", "env"),
        ("cpuv.toml", "cpuv.toml"),
        ("cpuv.lock", "cpuv.lock"),
        (".cpuv/toolchain.json", "toolchain.json"),
        (".cpuv/activate.ps1", "activate.ps1"),
    ];
    let mut committed = Vec::new();
    let transaction: Result<()> = (|| {
        for (index, (target, staged)) in targets.iter().enumerate() {
            let destination = root.join(target);
            let backup = stage.path().join(format!("backup-{index}"));
            let existed = destination.exists();
            if existed {
                fs::rename(&destination, &backup)?;
            }
            committed.push((index, existed));
            fs::rename(stage.path().join(staged), &destination)?;
        }
        Ok(())
    })();
    if let Err(error) = transaction {
        let mut recovery_errors = Vec::new();
        for (index, existed) in committed.into_iter().rev() {
            let (target, _) = targets[index];
            let destination = root.join(target);
            if destination.exists()
                && let Err(failure) = fs::rename(
                    &destination,
                    stage.path().join(format!("uncommitted-{index}")),
                )
            {
                recovery_errors.push(failure.to_string());
                continue;
            }
            if existed
                && let Err(failure) = fs::rename(
                    stage.path().join(format!("backup-{index}")),
                    root.join(target),
                )
            {
                recovery_errors.push(failure.to_string());
            }
        }
        if !recovery_errors.is_empty() {
            let recovery = stage.keep();
            bail!(
                "Catalog update failed: {error:#}. Automatic restoration was incomplete: {}. Preserved backups: {}",
                recovery_errors.join("; "),
                recovery.display()
            );
        }
        return Err(error);
    }
    Ok(())
}
