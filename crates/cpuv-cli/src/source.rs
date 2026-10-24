//! Community catalog backend: cpuv contains no per-library source recipes.
use super::*;
use std::collections::BTreeSet;
mod archive;
mod tools;
pub async fn build_tools(cli: &Cli) -> Result<(PathBuf, PathBuf)> {
    let registry: Registry = toml::from_str(include_str!("../../../build-tools.toml"))?;
    tools::build_tools(
        &registry,
        &cache_dir(cli)?.join("catalog"),
        cli.offline,
        &cli.transfer_limits(),
    )
    .await
}
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
    // Provider declarations are not part of the downloaded artifact's identity.
    #[serde(default, skip_serializing)]
    conan_package: Option<String>,
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

pub fn explicit_references(manifest: &Manifest) -> bool {
    manifest.dependencies.values().any(|dependency| {
        matches!(
            dependency,
            Dependency::Detailed {
                reference: Some(_),
                ..
            }
        )
    })
}

pub fn reference_name(reference: &str) -> Result<&str> {
    let (name, version) = reference
        .split_once('/')
        .context("A Conan recipe reference must include name/version")?;
    ensure!(
        !name.is_empty()
            && !version.is_empty()
            && name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-.+".contains(&c))
            && version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-.+/@#".contains(&c))
            && !reference.contains("//"),
        "Invalid Conan recipe reference; use name/version[@user/channel][#revision]"
    );
    Ok(name)
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Profile {
    cpp: Vec<String>,
    c: Vec<String>,
    cxx: Vec<String>,
    link: Vec<String>,
    #[serde(default = "release_configuration")]
    configuration: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            cpp: vec![],
            c: vec![],
            cxx: vec![],
            link: vec![],
            configuration: release_configuration(),
        }
    }
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
            configuration: previous.configuration,
            cpp: flags("CPPFLAGS", previous.cpp)?,
            c: flags("CFLAGS", previous.c)?,
            cxx: flags("CXXFLAGS", previous.cxx)?,
            link: flags("LDFLAGS", previous.link)?,
        })
    }
    fn compiler_flags(&self, standard: &str, msvc: bool) -> Vec<OsString> {
        let mut flags: Vec<OsString> = self
            .cpp
            .iter()
            .chain(&self.cxx)
            .map(OsString::from)
            .collect();
        if standard != "default"
            && !flags.iter().any(|s| {
                let text = s.to_string_lossy();
                text.starts_with("-std=") || text.starts_with("/std:")
            })
        {
            flags.push(if msvc {
                format!(
                    "/std:{}",
                    if ["c++23", "c++26", "gnu++23", "gnu++26"].contains(&standard) {
                        "c++latest".to_owned()
                    } else {
                        standard.replace("gnu++", "c++")
                    }
                )
                .into()
            } else {
                format!("-std={standard}").into()
            });
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
                if request.contains('/') {
                    return reference_name(request).is_ok_and(|name| matches!(manifest.dependencies.get(name), Some(Dependency::Detailed { reference: Some(saved), .. }) if saved == request));
                }
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
    if let Dependency::Detailed {
        reference: Some(value),
        ..
    } = dependency
    {
        ensure!(
            reference_name(value)? == name,
            "Conan reference name disagrees with dependency key {name}"
        );
        ensure!(
            dependency.version()? == "*",
            "Use the reference's version rather than a separate version constraint"
        );
        return Ok(value.clone());
    }
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
        let explanation = if tail
            .lines()
            .any(|line| line.starts_with("ERROR: Package '") && line.contains("not resolved:"))
        {
            "Conan could not resolve this package name/version; see the remote error above for the cause. Resolution failed before compilation, so this error does not establish a GCC, linker, or ABI incompatibility. A package must be published in the selected catalog; cpuv cannot infer an unregistered library's build instructions."
        } else {
            "See the provider error above and the full log for the failed operation. Catalog availability, compiler support, network access, and build requirements are determined by the upstream provider."
        };
        bail!(
            "Community catalog command failed ({status}). Log: {}\n{tail}\n{explanation}",
            log.display()
        );
    }
    Ok(())
}

