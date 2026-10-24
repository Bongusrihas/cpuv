//! A provider-owned vcpkg graph, pinned to an immutable registry/tool revision.
use super::*;

const REVISION: &str = "0c2a9ace1964b818b15f7ae50ba1c42404918a7b";
fn revision() -> String {
    REVISION.into()
}
fn triplet() -> String {
    "x64-windows".into()
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default = "revision")]
    pub revision: String,
    #[serde(default = "triplet")]
    pub triplet: String,
    #[serde(default = "triplet")]
    pub host_triplet: String,
    #[serde(default)]
    pub features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub default_features: BTreeMap<String, bool>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            revision: revision(),
            triplet: triplet(),
            host_triplet: triplet(),
            features: BTreeMap::new(),
            default_features: BTreeMap::new(),
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Lock {
    version: u32,
    backend: String,
    dependencies: BTreeMap<String, Dependency>,
    settings: Settings,
    configuration: String,
    toolchain_identity: String,
    installed_status: String,
}

fn run(command: &mut Command) -> Result<()> {
    let status = command.status().context(
        "Starting vcpkg/Git; Git and Microsoft C++ Build Tools are prerequisites for this adapter",
    )?;
    ensure!(
        status.success(),
        "vcpkg provider operation failed with {status}; previous project environment is preserved"
    );
    Ok(())
}
fn git(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).env("GIT_TERMINAL_PROMPT", "0");
    command
}
fn prepare(cache: &Path, settings: &Settings, offline: bool) -> Result<PathBuf> {
    ensure!(
        settings.revision.len() == 40 && settings.revision.bytes().all(|c| c.is_ascii_hexdigit()),
        "vcpkg.revision must be an immutable 40-digit Git commit"
    );
    let directory = cache.join(&settings.revision);
    if !directory.is_dir() {
        ensure!(
            !offline,
            "Pinned vcpkg checkout is unavailable offline; run cpuv lock online first"
        );
        let temporary = tempfile::Builder::new()
            .prefix("checkout-")
            .tempdir_in(cache)?;
        run(Command::new("git").arg("init").arg(temporary.path()))?;
        run(git(temporary.path()).args([
            "remote",
            "add",
            "origin",
            "https://github.com/microsoft/vcpkg.git",
        ]))?;
        run(git(temporary.path()).args(["fetch", "--depth=1", "origin", &settings.revision]))?;
        run(git(temporary.path()).args(["checkout", "--detach", "FETCH_HEAD"]))?;
        fs::rename(temporary.path(), &directory)?;
    }
    let output = git(&directory).args(["rev-parse", "HEAD"]).output()?;
    ensure!(
        output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .trim()
                .eq_ignore_ascii_case(&settings.revision),
        "vcpkg checkout revision changed; restore the pinned cache checkout"
    );
    run(git(&directory).args(["diff", "--quiet", "HEAD", "--"]))?;
    let executable = directory.join("vcpkg.exe");
    let receipt = directory.join("cpuv-bootstrap.json");
    if !executable.is_file() {
        ensure!(
            !offline,
            "vcpkg executable has not been bootstrapped; run cpuv lock online first"
        );
        run(Command::new("cmd.exe").current_dir(&directory).args([
            "/d",
            "/c",
            "bootstrap-vcpkg.bat",
            "-disableMetrics",
        ]))?;
        let identity = cpuv_core::ToolIdentity::capture([executable.clone()])?;
        cpuv_core::atomic_write(&receipt, &serde_json::to_vec(&identity)?)?;
    }
    let identity: cpuv_core::ToolIdentity = serde_json::from_slice(&fs::read(receipt).context(
        "Unverified vcpkg bootstrap; use a new --cache-dir to bootstrap a verified checkout",
    )?)?;
    identity.verify()?;
    Ok(directory)
}

fn native_manifest(manifest: &Manifest, settings: &Settings) -> Result<serde_json::Value> {
    let mut dependencies = Vec::new();
    let mut overrides = Vec::new();
    for (name, dependency) in &manifest.dependencies {
        ensure!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
            "Invalid vcpkg port name {name}"
        );
        if let Dependency::Detailed {
            reference,
            linkage,
            options,
            ..
        } = dependency
        {
            ensure!(
                reference.is_none() && linkage.is_none() && options.is_empty(),
                "vcpkg uses native ports, features and triplet linkage; remove Conan reference/options/linkage for {name}"
            );
        }
        let mut port = serde_json::json!({"name":name});
        if let Some(features) = settings.features.get(name) {
            ensure!(
                features.iter().all(|f| !f.is_empty()
                    && f.bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')),
                "Invalid vcpkg feature for {name}"
            );
            port["features"] = serde_json::to_value(features)?;
        }
        if let Some(defaults) = settings.default_features.get(name) {
            port["default-features"] = (*defaults).into();
        }
        let version = dependency.version()?;
        if let Some(minimum) = version.strip_prefix(">=") {
            ensure!(!minimum.is_empty(), "Missing vcpkg minimum version");
            port["version>="] = minimum.into();
        } else if version != "*" && !version.is_empty() {
            ensure!(
                !version.starts_with(['<', '>', '^', '~']),
                "vcpkg supports an exact version override or >= minimum, not {version}"
            );
            overrides
                .push(serde_json::json!({"name":name,"version":version.trim_start_matches('=')}));
        }
        dependencies.push(port);
    }
    for name in settings
        .features
        .keys()
        .chain(settings.default_features.keys())
    {
        ensure!(
            manifest.dependencies.contains_key(name),
            "vcpkg feature settings reference undeclared port {name}"
        );
    }
    Ok(
        serde_json::json!({"name":"cpuv-environment","version-string":"1","builtin-baseline":settings.revision,"dependencies":dependencies,"overrides":overrides}),
    )
}

