//! Recipe-backed builds using the user's compiler, flags, ABI and driver linker.
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
fn cxx_level(tc: &Toolchain) -> u16 {
    let value = tc
        .observed_abi_options
        .get("__cplusplus")
        .and_then(|value| {
            value
                .trim_end_matches(|c: char| c.is_ascii_alphabetic())
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(0);
    if value >= 202400 {
        26
    } else if value >= 202302 {
        23
    } else if value >= 202002 {
        20
    } else if value >= 201703 {
        17
    } else if value >= 201402 {
        14
    } else if value >= 201103 {
        11
    } else {
        98
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Recipe {
    version: String,
    #[serde(flatten)]
    source: Archive,
    #[serde(default)]
    cmake: Vec<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    requires_c: bool,
    #[serde(default)]
    minimum_cxx: u16,
    license_file: String,
    #[serde(default)]
    patches: Vec<Patch>,
    #[serde(default)]
    interface: Option<Interface>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Patch {
    file: String,
    find: String,
    replace: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Interface {
    module: String,
    #[serde(default)]
    libraries: Vec<String>,
    #[serde(default)]
    libraries_by_runtime: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    cflags: Vec<String>,
    #[serde(default)]
    requires: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Tool {
    version: String,
    #[serde(flatten)]
    source: Archive,
    executable: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    schema: u32,
    #[serde(default)]
    packages: BTreeMap<String, Recipe>,
    #[serde(default)]
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

#[derive(Serialize, Deserialize)]
struct SourceLock {
    version: u32,
    backend: String,
    dependencies: BTreeMap<String, Dependency>,
    packages: BTreeMap<String, Recipe>,
    tools: BTreeMap<String, Tool>,
    profile: Profile,
    compiler: Toolchain,
    compiler_sha256: String,
    cache_key: String,
}
/// Portable project lock: local executable/cache paths belong under .cpuv only.
#[derive(Serialize, Deserialize)]
struct ProjectLock {
    version: u32,
    backend: String,
    dependencies: BTreeMap<String, Dependency>,
    packages: BTreeMap<String, Recipe>,
    tools: BTreeMap<String, Tool>,
    profile: Profile,
    #[serde(default)]
    gcc_version: String,
    #[serde(default)]
    abi: Option<cpuv_toolchain::AbiTuple>,
    #[serde(default)]
    compiler_sha256: String,
}
impl From<&SourceLock> for ProjectLock {
    fn from(value: &SourceLock) -> Self {
        Self {
            version: value.version,
            backend: value.backend.clone(),
            dependencies: value.dependencies.clone(),
            packages: value.packages.clone(),
            tools: value.tools.clone(),
            profile: value.profile.clone(),
            gcc_version: value.compiler.gcc_version.clone(),
            abi: Some(value.compiler.abi.clone()),
            compiler_sha256: value.compiler_sha256.clone(),
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Installed {
    original_prefix: String,
    files: BTreeMap<String, String>,
    ownership: BTreeMap<String, Vec<PathBuf>>,
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

// CMake expands flag strings into a Windows command line; preserve spaces and quotes.
fn joined(flags: impl Iterator<Item = String>) -> String {
    flags
        .map(|flag| {
            let mut out = String::from("\"");
            let mut slashes = 0;
            for character in flag.chars() {
                if character == '\\' {
                    slashes += 1;
                } else {
                    out.push_str(&"\\".repeat(if character == '"' {
                        slashes * 2 + 1
                    } else {
                        slashes
                    }));
                    slashes = 0;
                    out.push(character);
                }
            }
            out.push_str(&"\\".repeat(slashes * 2));
            out.push('"');
            out
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_registry(text: &str) -> Result<Registry> {
    // Serde flatten does not reject misspelled archive/recipe keys itself.
    let document: toml::Value = toml::from_str(text)?;
    let archive_keys = [
        "url",
        "sha256",
        "archive",
        "strip_components",
        "include",
        "exclude",
    ];
    for (section, specific) in [
        (
            "packages",
            &[
                "version",
                "cmake",
                "dependencies",
                "requires_c",
                "minimum_cxx",
                "license_file",
                "patches",
                "interface",
            ][..],
        ),
        ("tools", &["version", "executable"][..]),
    ] {
        if let Some(entries) = document.get(section).and_then(toml::Value::as_table) {
            for (name, entry) in entries {
                let fields = entry.as_table().context("Recipe must be a TOML table")?;
                for key in fields.keys() {
                    ensure!(
                        archive_keys.contains(&key.as_str()) || specific.contains(&key.as_str()),
                        "Unknown source registry key {section}.{name}.{key}"
                    );
                }
            }
        }
    }
    Ok(toml::from_str(text)?)
}

fn registry(cli: &Cli) -> Result<Registry> {
    let mut value = parse_registry(include_str!("../../../recipes.toml"))?;
    if let Some(path) = &cli.recipes {
        let text = fs::read_to_string(path).context("Reading source recipe registry")?;
        let custom = parse_registry(&text).context("Invalid source recipe registry")?;
        ensure!(
            custom.schema == 1,
            "Unsupported custom source registry schema"
        );
        value.packages.extend(custom.packages);
        value.tools.extend(custom.tools);
    }
    ensure!(
        value.schema == 1,
        "Unsupported source recipe registry schema"
    );
    Ok(value)
}

fn resolve(registry: &Registry, requirements: &[String]) -> Result<BTreeMap<String, Recipe>> {
    let mut pending = requirements.to_vec();
    let mut packages = BTreeMap::new();
    while let Some(requirement) = pending.pop() {
        let (name, operator, version) = cpuv_solver::split_requirement(&requirement);
        ensure!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.+".contains(&c)),
            "Invalid source package name {name}"
        );
        let recipe = registry.packages.get(name).with_context(|| {
            let similar = cpuv_solver::suggestions(name, registry.packages.keys().map(String::as_str));
            format!("No source build recipe for {name}. {} Add a checked recipe with --recipes FILE; incompatible binaries will not be installed as a fallback",
                if similar.is_empty() { "No similar recipe names were found.".into() }
                else { format!("Similar recipes: {}.", similar.join(", ")) }
            )
        })?;
        let order = cpuv_solver::version_cmp(&recipe.version, version);
        use std::cmp::Ordering::*;
        ensure!(
            match operator {
                "" => true,
                "=" | "==" => order == Equal,
                ">=" => order != Less,
                "<=" => order != Greater,
                ">" => order == Greater,
                "<" => order == Less,
                _ => false,
            },
            "Source recipe {name}@{} does not satisfy {operator}{version}",
            recipe.version
        );
        if packages.insert(name.into(), recipe.clone()).is_none() {
            pending.extend(recipe.dependencies.clone());
        }
        ensure!(packages.len() <= 4096, "Too many source dependencies");
    }
    Ok(packages)
}

fn order(packages: &BTreeMap<String, Recipe>) -> Result<Vec<String>> {
    let mut ordered = Vec::new();
    let mut done = BTreeSet::new();
    while ordered.len() < packages.len() {
        let mut changed = false;
        for (name, recipe) in packages {
            if done.contains(name) {
                continue;
            }
            if recipe
                .dependencies
                .iter()
                .all(|r| done.contains(cpuv_solver::split_requirement(r).0))
            {
                done.insert(name.clone());
                ordered.push(name.clone());
                changed = true;
            }
        }
        ensure!(
            changed,
            "Source recipe dependency cycle or missing locked dependency"
        );
    }
    Ok(ordered)
}

fn run(command: &mut Command, log: &Path) -> Result<()> {
    use std::io::Write;
    let output = command.output().context("Running source build tool")?;
    let mut file = OpenOptions::new().append(true).create(true).open(log)?;
    writeln!(file, "command: {command:?}")?;
    file.write_all(&output.stdout)?;
    file.write_all(&output.stderr)?;
    ensure!(
        output.status.success(),
        "Source build failed ({}). Log: {}\n{}\n{}",
        output.status,
        log.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn compiler_path(tc: &Toolchain) -> Result<OsString> {
    let mut paths = vec![tc.compiler_path.parent().unwrap().to_owned()];
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    Ok(std::env::join_paths(paths)?)
}

fn c_compiler(tc: &Toolchain) -> Result<PathBuf> {
    let file = tc
        .compiler_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .replace("g++", "gcc");
    let path = tc.compiler_path.with_file_name(file);
    ensure!(
        path.is_file(),
        "Recipe needs the matching C compiler next to {}",
        tc.compiler_path.display()
    );
    let target = Command::new(&path)
        .arg("-dumpmachine")
        .env("PATH", compiler_path(tc)?)
        .output()?;
    ensure!(
        target.status.success()
            && String::from_utf8_lossy(&target.stdout).trim() == tc.abi.target_triple,
        "C and C++ compilers target different architectures"
    );
    Ok(path)
}

async fn built_environment(
    cache: &Path,
    locked: &SourceLock,
    registry: &Registry,
    cli: &Cli,
    standard: &str,
) -> Result<(PathBuf, Installed, bool)> {
    let base = cache.join("source");
    let builds = base.join("builds");
    fs::create_dir_all(&builds)?;
    fs::create_dir_all(base.join("locks"))?;
    let guard = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(base.join("locks").join(&locked.cache_key))?;
    guard
        .try_lock_exclusive()
        .context("Another process is building this source profile; retry after it completes")?;
    let final_root = builds.join(&locked.cache_key);
    if final_root.join("installed.json").is_file() {
        let installed: Installed =
            serde_json::from_slice(&fs::read(final_root.join("installed.json"))?)?;
        archive::verify(&final_root.join("prefix"), &installed.files)?;
        return Ok((final_root.join("prefix"), installed, true));
    }
    ensure!(
        !final_root.exists(),
        "Incomplete source cache entry: use a fresh --cache-dir"
    );
    let work = tempfile::Builder::new()
        .prefix("build-")
        .tempdir_in(&builds)?;
    let prefix = work.path().join("prefix");
    for dir in ["include", "lib", "bin"] {
        fs::create_dir_all(prefix.join(dir))?;
    }
    let tc = &locked.compiler;
    let limits = cli.transfer_limits();
    let build_path = std::env::join_paths(
        std::iter::once(prefix.join("bin")).chain(std::env::split_paths(&compiler_path(tc)?)),
    )?;
    let mut ownership = BTreeMap::new();
    let mut files = BTreeMap::new();
    if !locked.packages.is_empty() {
        let prepared = tools::prepare(registry, &base, cli.offline, &limits).await?;
        let archives: BTreeMap<String, PathBuf> =
            stream::iter(locked.packages.iter().map(|(name, recipe)| {
                let archive_cache = base.join("archives");
                let limits = &limits;
                async move {
                    Ok::<_, anyhow::Error>((
                        name.clone(),
                        archive::download(&archive_cache, &recipe.source, cli.offline, limits)
                            .await?,
                    ))
                }
            }))
            .buffer_unordered(cli.concurrent_downloads as usize)
            .try_collect()
            .await?;
        let source_root = work.path().join("sources");
        let output_root = work.path().join("build");
        let persistent_logs = base.join("logs").join(&locked.cache_key);
        fs::create_dir_all(&persistent_logs)?;
        let jobs = cli
            .build_jobs
            .map(usize::from)
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from));
        for name in order(&locked.packages)? {
            let recipe = &locked.packages[&name];
            let source = source_root.join(&name);
            let output = output_root.join(&name);
            archive::extract(&archives[&name], &recipe.source, &source, &limits)?;
            for patch in &recipe.patches {
                let path = source.join(cpuv_store::safe_relative(Path::new(&patch.file))?);
                ensure!(
                    fs::metadata(&path)?.len() <= 10 * 1024 * 1024,
                    "Recipe patch target is too large"
                );
                let text = fs::read_to_string(&path)?;
                ensure!(
                    !patch.find.is_empty() && text.matches(&patch.find).count() == 1,
                    "Recipe patch for {name}/{} does not match its pinned source exactly once",
                    patch.file
                );
                fs::write(path, text.replacen(&patch.find, &patch.replace, 1))?;
            }
            ensure!(
                source.join("CMakeLists.txt").is_file(),
                "Source recipe {name} has no CMakeLists.txt"
            );
            ensure!(
                recipe
                    .cmake
                    .iter()
                    .all(|argument| argument.starts_with("-D")
                        && ![
                            "CMAKE_CXX_COMPILER",
                            "CMAKE_C_COMPILER",
                            "CMAKE_TOOLCHAIN_FILE",
                            "CMAKE_INSTALL_PREFIX",
                            "CMAKE_MAKE_PROGRAM",
                            "CMAKE_CXX_FLAGS",
                            "CMAKE_C_FLAGS",
                            "CMAKE_EXE_LINKER_FLAGS",
                            "CMAKE_SHARED_LINKER_FLAGS",
                            "CMAKE_SYSTEM_NAME",
                            "CMAKE_SYSTEM_PROCESSOR",
                            "CMAKE_GENERATOR",
                            "CMAKE_INSTALL_LIBDIR",
                            "CMAKE_PREFIX_PATH"
                        ]
                        .iter()
                        .any(|key| argument[2..].starts_with(key))),
                "Recipe {name} overrides the compiler/profile; move those choices into the source build environment"
            );
            if recipe.minimum_cxx >= 11 {
                let actual = cxx_level(tc);
                ensure!(
                    (11..=26).contains(&actual) && actual >= recipe.minimum_cxx,
                    "Recipe {name} requires C++{}; choose a compatible project standard/compiler or an older recipe",
                    recipe.minimum_cxx
                );
            }
            let log = persistent_logs.join(format!("{name}.log"));
            eprintln!(
                "Building {name} {} with GCC {} ({})",
                recipe.version,
                tc.gcc_version,
                tc.runtime_tag()
            );
            let mut configure = Command::new(&prepared.cmake);
            configure
                .args(["-S"])
                .arg(&source)
                .arg("-B")
                .arg(&output)
                .args(["-G", "Ninja"])
                .arg(format!(
                    "-DCMAKE_MAKE_PROGRAM={}",
                    cpuv_relocate::portable(&prepared.ninja)
                ))
                .arg(format!(
                    "-DCMAKE_CXX_COMPILER={}",
                    cpuv_relocate::portable(&tc.compiler_path)
                ))
                .arg(format!(
                    "-DCMAKE_INSTALL_PREFIX={}",
                    cpuv_relocate::portable(&prefix)
                ))
                .arg(format!(
                    "-DCMAKE_PREFIX_PATH={}",
                    cpuv_relocate::portable(&prefix)
                ))
                .arg(format!(
                    "-DCMAKE_CXX_FLAGS={}",
                    joined(
                        locked
                            .profile
                            .compiler_flags(standard)
                            .into_iter()
                            .map(|s| s.to_string_lossy().into_owned())
                    )
                ))
                .arg(format!(
                    "-DCMAKE_C_FLAGS={}",
                    joined(locked.profile.cpp.iter().chain(&locked.profile.c).cloned())
                ))
                .arg(format!(
                    "-DCMAKE_EXE_LINKER_FLAGS={}",
                    joined(locked.profile.link.iter().cloned())
                ))
                .arg(format!(
                    "-DCMAKE_SHARED_LINKER_FLAGS={}",
                    joined(locked.profile.link.iter().cloned())
                ))
                .args([
                    "-DCMAKE_BUILD_TYPE=Release",
                    "-DCMAKE_CXX_FLAGS_RELEASE=",
                    "-DCMAKE_C_FLAGS_RELEASE=",
                    "-DCMAKE_EXE_LINKER_FLAGS_RELEASE=",
                    "-DCMAKE_SHARED_LINKER_FLAGS_RELEASE=",
                    "-DCMAKE_INTERPROCEDURAL_OPTIMIZATION=OFF",
                    "-DCMAKE_INSTALL_LIBDIR=lib",
                    "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
                    "-DBUILD_TESTING=OFF",
                    "-DFETCHCONTENT_FULLY_DISCONNECTED=ON",
                    "-DCMAKE_FIND_USE_PACKAGE_REGISTRY=OFF",
                    "-DCMAKE_FIND_USE_SYSTEM_PACKAGE_REGISTRY=OFF",
                ])
                .args(&recipe.cmake)
                .env("PATH", &build_path)
                .env_remove("CMAKE_PREFIX_PATH")
                .env_remove("CC")
                .env_remove("CXX");
            if recipe.requires_c {
                configure.arg(format!(
                    "-DCMAKE_C_COMPILER={}",
                    cpuv_relocate::portable(&c_compiler(tc)?)
                ));
            }
            if tc.abi.crt_runtime == "cygwin" {
                configure.arg("-DCMAKE_SYSTEM_NAME=CYGWIN");
            }
            run(&mut configure, &log)?;
            run(
                Command::new(&prepared.cmake)
                    .arg("--build")
                    .arg(&output)
                    .arg("--parallel")
                    .arg(jobs.to_string())
                    .env("PATH", &build_path),
                &log,
            )?;
            run(
                Command::new(&prepared.cmake)
                    .arg("--install")
                    .arg(&output)
                    .env("PATH", &build_path),
                &log,
            )?;
            if let Some(interface) = &recipe.interface {
                ensure!(
                    !interface.module.is_empty()
                        && interface
                            .module
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"-_.+".contains(&c)),
                    "Invalid recipe interface module name"
                );
                let module_file = format!("{}.pc", interface.module);
                let mut previous_modules = Vec::new();
                for entry in walkdir::WalkDir::new(&prefix) {
                    let entry = entry?;
                    if entry.file_type().is_file()
                        && entry.file_name().to_string_lossy() == module_file
                    {
                        let relative = cpuv_relocate::portable(entry.path().strip_prefix(&prefix)?);
                        ensure!(
                            !files.contains_key(&relative),
                            "Recipe interface {} would replace another package's module",
                            interface.module
                        );
                        previous_modules.push(entry.path().to_owned());
                    }
                }
                for path in previous_modules {
                    fs::remove_file(path)?;
                }
                let libraries = interface
                    .libraries_by_runtime
                    .get(&tc.abi.crt_runtime)
                    .unwrap_or(&interface.libraries);
                ensure!(
                    libraries.iter().all(|s| !s.is_empty()
                        && s.bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"-_.+".contains(&c))),
                    "Invalid recipe interface library name"
                );
                ensure!(
                    interface.cflags.iter().all(|s| !s.contains(['\n', '\r'])),
                    "Recipe interface flags contain a newline"
                );
                let directory = prefix.join("lib/pkgconfig");
                fs::create_dir_all(&directory)?;
                let text = format!(
                    "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\nName: {}\nDescription: cpuv recipe public interface\nVersion: {}\nRequires: {}\nLibs: -L${{libdir}} {}\nCflags: -I${{includedir}} {}\n",
                    cpuv_relocate::portable(&prefix),
                    interface.module,
                    recipe.version,
                    interface.requires.join(", "),
                    libraries
                        .iter()
                        .map(|name| format!("-l{name}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                    interface.cflags.join(" ")
                );
                fs::write(directory.join(format!("{}.pc", interface.module)), text)?;
            }
            let license = cpuv_store::safe_relative(Path::new(&recipe.license_file))?;
            let destination = prefix.join("share/licenses").join(&name).join("LICENSE");
            fs::create_dir_all(destination.parent().unwrap())?;
            fs::copy(source.join(license), &destination)
                .context("Preserving upstream source license")?;
            let current = archive::inventory(&prefix)?;
            ensure!(
                files
                    .iter()
                    .all(|(path, hash)| current.get(path) == Some(hash)),
                "Recipe {name} overwrote another package's installed files"
            );
            let owned: Vec<PathBuf> = current
                .keys()
                .filter(|path| !files.contains_key(*path))
                .map(PathBuf::from)
                .collect();
            ownership.insert(name, owned);
            files = current;
        }
    }
    let installed = Installed {
        original_prefix: cpuv_relocate::portable(&prefix),
        files,
        ownership,
    };
    fs::write(
        work.path().join("installed.json"),
        serde_json::to_vec_pretty(&installed)?,
    )?;
    fs::rename(work.path(), &final_root)?;
    Ok((final_root.join("prefix"), installed, false))
}

fn stage_environment(
    cached: &Path,
    installed: &Installed,
    stage: &Path,
    final_env: &Path,
    manifest: &Manifest,
    locked: &SourceLock,
) -> Result<()> {
    for dir in ["include", "lib", "bin"] {
        fs::create_dir_all(stage.join(dir))?;
    }
    for relative in installed.files.keys() {
        let path = cpuv_store::safe_relative(Path::new(relative))?;
        cpuv_store::link_or_copy(&cached.join(&path), &stage.join(&path))?;
    }
    // Installed metadata is private to a project; never edit cached hardlinks.
    for relative in installed.files.keys() {
        let path = stage.join(relative);
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ["pc", "cmake"].contains(&extension) {
            let text = fs::read_to_string(&path)?;
            let replacement = if extension == "pc" {
                "${prefix}".into()
            } else {
                cpuv_relocate::portable(final_env)
            };
            let updated = text
                .replace(&installed.original_prefix, &replacement)
                .replace(&installed.original_prefix.replace('/', "\\"), &replacement);
            fs::remove_file(&path)?;
            fs::write(&path, updated)?;
        }
    }
    let tc = &locked.compiler;
    let mut metadata = cpuv_relocate::index_environment(
        stage,
        final_env,
        tc.compiler_path.clone(),
        tc.abi_hash.clone(),
        manifest.project.std.clone(),
    )?;
    let packages: Vec<_> = installed
        .ownership
        .iter()
        .map(|(name, files)| cpuv_relocate::linking::PackageFiles {
            name: cpuv_solver::normalize(name),
            files: files.clone(),
        })
        .collect();
    cpuv_relocate::linking::apply(
        stage,
        &mut metadata,
        &packages,
        &requirements(manifest)?,
        &manifest.link,
    )?;
    metadata.dependency_origin = "source".into();
    metadata.source_arch = tc.abi.arch.clone();
    metadata.source_flags = locked
        .profile
        .cpp
        .iter()
        .chain(&locked.profile.cxx)
        .cloned()
        .collect();
    metadata.cflags.extend(
        locked
            .profile
            .cpp
            .iter()
            .chain(&locked.profile.cxx)
            .cloned(),
    );
    metadata.link_flags.extend(locked.profile.link.clone());
    cpuv_relocate::validate_profile(&metadata, &[])?;
    for define in &metadata.defines {
        if let Some(value) = define.strip_prefix("_GLIBCXX_USE_CXX11_ABI=") {
            ensure!(
                value == tc.abi.glibcxx_cxx11_abi,
                "Source public interface requests dual ABI {value}, but dependencies were built with {}; change the source profile and rebuild",
                tc.abi.glibcxx_cxx11_abi
            );
        }
        if define.starts_with("_GLIBCXX_DEBUG") {
            ensure!(
                tc.observed_abi_options
                    .get("_GLIBCXX_DEBUG")
                    .is_some_and(|value| value != "not defined"),
                "Source public interface requests libstdc++ debug mode absent from the build profile"
            );
        }
    }
    let mut inspection =
        cpuv_relocate::inspect::environment_with_runtime_policy(stage, &tc.abi.arch, false)?;
    inspection["dependency_origin"] = serde_json::json!(
        "built from source with the selected compiler/profile; no universal ABI proof"
    );
    inspection["compiler"] = serde_json::to_value(tc)?;
    fs::write(
        stage.join("compatibility.json"),
        serde_json::to_vec_pretty(&inspection)?,
    )?;
    fs::write(
        stage.join("manifest.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    fs::write(
        stage.join("cpuv.specs"),
        "*cc1plus:\n+ %ecpuv source profiles require the compiler shim\n\n",
    )?;
    fs::write(
        stage.join("source-profile.json"),
        serde_json::to_vec_pretty(locked)?,
    )?;
    Ok(())
}

pub async fn sync(
    root: &Path,
    cli: &Cli,
    manifest: &Manifest,
    base_tc: &Toolchain,
    refresh: bool,
) -> Result<()> {
    let started = Instant::now();
    let old: Option<ProjectLock> = fs::read_to_string(root.join("cpuv.lock"))
        .ok()
        .and_then(|s| toml::from_str(&s).ok());
    let reuse_lock = !refresh
        && old.as_ref().is_some_and(|locked| {
            locked.dependencies == manifest.dependencies
                && locked.version == 2
                && locked.backend == "source"
        });
    let registry = if reuse_lock {
        let old = old.as_ref().unwrap();
        Registry {
            schema: 1,
            packages: old.packages.clone(),
            tools: old.tools.clone(),
        }
    } else {
        registry(cli)?
    };
    let packages = resolve(&registry, &requirements(manifest)?)?;
    order(&packages)?;
    let mut profile = Profile::environment(if cli.reset_profile {
        None
    } else {
        old.as_ref().map(|lock| &lock.profile)
    })?;
    let required_cxx = packages
        .values()
        .map(|recipe| recipe.minimum_cxx)
        .filter(|value| *value >= 11)
        .max()
        .unwrap_or(0);
    if manifest.project.std == "default"
        && required_cxx > 0
        && !profile
            .cpp
            .iter()
            .chain(&profile.cxx)
            .any(|flag| flag.starts_with("-std="))
        && (cxx_level(base_tc) == 98 || cxx_level(base_tc) < required_cxx)
    {
        profile.cxx.push(format!("-std=gnu++{required_cxx}"));
    }
    let compiler_flags = profile.compiler_flags(&manifest.project.std);
    let tc = cpuv_toolchain::probe_with_flags(Some(&base_tc.compiler_path), &compiler_flags)?;
    let compiler_sha256 = archive::digest(&tc.compiler_path)?;
    let context = serde_json::to_vec(&(
        3,
        &packages,
        &registry.tools,
        &profile,
        &tc,
        &compiler_sha256,
        &manifest.project.std,
    ))?;
    let cache_key = blake3::hash(&context).to_hex().to_string();
    let locked = SourceLock {
        version: 2,
        backend: "source".into(),
        dependencies: manifest.dependencies.clone(),
        packages,
        tools: registry.tools.clone(),
        profile,
        compiler: tc,
        compiler_sha256,
        cache_key,
    };
    let mut doctor_flags = compiler_flags;
    doctor_flags.extend(locked.profile.link.iter().map(OsString::from));
    cpuv_toolchain::doctor(&locked.compiler, &doctor_flags)?;
    let cache = cache_dir(cli)?;
    let (cached, installed, reused) =
        built_environment(&cache, &locked, &registry, cli, &manifest.project.std).await?;
    let stage = tempfile::Builder::new()
        .prefix("stage-source-")
        .tempdir_in(root.join(".cpuv"))?;
    stage_environment(
        &cached,
        &installed,
        &stage.path().join("env"),
        &root.join(".cpuv/env"),
        manifest,
        &locked,
    )?;
    fs::write(
        stage.path().join("cpuv.toml"),
        toml::to_string_pretty(manifest)?,
    )?;
    fs::write(
        stage.path().join("cpuv.lock"),
        toml::to_string_pretty(&ProjectLock::from(&locked))?,
    )?;
    fs::write(
        stage.path().join("toolchain.json"),
        serde_json::to_vec_pretty(&locked.compiler)?,
    )?;
    fs::write(
        stage.path().join("activate.ps1"),
        activation(root, &locked.compiler)?,
    )?;
    install_shim(root)?;
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
                "Source update failed: {error:#}. Automatic restoration was incomplete: {}. Preserved backups: {}",
                recovery_errors.join("; "),
                recovery.display()
            );
        }
        return Err(error);
    }
    fs::write(
        root.join(".cpuv/last-sync.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"backend":"source","packages":locked.packages.len(),"compiled_cache_reused":reused,"cache_key":locked.cache_key,"total_ms":started.elapsed().as_secs_f64()*1000.0}),
        )?,
    )?;
    eprintln!(
        "Ready from source: {} packages; compiled cache {}; {:.0} ms",
        locked.packages.len(),
        if reused { "hit" } else { "built" },
        started.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}

pub fn info(root: &Path) -> Result<()> {
    let lock: ProjectLock = toml::from_str(&fs::read_to_string(root.join("cpuv.lock"))?)?;
    println!(
        "Backend: source\nCompiler recorded by lock: GCC {}\nTarget recorded by lock: {}",
        lock.gcc_version,
        lock.abi
            .as_ref()
            .map_or("not recorded", |abi| abi.target_triple.as_str())
    );
    for (name, recipe) in lock.packages {
        println!("{name} {} (built from source)", recipe.version);
    }
    Ok(())
}