fn configure_profile(
    tc: &Toolchain,
    profile: &Profile,
    key: &str,
    jobs: usize,
    platform_tools: &[String],
) -> Result<String> {
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
    let c = &tc.c_compiler;
    ensure!(
        c.is_file(),
        "Catalog builds need the matching C compiler: {}",
        c.display()
    );
    let cpp: Vec<_> = profile.cpp.iter().chain(&profile.cxx).cloned().collect();
    let cflags: Vec<_> = profile.cpp.iter().chain(&profile.c).cloned().collect();
    let conf = BTreeMap::from([
        (
            "tools.build:compiler_executables",
            serde_json::json!({"c":cpuv_relocate::portable(c),"cpp":cpuv_relocate::portable(&tc.compiler_path)}),
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
    let runtime = if tc.driver == "msvc" {
        format!(
            "compiler.runtime=dynamic\ncompiler.runtime_type={}\n",
            profile.configuration
        )
    } else {
        format!(
            "compiler.libcxx={}\n",
            if tc.abi.stdlib == "libc++" {
                "libc++"
            } else if tc.abi.glibcxx_cxx11_abi == "0" {
                "libstdc++"
            } else {
                "libstdc++11"
            }
        )
    };
    let mut output = format!(
        "[settings]\nos=Windows\narch={arch}\ncompiler={}\ncompiler.version={}\n{runtime}compiler.cppstd={}\nbuild_type={}\n[conf]\n",
        tc.abi.compiler_family,
        tc.abi.compiler_abi_major,
        standard(tc)?,
        profile.configuration
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
    if !tc.environment.is_empty() {
        output.push_str("[buildenv]\n");
        for name in ["INCLUDE", "LIB", "LIBPATH"] {
            if let Some(value) = tc.environment.get(name) {
                ensure!(
                    !value.contains(['\n', '\r']),
                    "Invalid developer environment value"
                );
                output.push_str(&format!("{name}={value}\n"));
            }
        }
    }
    append_platform_tools(&mut output, platform_tools)?;
    Ok(output)
}

fn append_platform_tools(output: &mut String, tools: &[String]) -> Result<()> {
    if !tools.is_empty() {
        output.push_str("[platform_tool_requires]\n");
        for tool in tools {
            reference_name(tool)?;
            output.push_str(tool);
            output.push('\n');
        }
    }
    Ok(())
}

fn build_architecture() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("x86_64"),
        "x86" => Ok("x86"),
        "aarch64" => Ok("armv8"),
        value => bail!("Unsupported build-tool execution architecture {value}"),
    }
}

fn native_build_compiler(cli: &Cli, base: &Toolchain) -> Result<Option<Toolchain>> {
    let native_arch = match build_architecture()? {
        "armv8" => "aarch64",
        value => value,
    };
    let selected = if let Some(compiler) = &cli.build_compiler {
        let tc = cpuv_toolchain::probe(Some(compiler))?;
        ensure!(
            tc.abi.arch == native_arch,
            "--build-compiler must produce {native_arch} executables for this cpuv host, not {}",
            tc.abi.arch
        );
        Some(tc)
    } else if base.abi.arch == native_arch {
        Some(base.clone())
    } else {
        cpuv_toolchain::installed()?
            .into_iter()
            .find(|tc| tc.abi.arch == native_arch && tc.abi.crt_runtime != "cygwin")
    };
    if let Some(tc) = &selected {
        cpuv_toolchain::doctor(tc, &[])?;
    }
    Ok(selected)
}

fn configure_build_profile(
    native: Option<&Toolchain>,
    jobs: usize,
    tools: &[String],
) -> Result<String> {
    if let Some(tc) = native {
        let key = blake3::hash(&serde_json::to_vec(&(
            tc,
            archive::digest(&tc.compiler_path)?,
            tools,
        ))?)
        .to_hex()
        .to_string();
        configure_profile(tc, &Profile::default(), &key, jobs, tools)
    } else {
        let mut output = format!(
            "[settings]\nos=Windows\narch={}\nbuild_type=Release\n[conf]\ntools.build:jobs={jobs}\n",
            build_architecture()?
        );
        append_platform_tools(&mut output, tools)?;
        Ok(output)
    }
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

fn verify_requested_linkage(
    name: &str,
    dependency: Option<&Dependency>,
    node: &serde_json::Value,
    files: &[PathBuf],
) -> Result<()> {
    let Some(Dependency::Detailed {
        linkage: Some(linkage),
        ..
    }) = dependency
    else {
        return Ok(());
    };
    let shared = node["options"]["shared"].as_bool().or_else(|| {
        node["options"]["shared"].as_str().and_then(|value| {
            if value.eq_ignore_ascii_case("true") {
                Some(true)
            } else if value.eq_ignore_ascii_case("false") {
                Some(false)
            } else {
                None
            }
        })
    });
    let expected = linkage == "shared";
    ensure!(
        shared == Some(expected),
        "Package {name} did not resolve the requested {linkage} linkage; choose a recipe/configuration that supports it"
    );
    let artifact = files.iter().any(|path| {
        let path = path.to_string_lossy().to_ascii_lowercase();
        if expected {
            path.ends_with(".dll")
        } else {
            (path.ends_with(".a") && !path.ends_with(".dll.a")) || path.ends_with(".lib")
        }
    });
    ensure!(
        artifact,
        "Package {name} declares {linkage} linkage but supplies no matching Windows artifact; unsupported/header-only recipes cannot force this linkage"
    );
    Ok(())
}

fn prepare_metadata(
    env: &Path,
    final_env: &Path,
    manifest: &Manifest,
    tc: &Toolchain,
    profile: &Profile,
    graph: &serde_json::Value,
    identity: &cpuv_core::ToolIdentity,
) -> Result<BTreeMap<String, String>> {
    fs::write(env.join("request.toml"), toml::to_string_pretty(manifest)?)?;
    let mut metadata = cpuv_relocate::Environment {
        schema: 1,
        request_hash: request_hash(manifest)?,
        native_build_only: false,
        compiler: tc.compiler_path.clone(),
        compiler_bin: tc.compiler_path.parent().unwrap().to_owned(),
        driver: tc.driver.clone(),
        process_environment: tc.environment.clone(),
        abi_hash: tc.abi_hash.clone(),
        tool_identity: identity.clone(),
        configuration: manifest.project.configuration.clone(),
        standard: manifest.project.std.clone(),
        include_dirs: vec![],
        libraries: vec![],
        defines: vec![],
        cflags: vec![],
        include_map: BTreeMap::new(),
        link_model: 3,
        link_flags: vec![],
        link_items: vec![],
        link_group: false,
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
                if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
                {
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
                if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("pc"))
                {
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
        verify_requested_linkage(name, manifest.dependencies.get(name), node, &files)?;
        packages.push(cpuv_relocate::linking::PackageFiles {
            name: name.to_owned(),
            static_link: node["options"]["shared"].as_bool() == Some(false)
                || node["options"]["shared"].as_str() == Some("False"),
            files,
        });
    }
    cpuv_relocate::linking::apply_for(
        env,
        &mut metadata,
        &packages,
        &requirements(manifest)?,
        &manifest.link,
        "",
    )?;
    metadata.dependency_origin = "source".into();
    metadata
        .cflags
        .extend(profile.cpp.iter().chain(&profile.cxx).cloned());
    metadata.link_flags.extend(profile.link.clone());
    metadata.link_items.extend(profile.link.clone());
    cpuv_relocate::validate_profile(&metadata, &[])?;
    let inspection =
        cpuv_relocate::inspect::environment_for_runtime(env, &tc.abi.arch, &tc.abi.crt_runtime)?;
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

struct CatalogContext {
    tc: Toolchain,
    identity: cpuv_core::ToolIdentity,
    base: PathBuf,
    home: PathBuf,
    tools: tools::Tools,
    path: OsString,
    profile_path: PathBuf,
    build_profile_path: PathBuf,
    key: String,
    _guard: File,
}

async fn prepare_context(
    cli: &Cli,
    base_tc: &Toolchain,
    profile: &Profile,
    standard: &str,
) -> Result<CatalogContext> {
    if base_tc.driver == "msvc" {
        let expected = if profile.configuration == "Debug" {
            "/MDd"
        } else {
            "/MD"
        };
        for flag in profile.cpp.iter().chain(&profile.c).chain(&profile.cxx) {
            if ["/MD", "/MDd", "/MT", "/MTd"].contains(&flag.as_str()) {
                ensure!(
                    flag == expected,
                    "Runtime flag {flag} disagrees with configuration {}; use {expected}",
                    profile.configuration
                );
            }
        }
    }
    let flags = profile.compiler_flags(standard, base_tc.driver == "msvc");
    let tc = cpuv_toolchain::probe_with_flags(Some(&base_tc.compiler_path), &flags)?;
    let mut doctor_flags = flags.clone();
    doctor_flags.extend(profile.link.iter().map(OsString::from));
    cpuv_toolchain::doctor(&tc, &doctor_flags)?;
    let registry: Registry = toml::from_str(include_str!("../../../build-tools.toml"))?;
    let base = cache_dir(cli)?.join("catalog");
    fs::create_dir_all(&base)?;
    let identity = cpuv_toolchain::identity(&tc)?;
    let key = blake3::hash(&serde_json::to_vec(&(
        1, &tc, &profile, standard, &identity,
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
    let native = native_build_compiler(cli, base_tc)?;
    let mut paths = vec![
        tools.cmake.parent().unwrap().to_owned(),
        tools.ninja.parent().unwrap().to_owned(),
        tc.compiler_path.parent().unwrap().to_owned(),
    ];
    if let Some(host) = cpuv_relocate::msys_host_bin(tc.compiler_path.parent().unwrap()) {
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
    let jobs = cli
        .build_jobs
        .map(usize::from)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from));
    let profile_path = home.join("cpuv-profile");
    let build_profile_path = home.join("cpuv-build-profile");
    fs::write(
        &profile_path,
        configure_profile(&tc, profile, &key, jobs, &tools.platform_references)?,
    )?;
    fs::write(
        &build_profile_path,
        configure_build_profile(native.as_ref(), jobs, &tools.platform_references)?,
    )?;
    if native.is_none() {
        eprintln!(
            "No native build compiler found; verified platform tools remain available. Tool recipes requiring compilation need --build-compiler or CPUV_BUILD_CXX."
        );
    }
    Ok(CatalogContext {
        tc,
        identity,
        base,
        home,
        tools,
        path,
        profile_path,
        build_profile_path,
        key,
        _guard: guard,
    })
}

/// Upstream Conan commands share the exact compiler/profile home used by sync.
pub async fn execute(root: &Path, cli: &Cli, arguments: &[OsString]) -> Result<i32> {
    let _project = project_lock(root)?;
    let tc = cpuv_toolchain::probe(cli.compiler.as_deref())?;
    let standard = if root.join("cpuv.toml").is_file() {
        read_manifest(root)?.project.std
    } else {
        tc.default_standard()
    };
    #[derive(Default, Deserialize)]
    struct Saved {
        #[serde(default)]
        profile: Profile,
    }
    let saved: Saved = if root.join("cpuv.lock").is_file() {
        toml::from_str(&fs::read_to_string(root.join("cpuv.lock"))?)?
    } else {
        Saved::default()
    };
    let mut profile = Profile::environment(if cli.reset_profile {
        None
    } else {
        Some(&saved.profile)
    })?;
    profile.configuration = cli.configuration.clone().unwrap_or_else(|| {
        read_manifest(root)
            .map(|m| m.project.configuration)
            .unwrap_or_else(|_| release_configuration())
    });
    let context = prepare_context(cli, &tc, &profile, &standard).await?;
    let profiles = context.home.join("profiles");
    fs::create_dir_all(&profiles)?;
    write_atomic(&profiles.join("default"), &fs::read(&context.profile_path)?)?;
    let mut upstream = command(&context.tools.conan, &context.home, &context.path);
    upstream
        .envs(&context.tc.environment)
        .current_dir(profiles::project_root(root)?);
    if arguments.is_empty() {
        upstream.arg("--help");
    } else {
        upstream.args(arguments);
    }
    let action = arguments
        .first()
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let subcommand = arguments
        .get(1)
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let uses_profiles = ["install", "create", "build", "test", "export-pkg"]
        .contains(&action.as_ref())
        || (action == "graph" && ["info", "explain", "build-order"].contains(&subcommand.as_ref()))
        || (action == "profile" && subcommand == "show");
    let explicit_build_profile = arguments.iter().any(|value| {
        let value = value.to_string_lossy();
        value == "-pr:b"
            || value == "--profile:build"
            || value.starts_with("-pr:b=")
            || value.starts_with("--profile:build=")
    });
    if uses_profiles && !explicit_build_profile {
        upstream.arg("-pr:b").arg(&context.build_profile_path);
    }
    if cli.offline {
        let action = arguments
            .first()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        if ["install", "create", "build"].contains(&action.as_ref()) {
            upstream.arg("--no-remote");
        } else {
            ensure!(
                ["", "--help", "--version", "export", "profile", "cache"]
                    .contains(&action.as_ref())
                    || (action == "remote" && arguments.get(1).is_some_and(|s| s == "list")),
                "For other Conan commands use upstream offline flags explicitly without cpuv --offline"
            );
        }
    }
    eprintln!(
        "Conan home: {}\nDefault target profile: {}\nBuild-tool profile: {}",
        context.home.display(),
        context.profile_path.display(),
        context.build_profile_path.display()
    );
    Ok(upstream
        .status()
        .context("Starting standalone Conan")?
        .code()
        .unwrap_or(1))
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
        backend: String,
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
            .is_none_or(|lock| (1..=3).contains(&lock.version)
                || refresh && lock.version == 4 && lock.backend == "vcpkg"),
        "Unsupported cpuv.lock version"
    );
    let old: Option<CatalogLock> = if saved.as_ref().is_some_and(|lock| lock.version == 3) {
        Some(toml::from_str(old_text.as_deref().unwrap()).context("Invalid catalog lock")?)
    } else {
        None
    };
    let mut profile = Profile::environment(if cli.reset_profile {
        None
    } else {
        saved.as_ref().map(|value| &value.profile)
    })?;
    profile.configuration = manifest.project.configuration.clone();
    if !refresh {
        let previous = old
            .as_ref()
            .context("No compatible catalog lock; run cpuv lock to resolve explicitly")?;
        ensure!(
            previous.dependencies == manifest.dependencies && previous.profile == profile,
            "Lock disagrees with manifest or build flags; run cpuv lock to resolve explicitly"
        );
    }
    let CatalogContext {
        tc,
        identity,
        base,
        home,
        tools,
        path,
        profile_path,
        build_profile_path,
        key,
        _guard,
    } = prepare_context(cli, base_tc, &profile, &manifest.project.std).await?;
    let stage = tempfile::Builder::new()
        .prefix("stage-catalog-")
        .tempdir_in(root.join(".cpuv"))?;
    let env = stage.path().join("env");
    fs::create_dir_all(env.join("bin"))?;
    let graph_file = stage.path().join("graph.json");
    let upstream_lock = stage.path().join("conan.lock");
    let input_lock = stage.path().join("input.lock");
    if !refresh {
        let previous = old
            .as_ref()
            .context("No compatible catalog lock; run cpuv lock to resolve explicitly")?;
        ensure!(
            previous.dependencies == manifest.dependencies
                && previous.profile == profile
                && previous.abi == tc.abi
                && previous.gcc_version == tc.gcc_version,
            "Lock disagrees with manifest, profile, or compiler; run cpuv lock to resolve explicitly"
        );
    }
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
    install.envs(&tc.environment).env("PATH", &path);
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
        .arg(&build_profile_path)
        .arg(if cli.from_source {
            "--build=*"
        } else {
            "--build=missing"
        })
        .args([
            "-g",
            "PkgConfigDeps",
            "-g",
            "CMakeDeps",
            "-g",
            "CMakeToolchain",
            "-d",
            "full_deploy",
            "-of",
        ])
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
        if let Dependency::Detailed {
            linkage: Some(linkage),
            ..
        } = dependency
        {
            install.arg(format!(
                "--options={name}/*:shared={}",
                if linkage == "shared" { "True" } else { "False" }
            ));
        }
    }
    for (name, dependency) in &manifest.dependencies {
        if let Dependency::Detailed {
            options, linkage, ..
        } = dependency
        {
            ensure!(
                linkage.is_none() || !options.contains_key("shared"),
                "Use either linkage or options.shared for {name}, not both"
            );
            for (option, value) in options {
                ensure!(
                    !option.is_empty()
                        && option
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
                        && !value.contains(['\r', '\n']),
                    "Invalid Conan option for {name}"
                );
                install.arg(format!("--options={name}/*:{option}={value}"));
            }
        }
    }
    if manifest.dependencies.is_empty() {
        fs::write(stage.path().join("conanfile.txt"), "[requires]\n")?;
        install.arg(stage.path().join("conanfile.txt"));
    }
    eprintln!(
        "Resolving through Conan using compiler {} ({}). Build log: {}",
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
    identity.verify().context(
        "Compiler/SDK changed while packages were being built; previous environment is preserved",
    )?;
    let versions = prepare_metadata(
        &env,
        &root.join(".cpuv/env"),
        manifest,
        &tc,
        &profile,
        &graph,
        &identity,
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
    cpuv_core::seal(&env)?;
    let mut manifest = manifest.clone();
    manifest.backend = Backend::Conan;
    manifest.toolchain = PinnedToolchain {
        target: tc.abi.target_triple.clone(),
        compiler: tc.compiler_id(),
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
    stage_auxiliary(stage.path(), root, &tc)?;
    commit(root, stage)?;
    if let Err(error) = write_atomic(
        &root.join(".cpuv/last-sync.json"),
        &serde_json::to_vec_pretty(
            &serde_json::json!({"backend":"conan","packages":locked.packages.len(),"total_ms":started.elapsed().as_secs_f64()*1000.0}),
        )?,
    ) {
        eprintln!("warning: environment committed, but sync report could not be saved: {error:#}");
    }
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
        "Backend: Conan (configured remotes and local recipes)\nCompiler recorded by lock: GCC {}",
        lock.gcc_version
    );
    for (name, version) in lock.packages {
        println!("{name} {version}");
    }
    Ok(())
}

fn commit(root: &Path, stage: tempfile::TempDir) -> Result<()> {
    cpuv_core::commit(
        root,
        stage,
        &[
            (".cpuv/env", "env"),
            ("cpuv.toml", "cpuv.toml"),
            ("cpuv.lock", "cpuv.lock"),
            (".cpuv/toolchain.json", "toolchain.json"),
            (".cpuv/activate.ps1", "activate.ps1"),
            (".cpuv/bin", "bin"),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dependency(linkage: &str) -> Dependency {
        Dependency::Detailed {
            version: "*".into(),
            linkage: Some(linkage.into()),
            reference: None,
            options: BTreeMap::new(),
        }
    }
    #[test]
    fn forced_linkage_requires_provider_setting_and_matching_artifact() {
        let shared = dependency("shared");
        assert!(
            verify_requested_linkage(
                "test",
                Some(&shared),
                &serde_json::json!({"options":{}}),
                &["test.DLL".into()]
            )
            .is_err()
        );
        let node = serde_json::json!({"options":{"shared":"True"}});
        assert!(
            verify_requested_linkage("test", Some(&shared), &node, &["test.a".into()]).is_err()
        );
        verify_requested_linkage("test", Some(&shared), &node, &["test.DLL".into()]).unwrap();
        let static_dependency = dependency("static");
        let node = serde_json::json!({"options":{"shared":false}});
        assert!(
            verify_requested_linkage(
                "test",
                Some(&static_dependency),
                &node,
                &["test.dll.a".into()]
            )
            .is_err()
        );
        verify_requested_linkage("test", Some(&static_dependency), &node, &["test.a".into()])
            .unwrap();
    }
}