pub fn validate(tc: &Toolchain, settings: &Settings) -> Result<()> {
    ensure!(
        tc.driver == "msvc" && tc.abi.compiler_family == "msvc" && tc.abi.arch == "x86_64",
        "The vcpkg adapter currently validates x64 MSVC targets; use Conan for the selected compiler/target"
    );
    ensure!(
        ["x64-windows", "x64-windows-static-md"].contains(&settings.triplet.as_str())
            && settings.host_triplet == "x64-windows",
        "Validated vcpkg triplets: x64-windows and x64-windows-static-md; host triplet: x64-windows"
    );
    Ok(())
}

pub async fn sync(
    root: &Path,
    cli: &Cli,
    manifest: &Manifest,
    tc: &Toolchain,
    refresh: bool,
) -> Result<()> {
    ensure!(
        !source::custom_profile_requested(),
        "Custom CPPFLAGS/CFLAGS/CXXFLAGS/LDFLAGS require the Conan profile adapter; vcpkg uses its pinned triplet flags"
    );
    for name in ["VCPKG_OVERLAY_PORTS", "VCPKG_OVERLAY_TRIPLETS"] {
        ensure!(
            std::env::var_os(name).is_none_or(|v| v.is_empty()),
            "Untracked {name} is unsupported; use the pinned registry and manifest features"
        );
    }
    let settings = manifest.vcpkg.clone().unwrap_or_default();
    validate(tc, &settings)?;
    let native_manifest = native_manifest(manifest, &settings)?;
    let identity = cpuv_toolchain::identity(tc)?;
    let identity_hash = blake3::hash(&serde_json::to_vec(&identity)?)
        .to_hex()
        .to_string();
    let old_text = match fs::read_to_string(root.join("cpuv.lock")) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).context("Reading dependency lock"),
    };
    let header: Option<toml::Value> = old_text
        .as_deref()
        .map(toml::from_str)
        .transpose()
        .context("Invalid dependency lock")?;
    if let Some(header) = &header {
        ensure!(
            header
                .get("version")
                .and_then(toml::Value::as_integer)
                .is_some_and(|v| (1..=4).contains(&v)),
            "Unsupported dependency lock version"
        );
    }
    let old: Option<Lock> = if header
        .as_ref()
        .is_some_and(|h| h.get("backend").and_then(toml::Value::as_str) == Some("vcpkg"))
    {
        Some(toml::from_str(old_text.as_deref().unwrap()).context("Invalid vcpkg lock")?)
    } else {
        None
    };
    if !refresh {
        let old = old
            .as_ref()
            .context("No compatible vcpkg lock; run cpuv lock first")?;
        ensure!(
            old.version == 4
                && old.backend == "vcpkg"
                && old.dependencies == manifest.dependencies
                && old.settings == settings
                && old.configuration == manifest.project.configuration
                && old.toolchain_identity == identity_hash,
            "vcpkg lock disagrees with the request or compiler/SDK contents; run cpuv lock explicitly"
        );
    }
    if cli.offline {
        ensure!(
            old.is_some(),
            "Offline vcpkg requires a previous lock; resolve online first"
        );
        let env = root.join(".cpuv/env");
        cpuv_core::verify(&env).context("Offline vcpkg currently requires an intact installed environment; repair online with cpuv sync")?;
        let metadata: cpuv_relocate::Environment =
            serde_json::from_slice(&fs::read(env.join("manifest.json"))?)?;
        ensure!(
            metadata.request_hash == request_hash(manifest)?,
            "Offline vcpkg environment disagrees with the request; resolve it online first"
        );
        metadata.tool_identity.verify()?;
        eprintln!("Verified pinned vcpkg environment offline.");
        return Ok(());
    }
    let cache = cache_dir(cli)?.join("vcpkg");
    fs::create_dir_all(&cache)?;
    let guard = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache.join("operation.lock"))?;
    guard
        .try_lock_exclusive()
        .context("Another vcpkg operation is active; retry when it finishes")?;
    let checkout = prepare(&cache, &settings, cli.offline)?;
    let stage = tempfile::Builder::new()
        .prefix("stage-vcpkg-")
        .tempdir_in(root.join(".cpuv"))?;
    let env = stage.path().join("env");
    let installed = env.join("vcpkg_installed");
    fs::create_dir_all(&env)?;
    for directory in ["include", "lib", "bin"] {
        fs::create_dir_all(env.join(directory))?;
    }
    // Initialize the generic shell before provider installation. Package-owned
    // vcpkg metadata must never pass through MSYS2 prefix rewriting.
    let mut metadata = cpuv_relocate::index_environment(
        &env,
        &root.join(".cpuv/env"),
        tc.compiler_path.clone(),
        tc.abi_hash.clone(),
        manifest.project.std.clone(),
    )?;
    fs::write(
        stage.path().join("vcpkg.json"),
        serde_json::to_vec_pretty(&native_manifest)?,
    )?;
    let overlay = stage.path().join("triplets");
    fs::create_dir_all(&overlay)?;
    let include = cpuv_relocate::portable(
        &checkout
            .join("triplets")
            .join(format!("{}.cmake", settings.triplet)),
    );
    let selected_version = tc
        .environment
        .get("VCTOOLSINSTALLDIR")
        .map(Path::new)
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .context("Selected MSVC toolset version is missing")?;
    ensure!(
        selected_version
            .bytes()
            .all(|c| c.is_ascii_digit() || c == b'.'),
        "Invalid MSVC toolset version"
    );
    let overlay_file = overlay.join(format!("{}.cmake", settings.triplet));
    let platform_toolset = match tc.abi.compiler_abi_major.as_str() {
        "195" => "v145",
        "194" | "193" => "v143",
        "192" => "v142",
        "191" => "v141",
        _ => bail!("vcpkg does not declare a supported platform toolset for this compiler"),
    };
    fs::write(
        &overlay_file,
        format!(
            "include(\"{include}\")\nset(VCPKG_PLATFORM_TOOLSET \"{platform_toolset}\")\nset(VCPKG_PLATFORM_TOOLSET_VERSION \"{selected_version}\")\n"
        ),
    )?;
    let mut install = Command::new(checkout.join("vcpkg.exe"));
    install
        .envs(&tc.environment)
        .env("VCPKG_DISABLE_METRICS", "1")
        .env("VCPKG_FORCE_DOWNLOADED_BINARIES", "1")
        .env_remove("VCPKG_FORCE_SYSTEM_BINARIES")
        .env(
            "VCPKG_MAX_CONCURRENCY",
            cli.build_jobs.map_or_else(|| "4".into(), |j| j.to_string()),
        )
        .current_dir(stage.path())
        .arg("install")
        .arg(format!("--overlay-triplets={}", overlay.display()))
        .arg(format!("--triplet={}", settings.triplet))
        .arg(format!("--host-triplet={}", settings.host_triplet))
        .arg(format!("--x-manifest-root={}", stage.path().display()))
        .arg(format!("--x-install-root={}", installed.display()))
        .arg(format!(
            "--downloads-root={}",
            cache.join("downloads").display()
        ))
        .arg(format!(
            "--x-buildtrees-root={}",
            cache.join("buildtrees").display()
        ))
        .arg(format!(
            "--x-packages-root={}",
            cache.join("packages").display()
        ));
    // The selected toolset version participates in vcpkg's own compiler detection.
    if let Some(vs) = tc.environment.get("VSINSTALLDIR") {
        install.env("VCPKG_VISUAL_STUDIO_PATH", vs.trim_end_matches(['\\', '/']));
    }
    if cli.offline {
        install.arg("--only-binarycaching");
    }
    run(&mut install)?;
    let raw_status = fs::read_to_string(installed.join("vcpkg/status"))?.replace("\r\n", "\n");
    let mut records: Vec<_> = raw_status
        .split("\n\n")
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .collect();
    records.sort();
    let status = records.join("\n\n");
    if !refresh {
        ensure!(
            old.as_ref().unwrap().installed_status == status,
            "vcpkg restored a different package graph; previous environment is preserved"
        );
    }
    let prefix = installed.join(&settings.triplet);
    let runtime = prefix.join(if manifest.project.configuration == "Debug" {
        "debug/bin"
    } else {
        "bin"
    });
    fs::create_dir_all(env.join("bin"))?;
    if runtime.is_dir() {
        for entry in fs::read_dir(&runtime)? {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
            {
                fs::copy(entry.path(), env.join("bin").join(entry.file_name()))?;
            }
        }
    }
    fs::write(env.join("request.toml"), toml::to_string_pretty(manifest)?)?;
    metadata.request_hash = request_hash(manifest)?;
    metadata.configuration = manifest.project.configuration.clone();
    metadata.driver = tc.driver.clone();
    metadata.process_environment = tc.environment.clone();
    let mut identity = identity;
    let mut provider_files = vec![checkout.join("vcpkg.exe")];
    let provider_roots = vec![checkout.join("scripts"), checkout.join("triplets")];
    for directory in &provider_roots {
        for entry in walkdir::WalkDir::new(directory) {
            let entry = entry?;
            if entry.file_type().is_file() {
                provider_files.push(entry.path().to_owned());
            }
        }
    }
    let provider_identity =
        cpuv_core::ToolIdentity::capture_with_roots(provider_files, provider_roots)?;
    identity.files.extend(provider_identity.files);
    identity.roots.extend(provider_identity.roots);
    metadata.tool_identity = identity;
    let inspection =
        cpuv_relocate::inspect::environment_for_runtime(&env, &tc.abi.arch, &tc.abi.crt_runtime)?;
    fs::write(
        env.join("compatibility.json"),
        serde_json::to_vec_pretty(&inspection)?,
    )?;
    metadata.managed_binaries = !manifest.dependencies.is_empty();
    metadata.dependency_origin = "vcpkg".into();
    metadata.native_build_only = !manifest.dependencies.is_empty();
    metadata.link_model = 3;
    fs::write(
        env.join("manifest.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    let cmake_path = cpuv_relocate::portable(&checkout.join("scripts/buildsystems/vcpkg.cmake"));
    ensure!(
        !cmake_path.contains(['"', ';', '\n', '\r']),
        "Unsupported vcpkg cache path"
    );
    fs::write(
        env.join("vcpkg-toolchain.cmake"),
        format!(
            "set(VCPKG_TARGET_TRIPLET \"{}\" CACHE STRING \"\" FORCE)\nset(VCPKG_HOST_TRIPLET \"{}\" CACHE STRING \"\" FORCE)\nset(VCPKG_MANIFEST_MODE OFF CACHE BOOL \"\" FORCE)\nset(VCPKG_INSTALLED_DIR \"${{CMAKE_CURRENT_LIST_DIR}}/vcpkg_installed\" CACHE PATH \"\" FORCE)\ninclude(\"{cmake_path}\")\n",
            settings.triplet, settings.host_triplet
        ),
    )?;
    // Relocate package-owned generated metadata before the environment is published.
    let final_env = root.join(".cpuv/env");
    for entry in walkdir::WalkDir::new(&env) {
        let entry = entry?;
        if entry.file_type().is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|e| e == "cmake" || e == "pc")
        {
            let text = fs::read_to_string(entry.path())?;
            fs::write(
                entry.path(),
                text.replace(
                    &cpuv_relocate::portable(&env),
                    &cpuv_relocate::portable(&final_env),
                )
                .replace(
                    env.to_string_lossy().as_ref(),
                    final_env.to_string_lossy().as_ref(),
                ),
            )?;
        }
    }
    fs::write(
        env.join("provider.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"provider":"vcpkg","settings":settings,"installed_status":status,"direct_compiler_interface":"Use native CMake imported targets with cpuv build/test"}),
        )?,
    )?;
    metadata.tool_identity.verify().context("Compiler/SDK or provider scripts changed during the build; previous environment is preserved")?;
    cpuv_core::seal(&env)?;
    let locked = Lock {
        version: 4,
        backend: "vcpkg".into(),
        dependencies: manifest.dependencies.clone(),
        settings,
        configuration: manifest.project.configuration.clone(),
        toolchain_identity: identity_hash,
        installed_status: status,
    };
    fs::write(
        stage.path().join("cpuv.toml"),
        toml::to_string_pretty(manifest)?,
    )?;
    fs::write(
        stage.path().join("cpuv.lock"),
        toml::to_string_pretty(&locked)?,
    )?;
    stage_auxiliary(stage.path(), root, tc)?;
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
    )?;
    eprintln!("Ready from pinned vcpkg registry; use cpuv build/test with its CMake targets.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_versions_and_features_are_preserved() {
        let manifest: Manifest = toml::from_str(
            r#"backend="vcpkg"
[project]
name="test"
version="1"
[toolchain]
target="test"
compiler="test"
abi="test"
[dependencies]
zlib="1.3.1"
"#,
        )
        .unwrap();
        let mut settings = Settings::default();
        settings.features.insert("zlib".into(), vec!["test".into()]);
        let value = native_manifest(&manifest, &settings).unwrap();
        assert_eq!(value["dependencies"][0]["features"][0], "test");
        assert_eq!(value["overrides"][0]["version"], "1.3.1");
        assert_eq!(value["builtin-baseline"], REVISION);
    }
}
