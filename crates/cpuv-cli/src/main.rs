use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use cpuv_fetch::{DEFAULT_REPOSITORY, DownloadStats, Fetcher};
use cpuv_solver::{Index, Package};
use cpuv_store::{Store, StoredPackage};
use cpuv_toolchain::Toolchain;
use fs2::FileExt;
use futures_util::{StreamExt, TryStreamExt, stream};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

mod host_tools;
mod install;
mod runner;
mod source;
mod toolchains;
mod uninstall;
mod update;

#[derive(Parser)]
#[command(
    name = "cpuv",
    version,
    about = "C++ packages and clean compile-and-run workflows for Windows GCC"
)]
struct Cli {
    #[arg(short = 'C', long = "project", global = true, default_value = ".")]
    project: PathBuf,
    #[arg(long, global = true, env = "CPUV_CACHE_DIR")]
    cache_dir: Option<PathBuf>,
    #[arg(long, global = true, env = "CPUV_REPOSITORY")]
    repository: Option<String>,
    #[arg(long, global=true, env="CPUV_CONCURRENT_DOWNLOADS", default_value_t=50, value_parser=clap::value_parser!(u16).range(1..=256))]
    concurrent_downloads: u16,
    /// Maximum compressed repository metadata size in MiB; separate from packages.
    #[arg(long, global=true, env="CPUV_MAX_INDEX_MIB", default_value_t=64, value_parser=clap::value_parser!(u64).range(1..=1_048_576))]
    max_index_mib: u64,
    /// Maximum MSYS2/bootstrap archive size in MiB; Conan owns catalog transfers.
    #[arg(long, global=true, env="CPUV_MAX_DOWNLOAD_MIB", default_value_t=16_384, value_parser=clap::value_parser!(u64).range(1..=1_048_576))]
    max_download_mib: u64,
    /// Maximum expanded payload per MSYS2/bootstrap archive in MiB.
    #[arg(long, global=true, env="CPUV_MAX_EXTRACT_MIB", default_value_t=65_536, value_parser=clap::value_parser!(u64).range(1..=1_048_576))]
    max_extract_mib: u64,
    /// Maximum entries per MSYS2/bootstrap archive.
    #[arg(long, global=true, env="CPUV_MAX_EXTRACT_FILES", default_value_t=1_000_000, value_parser=clap::value_parser!(u64).range(1..=10_000_000))]
    max_extract_files: u64,
    /// MSYS2/bootstrap transfer timeout; 0 keeps connection/read timeouts only.
    #[arg(
        long,
        global = true,
        env = "CPUV_DOWNLOAD_TIMEOUT_SECS",
        default_value_t = 3600
    )]
    download_timeout_secs: u64,
    #[arg(long, global = true)]
    offline: bool,
    #[arg(long, global = true)]
    refresh: bool,
    #[arg(long, global = true)]
    compiler: Option<PathBuf>,
    /// Native GCC used for compiling build tools, independently of the target GCC.
    #[arg(long, global = true, env = "CPUV_BUILD_CXX")]
    build_compiler: Option<PathBuf>,
    /// Build catalog packages from source with the selected GCC.
    #[arg(long, global = true, env = "CPUV_FROM_SOURCE")]
    from_source: bool,
    /// Use the shared ConanCenter catalog for the entire dependency environment.
    #[arg(long, global = true)]
    catalog: bool,
    /// Maximum simultaneous compiler jobs for source builds.
    #[arg(long, global=true, env="CPUV_BUILD_JOBS", value_parser=clap::value_parser!(u16).range(1..=256))]
    build_jobs: Option<u16>,
    /// Clear saved source-build flags before applying the current environment.
    #[arg(long, global = true)]
    reset_profile: bool,
    #[command(subcommand)]
    command: Action,
}

impl Cli {
    fn transfer_limits(&self) -> cpuv_fetch::Limits {
        cpuv_fetch::Limits {
            index_bytes: self.max_index_mib * 1024 * 1024,
            archive_bytes: self.max_download_mib * 1024 * 1024,
            expanded_bytes: self.max_extract_mib * 1024 * 1024,
            entries: self.max_extract_files,
            timeout_secs: self.download_timeout_secs,
        }
    }
}

#[derive(Subcommand)]
enum Action {
    /// Uninstall the invoked cpuv application; keep projects and package caches.
    Uninstall(uninstall::Options),
    /// Manage the installed cpuv executable.
    #[command(name = "self")]
    SelfCommand {
        #[command(subcommand)]
        command: SelfAction,
    },
    /// Create a C++ project environment and probe the real compiler.
    Init,
    /// Add packages by repository name, optionally with a version constraint.
    Add {
        /// Keep a compatible locked installation instead of resolving it again.
        #[arg(long)]
        needed: bool,
        #[arg(required = true)]
        packages: Vec<String>,
    },
    /// Remove direct dependencies and rebuild the environment.
    #[command(visible_alias = "rm")]
    Remove {
        #[arg(required = true)]
        packages: Vec<String>,
    },
    /// Recreate exactly the locked environment; --offline uses the local CAS.
    Sync,
    /// Resolve current manifest against a fresh repository snapshot.
    Lock,
    /// Compile and run C++ in .cpuv/build, or run another project command.
    Run {
        /// Build without executing the resulting program.
        #[arg(long)]
        no_run: bool,
        #[arg(trailing_var_arg = true, required = true, allow_hyphen_values = true)]
        command: Vec<OsString>,
    },
    /// Move loose EXE, DLL, and object files into .cpuv/build/legacy (preserved).
    Clean,
    /// Open an activated PowerShell. Use --print for current-shell activation.
    Shell {
        #[arg(long)]
        print: bool,
    },
    /// Inspect the compiler ABI fingerprint.
    Toolchain {
        #[command(subcommand)]
        command: Option<toolchains::Action>,
    },
    /// Run standalone Conan in the selected compiler/profile's managed home.
    #[command(disable_help_flag = true)]
    Conan {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
    /// Compile, link and execute a program with the real compiler and optional flags.
    Doctor {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        flags: Vec<OsString>,
    },
    /// Show the resolved package list and cache location.
    Info,
}

#[derive(Subcommand)]
enum SelfAction {
    /// Install this prebuilt executable and add it to the user PATH.
    Install(install::Options),
    /// Download a verified prebuilt release and replace this executable.
    Update(update::Options),
}

#[derive(Clone, Serialize, Deserialize)]
struct Project {
    name: String,
    version: String,
    #[serde(default = "default_standard")]
    std: String,
}
fn default_standard() -> String {
    "c++20".into()
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum Dependency {
    Simple(String),
    Detailed {
        #[serde(default = "any_version")]
        version: String,
        #[serde(default)]
        linkage: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reference: Option<String>,
    },
}
fn any_version() -> String {
    "*".into()
}
impl Dependency {
    fn version(&self) -> Result<&str> {
        match self {
            Self::Simple(v) => Ok(v),
            Self::Detailed {
                version, linkage, ..
            } => {
                ensure!(
                    linkage.as_deref().is_none_or(|s| s == "shared"),
                    "Linkage is defined by the selected provider or upstream package metadata; this declaration supports default/shared linkage"
                );
                Ok(version)
            }
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct PinnedToolchain {
    target: String,
    compiler: String,
    abi: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Manifest {
    #[serde(default, skip_serializing_if = "backend_is_auto")]
    backend: Backend,
    project: Project,
    toolchain: PinnedToolchain,
    #[serde(default)]
    dependencies: BTreeMap<String, Dependency>,
    #[serde(default, skip_serializing_if = "link_selection_empty")]
    link: cpuv_relocate::linking::Selection,
}
fn link_selection_empty(selection: &cpuv_relocate::linking::Selection) -> bool {
    selection == &cpuv_relocate::linking::Selection::default()
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Backend {
    #[default]
    Auto,
    #[serde(alias = "source")]
    Conan,
}
fn backend_is_auto(value: &Backend) -> bool {
    *value == Backend::Auto
}
#[derive(Serialize, Deserialize)]
struct Lock {
    version: u32,
    abi_hash: String,
    repository: String,
    dependencies: BTreeMap<String, Dependency>,
    packages: Vec<Package>,
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().context("No parent")?)?;
    use std::io::Write;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn project_lock(root: &Path) -> Result<File> {
    runner::state_directory(root)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(".cpuv/project.lock"))?;
    file.try_lock_exclusive()
        .context("Another cpuv operation is using this project; retry when it finishes")?;
    Ok(file)
}

fn read_manifest(root: &Path) -> Result<Manifest> {
    let text = fs::read_to_string(root.join("cpuv.toml"))
        .context("No cpuv.toml found; run cpuv init first")?;
    let m: Manifest = toml::from_str(&text).context("Invalid cpuv.toml")?;
    ensure!(
        [
            "c++98", "c++03", "c++11", "c++14", "c++17", "c++20", "c++23", "c++26", "gnu++98",
            "gnu++03", "gnu++11", "gnu++14", "gnu++17", "gnu++20", "gnu++23", "gnu++26", "default"
        ]
        .contains(&m.project.std.as_str()),
        "Unsupported C++ standard; choose a GCC -std= value from C++98 through C++26"
    );
    Ok(m)
}

fn check_toolchain(manifest: &Manifest, tc: &Toolchain) -> Result<()> {
    ensure!(
        manifest.toolchain.target == tc.abi.target_triple
            && manifest.toolchain.compiler == format!("gcc@{}", tc.gcc_version)
            && (manifest.toolchain.abi == tc.runtime_tag()
                || manifest.toolchain.abi == "ucrt64-posix-seh"
                    && tc.provider.as_ref().is_some_and(|p| p.name == "ucrt64")),
        "Project toolchain pin disagrees with this compiler; select the pinned GCC compiler or reinitialize the project with the new toolchain"
    );
    Ok(())
}

fn requirements(m: &Manifest) -> Result<Vec<String>> {
    m.dependencies
        .iter()
        .map(|(name, dependency)| {
            let v = dependency.version()?;
            ensure!(
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.+".contains(&b)),
                "Invalid dependency name {name}"
            );
            if v == "*" || v.is_empty() {
                return Ok(name.clone());
            }
            ensure!(
                !v.starts_with(['^', '~']) && !v.contains([',', ' ']),
                "Use '*', an exact version, or a single >=/<= constraint instead of {v}"
            );
            Ok(format!(
                "{name}{}{v}",
                if v.starts_with(['<', '>', '=']) {
                    ""
                } else {
                    "="
                }
            ))
        })
        .collect()
}

fn install_shim(root: &Path) -> Result<()> {
    // Use this exact executable as a multicall shim. This also makes a single
    // cargo-installed cpuv sufficient and prevents stale sibling-shim versions.
    let binary = std::env::current_exe()?;
    let dir = root.join(".cpuv/bin");
    fs::create_dir_all(&dir)?;
    // Copy: linking a compiler shim to target/ would lock Cargo's build output on Windows.
    fs::copy(binary, dir.join("g++.exe"))
        .context("Installing compiler shim (close active compiler processes first)")?;
    Ok(())
}

fn activation(root: &Path, tc: &Toolchain) -> Result<String> {
    let quote = |p: &Path| cpuv_relocate::portable(p).replace('\'', "''");
    let env = quote(&root.join(".cpuv/env"));
    let host =
        cpuv_relocate::msys_host_bin(tc.compiler_path.parent().context("Missing compiler bin")?)
            .map_or(String::new(), |path| format!("{};", quote(&path)));
    Ok(format!(
        "$env:CPUV_ENV = '{env}'\n$env:PATH = '{};{env}/bin;{};{host}' + $env:PATH\n",
        quote(&root.join(".cpuv/bin")),
        quote(&tc.sysroot.join("bin"))
    ))
}

fn init(root: &Path, cli: &Cli) -> Result<()> {
    let _guard = project_lock(root)?;
    ensure!(
        !root.join("cpuv.toml").exists(),
        "cpuv.toml already exists; use cpuv sync"
    );
    let tc = cpuv_toolchain::probe(cli.compiler.as_deref())?;
    let m = Manifest {
        backend: if cli.catalog
            || cli.from_source
            || tc.provider.is_none()
            || source::custom_profile_requested()
        {
            Backend::Conan
        } else {
            Backend::Auto
        },
        project: Project {
            name: root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            version: "0.1.0".into(),
            std: if cli.catalog
                || cli.from_source
                || tc.provider.is_none()
                || source::custom_profile_requested()
            {
                "default".into()
            } else {
                tc.default_standard()
            },
        },
        toolchain: PinnedToolchain {
            target: tc.abi.target_triple.clone(),
            compiler: format!("gcc@{}", tc.gcc_version),
            abi: tc.runtime_tag(),
        },
        dependencies: BTreeMap::new(),
        link: Default::default(),
    };
    install_shim(root)?;
    let env = root.join(".cpuv/env");
    for dir in ["include", "lib", "bin"] {
        fs::create_dir_all(env.join(dir))?;
    }
    cpuv_relocate::index_environment(
        &env,
        &env,
        tc.compiler_path.clone(),
        tc.abi_hash.clone(),
        m.project.std.clone(),
    )?;
    configure_environment(&env, &tc, &m, &[])?;
    write_atomic(
        &root.join(".cpuv/toolchain.json"),
        &serde_json::to_vec_pretty(&tc)?,
    )?;
    write_atomic(
        &root.join(".cpuv/activate.ps1"),
        activation(root, &tc)?.as_bytes(),
    )?;
    write_atomic(
        &root.join("cpuv.toml"),
        toml::to_string_pretty(&m)?.as_bytes(),
    )?;
    eprintln!(
        "Initialized {} with GCC {} ({})",
        m.project.name,
        tc.gcc_version,
        tc.runtime_tag()
    );
    eprintln!("Activate: . .\\.cpuv\\activate.ps1   or use: cpuv run g++ main.cpp");
    Ok(())
}

fn validate_packages(packages: &[Package], prefix: &str) -> Result<()> {
    for p in packages {
        ensure!(
            p.name.starts_with(prefix),
            "Lock contains a package from a different compiler environment"
        );
        ensure!(
            p.sha256.len() == 64 && p.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid locked SHA-256"
        );
        ensure!(
            !p.filename.contains(['/', '\\', ':']) && p.filename.ends_with(".pkg.tar.zst"),
            "Unsafe locked filename"
        );
    }
    Ok(())
}

async fn add(root: &Path, cli: &Cli, requested: &[String], needed: bool) -> Result<()> {
    // Validate the project before performing any explicit host-tool transaction.
    let manifest = read_manifest(root)?;
    let has_host_tools = requested.iter().any(|name| name == "base-devel");
    let libraries: Vec<_> = requested
        .iter()
        .filter(|name| name.as_str() != "base-devel")
        .cloned()
        .collect();
    let tc = cpuv_toolchain::probe(cli.compiler.as_deref())?;
    let host = if has_host_tools {
        Some(host_tools::Manager::discover(&tc)?)
    } else {
        None
    };
    if !libraries.is_empty() {
        let namespace = tc
            .provider
            .as_ref()
            .map_or(cpuv_solver::PREFIX, |provider| {
                provider.package_prefix.as_str()
            });
        let locked: Option<Lock> = fs::read_to_string(root.join("cpuv.lock"))
            .ok()
            .and_then(|text| toml::from_str(&text).ok());
        let keep_lock = needed
            && (source::requests_locked(root, &manifest, &libraries)
                || locked.as_ref().is_some_and(|lock| {
                    lock.dependencies == manifest.dependencies
                        && lock.abi_hash == tc.abi_hash
                        && libraries.iter().all(|request| {
                            let (name, op, value) = cpuv_solver::split_requirement(request);
                            manifest.dependencies.iter().any(|(existing, dependency)| {
                                cpuv_solver::normalize_for(existing, namespace)
                                    == cpuv_solver::normalize_for(name, namespace)
                                    && dependency.version().is_ok_and(|constraint| {
                                        op.is_empty() || constraint == format!("{op}{value}")
                                    })
                            })
                        })
                }));
        if keep_lock {
            eprintln!(
                "Requested dependencies are already locked; keeping their versions (--needed)."
            );
            sync(root, cli, None, false).await?;
        } else {
            sync(root, cli, Some((&libraries, false)), false).await?;
        }
    }
    if let Some(host) = host {
        host.ensure_base_devel(cli.offline)?;
    }
    Ok(())
}

async fn sync(
    root: &Path,
    cli: &Cli,
    update: Option<(&[String], bool)>,
    force_resolve: bool,
) -> Result<()> {
    let start = Instant::now();
    let _guard = project_lock(root)?;
    let mut manifest = read_manifest(root)?;
    let probe_start = Instant::now();
    let tc = cpuv_toolchain::probe(cli.compiler.as_deref())?;
    let changed_compiler = check_toolchain(&manifest, &tc).is_err();
    let toolchain_probe_ms = probe_start.elapsed().as_secs_f64() * 1000.0;
    let namespace = tc
        .provider
        .as_ref()
        .map_or(cpuv_solver::PREFIX, |p| p.package_prefix.as_str());
    if let Some((names, remove)) = update {
        for input in names {
            let reference_name = if input.contains('/') {
                Some(source::reference_name(input)?)
            } else {
                None
            };
            let (name, op, v) = reference_name.map_or_else(
                || cpuv_solver::split_requirement(input),
                |name| (name, "", ""),
            );
            if remove {
                ensure!(
                    manifest.dependencies.remove(name).is_some(),
                    "{name} is not a direct dependency"
                );
            } else {
                // Reject duplicated aliases instead of representing the same root twice.
                if let Some(existing) = manifest.dependencies.keys().find(|k| {
                    *k != name
                        && cpuv_solver::normalize_for(k, namespace)
                            == cpuv_solver::normalize_for(name, namespace)
                }) {
                    bail!("{name} is already declared as {existing}");
                }
                manifest.dependencies.insert(
                    name.into(),
                    if reference_name.is_some() {
                        Dependency::Detailed {
                            version: "*".into(),
                            linkage: None,
                            reference: Some(input.clone()),
                        }
                    } else {
                        Dependency::Simple(if op.is_empty() {
                            "*".into()
                        } else {
                            format!("{op}{v}")
                        })
                    },
                );
            }
        }
    }
    let roots = requirements(&manifest)?;
    if cli.catalog
        || cli.from_source
        || manifest.backend == Backend::Conan
        || tc.provider.is_none()
        || source::custom_profile_requested()
        || source::explicit_references(&manifest)
        || changed_compiler
        || cli.reset_profile
    {
        manifest.backend = Backend::Conan;
        manifest.toolchain = PinnedToolchain {
            target: tc.abi.target_triple.clone(),
            compiler: format!("gcc@{}", tc.gcc_version),
            abi: tc.runtime_tag(),
        };
        return source::sync(root, cli, &manifest, &tc, force_resolve).await;
    }
    ensure!(
        roots.is_empty() || tc.provider.is_some(),
        "No supported binary-package provider for this GCC ABI ({}; {}; {}). Managed binaries require matching MSYS2 UCRT64 or MINGW64 x64 POSIX/SEH profiles. Other native GCC builds can use their own libraries.",
        tc.abi.target_triple,
        tc.runtime_tag(),
        tc.gcc_version
    );
    let lock_path = root.join("cpuv.lock");
    let old_lock: Option<Lock> = if lock_path.is_file() {
        Some(toml::from_str(&fs::read_to_string(&lock_path)?).context("Invalid cpuv.lock")?)
    } else {
        None
    };
    let repository = cli
        .repository
        .clone()
        .or_else(|| old_lock.as_ref().map(|l| l.repository.clone()))
        .or_else(|| tc.provider.as_ref().map(|p| p.repository.clone()))
        .unwrap_or_else(|| DEFAULT_REPOSITORY.into());
    let cache = cache_dir(cli)?;
    let fetcher = Fetcher::with_limits(
        repository.clone(),
        cache.clone(),
        cli.concurrent_downloads as usize,
        cli.offline,
        cli.transfer_limits(),
    )?
    .with_namespace(
        namespace,
        tc.provider
            .as_ref()
            .map_or("ucrt64.db", |p| p.database.as_str()),
    );
    let locked = update.is_none() && !force_resolve;
    let resolve_start = Instant::now();
    let mut index_fetch_ms = 0.0;
    let packages = if locked {
        let lock = old_lock
            .as_ref()
            .context("No cpuv.lock; run cpuv lock or cpuv add first")?;
        ensure!(
            lock.version == 1
                && lock.abi_hash == tc.abi_hash
                && lock.dependencies == manifest.dependencies
                && lock.repository == repository,
            "Lock disagrees with manifest, ABI, or repository; run cpuv lock to resolve explicitly"
        );
        validate_packages(&lock.packages, namespace)?;
        let index: Index = lock
            .packages
            .iter()
            .cloned()
            .map(|p| (p.name.clone(), p))
            .collect();
        let resolved = cpuv_solver::resolve_for(&index, &roots, namespace)?;
        ensure!(
            resolved.len() == lock.packages.len(),
            "Lock contains duplicate or unreachable packages"
        );
        resolved
    } else {
        if roots.is_empty() {
            vec![]
        } else {
            let index_start = Instant::now();
            let index = fetcher
                .index_for_requirements(&roots, cli.refresh || force_resolve)
                .await?;
            index_fetch_ms = index_start.elapsed().as_secs_f64() * 1000.0;
            let missing: Vec<_> = roots
                .iter()
                .filter(|requirement| {
                    let name = cpuv_solver::split_requirement(requirement).0;
                    !cpuv_solver::available(&index, name, namespace)
                })
                .cloned()
                .collect();
            if !missing.is_empty() {
                eprintln!(
                    "MSYS2 snapshot for {repository} does not provide: {}. Querying the shared ConanCenter catalog...",
                    missing.join(", ")
                );
                let lookup_errors = missing
                    .iter()
                    .filter_map(|requirement| {
                        cpuv_solver::resolve_for(
                            &index,
                            std::slice::from_ref(requirement),
                            namespace,
                        )
                        .err()
                        .map(|error| error.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                manifest.backend = Backend::Conan;
                return source::sync(root, cli, &manifest, &tc, force_resolve)
                    .await
                    .with_context(|| {
                        format!(
                            "Automatic catalog fallback failed. MSYS2 lookup in {repository}:\n{lookup_errors}\nPackage names and availability differ between providers; check the exact upstream name and version. ConanCenter error follows"
                        )
                    });
            }
            cpuv_solver::resolve_for(&index, &roots, namespace)?
        }
    };
    let dependency_resolution_ms =
        (resolve_start.elapsed().as_secs_f64() * 1000.0 - index_fetch_ms).max(0.0);
    validate_packages(&packages, namespace)?;
    // Rolling repositories are not a universal ABI guarantee. Never silently use
    // a newer major runtime with an older host compiler in this Phase A prototype.
    for p in &packages {
        if ["cc-libs", "libstdc++"]
            .iter()
            .any(|name| p.name == format!("{namespace}{name}"))
        {
            let major = p.version.split('.').next().unwrap_or("");
            if major != tc.abi.compiler_abi_major {
                eprintln!(
                    "Repository runtime {} differs from GCC {}; querying ConanCenter for the selected compiler",
                    p.version, tc.gcc_version
                );
                manifest.backend = Backend::Conan;
                return source::sync(root, cli, &manifest, &tc, force_resolve).await;
            }
        }
    }
    let store = Store::new(cache.join("store"))?
        .with_extraction_limits(cli.transfer_limits().expanded_bytes, cli.max_extract_files);
    let cache_start = Instant::now();
    let mut stored = BTreeMap::<String, StoredPackage>::new();
    let mut missing = Vec::new();
    for p in &packages {
        if let Some(s) = store.cached(p)? {
            stored.insert(p.name.clone(), s);
        } else {
            missing.push(p.clone());
        }
    }
    let cache_validation_ms = cache_start.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "Resolved {} packages; {} in CAS; download limit {}",
        packages.len(),
        stored.len(),
        cli.concurrent_downloads
    );
    let (archives, downloads) = fetcher.download_all(&missing).await?;
    let extract_start = Instant::now();
    let store_ref = &store;
    let extracted: Vec<_> = stream::iter(archives.into_iter().map(|(p, path)| {
        let store = store_ref.clone();
        async move { tokio::task::spawn_blocking(move || store.ingest(&p, &path)).await? }
    }))
    .buffer_unordered(4)
    .try_collect()
    .await?;
    for package in extracted {
        stored.insert(package.package.name.clone(), package);
    }
    let extract_ms = extract_start.elapsed().as_millis();
    let stored: Vec<_> = packages
        .iter()
        .map(|p| stored.remove(&p.name).unwrap())
        .collect();
    let stage = tempfile::Builder::new()
        .prefix("stage-")
        .tempdir_in(root.join(".cpuv"))?;
    let stage_env = stage.path().join("env");
    let final_env = root.join(".cpuv/env");
    let link_start = Instant::now();
    let identity = serde_json::to_vec(&(
        3,
        &tc.provider,
        &tc.abi_hash,
        &tc.compiler_path,
        &manifest.project.std,
        &manifest.link,
        &packages,
    ))?;
    let environment_reused = fs::read(final_env.join("install-state.json"))
        .is_ok_and(|bytes| bytes == identity)
        && [
            "manifest.json",
            "include_map.json",
            "cpuv.specs",
            "link-plan.json",
            "compatibility.json",
        ]
        .iter()
        .all(|name| final_env.join(name).is_file())
        && stored
            .iter()
            .all(|p| p.files.iter().all(|f| final_env.join(&f.path).is_file()));
    let mut materialization_ms = 0.0;
    let mut metadata_ms = 0.0;
    let mut binary_inspection_ms = 0.0;
    let link_stats = if environment_reused {
        cpuv_store::LinkStats::default()
    } else {
        let materialize_start = Instant::now();
        let stats = store.materialize(&stored, &stage_env)?;
        materialization_ms = materialize_start.elapsed().as_secs_f64() * 1000.0;
        let metadata_start = Instant::now();
        cpuv_relocate::index_environment(
            &stage_env,
            &final_env,
            tc.compiler_path.clone(),
            tc.abi_hash.clone(),
            manifest.project.std.clone(),
        )?;
        binary_inspection_ms = configure_environment(&stage_env, &tc, &manifest, &stored)?;
        metadata_ms =
            (metadata_start.elapsed().as_secs_f64() * 1000.0 - binary_inspection_ms).max(0.0);
        fs::write(stage_env.join("install-state.json"), identity)?;
        stats
    };
    let environment_prepare_ms = link_start.elapsed().as_secs_f64() * 1000.0;
    let lock = Lock {
        version: 1,
        abi_hash: tc.abi_hash.clone(),
        repository,
        dependencies: manifest.dependencies.clone(),
        packages: packages.clone(),
    };
    // Prepare every artifact before switching the live environment.
    fs::write(
        stage.path().join("cpuv.toml"),
        toml::to_string_pretty(&manifest)?,
    )?;
    fs::write(
        stage.path().join("cpuv.lock"),
        toml::to_string_pretty(&lock)?,
    )?;
    install_shim(root)?;
    // Recovery uses backups until all three project artifacts have committed.
    let mut targets = vec![("cpuv.toml", "cpuv.toml"), ("cpuv.lock", "cpuv.lock")];
    if !environment_reused {
        targets.insert(0, (".cpuv/env", "env"));
    }
    let mut swapped: Vec<(&str, String)> = Vec::new();
    let commit: Result<()> = (|| {
        for (target, staged) in targets {
            let destination = root.join(target);
            let backup = format!("backup-{staged}");
            if destination.exists() {
                fs::rename(&destination, stage.path().join(&backup))?;
            }
            if let Err(error) = fs::rename(stage.path().join(staged), &destination) {
                if stage.path().join(&backup).exists() {
                    fs::rename(stage.path().join(&backup), &destination)?;
                }
                return Err(error.into());
            }
            swapped.push((target, backup));
        }
        Ok(())
    })();
    if let Err(error) = commit {
        for (target, backup) in swapped.into_iter().rev() {
            let target = root.join(target);
            if target.is_dir() {
                fs::remove_dir_all(&target)?;
            } else {
                fs::remove_file(&target)?;
            }
            if stage.path().join(&backup).exists() {
                fs::rename(stage.path().join(&backup), target)?;
            }
        }
        return Err(error).context("Commit failed; previous project restored");
    }
    write_atomic(
        &root.join(".cpuv/toolchain.json"),
        &serde_json::to_vec_pretty(&tc)?,
    )?;
    write_atomic(
        &root.join(".cpuv/activate.ps1"),
        activation(root, &tc)?.as_bytes(),
    )?;
    #[derive(Serialize)]
    struct Report<'a> {
        packages: usize,
        cas_hits: usize,
        environment_reused: bool,
        downloads: &'a DownloadStats,
        extraction_ms: u128,
        materialization_ms: u128,
        environment_prepare_ms: f64,
        toolchain_probe_ms: f64,
        index_fetch_ms: f64,
        dependency_resolution_ms: f64,
        cache_validation_ms: f64,
        metadata_ms: f64,
        binary_inspection_ms: f64,
        total_ms: u128,
        links: &'a cpuv_store::LinkStats,
    }
    let report = Report {
        packages: packages.len(),
        cas_hits: packages.len() - missing.len(),
        environment_reused,
        downloads: &downloads,
        extraction_ms: extract_ms,
        materialization_ms: materialization_ms as u128,
        environment_prepare_ms,
        toolchain_probe_ms,
        index_fetch_ms,
        dependency_resolution_ms,
        cache_validation_ms,
        metadata_ms,
        binary_inspection_ms,
        total_ms: start.elapsed().as_millis(),
        links: &link_stats,
    };
    write_atomic(
        &root.join(".cpuv/last-sync.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    eprintln!(
        "Ready in {} ms: {} downloaded, {} CAS hits, peak {} parallel transfers, {} hardlinks / {} copies",
        report.total_ms,
        downloads.downloaded,
        report.cas_hits,
        downloads.peak_concurrent_downloads,
        link_stats.hardlinks,
        link_stats.copies
    );
    for p in packages {
        eprintln!("  + {} {}", p.name.trim_start_matches(namespace), p.version);
    }
    Ok(())
}

fn cache_dir(cli: &Cli) -> Result<PathBuf> {
    let dir = cli
        .cache_dir
        .clone()
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("cpuv")))
        .context("Set --cache-dir or LOCALAPPDATA")?;
    fs::create_dir_all(&dir)?;
    Ok(cpuv_toolchain::driver_path(dir.canonicalize()?))
}

fn configure_environment(
    env: &Path,
    tc: &Toolchain,
    manifest: &Manifest,
    stored: &[StoredPackage],
) -> Result<f64> {
    let path = env.join("manifest.json");
    let mut metadata: cpuv_relocate::Environment = serde_json::from_slice(&fs::read(&path)?)?;
    let packages: Vec<_> = stored
        .iter()
        .map(|package| cpuv_relocate::linking::PackageFiles {
            name: package.package.name.clone(),
            files: package.files.iter().map(|file| file.path.clone()).collect(),
        })
        .collect();
    cpuv_relocate::linking::apply_for(
        env,
        &mut metadata,
        &packages,
        &requirements(manifest)?,
        &manifest.link,
        tc.provider
            .as_ref()
            .map_or(cpuv_solver::PREFIX, |provider| {
                provider.package_prefix.as_str()
            }),
    )?;
    let inspect_start = Instant::now();
    let mut inspection =
        cpuv_relocate::inspect::environment_for_runtime(env, &tc.abi.arch, &tc.abi.crt_runtime)?;
    inspection["observed_compiler_abi_macros"] = serde_json::to_value(&tc.observed_abi_options)?;
    inspection["fingerprint_semantics"] = serde_json::json!(
        "coarse compatibility filter; equality does not prove binary compatibility"
    );
    let inspection_ms = inspect_start.elapsed().as_secs_f64() * 1000.0;
    fs::write(
        env.join("compatibility.json"),
        serde_json::to_vec_pretty(&inspection)?,
    )?;
    for define in &metadata.defines {
        if let Some(value) = define.strip_prefix("_GLIBCXX_USE_CXX11_ABI=") {
            ensure!(
                value == tc.abi.glibcxx_cxx11_abi,
                "Package metadata requests libstdc++ dual ABI {value}, but the selected compiler uses {}; rebuild dependencies with a matching profile",
                tc.abi.glibcxx_cxx11_abi
            );
        }
    }
    if metadata.managed_binaries && ["0", "1"].contains(&tc.abi.glibcxx_cxx11_abi.as_str()) {
        metadata.defines.push(format!(
            "_GLIBCXX_USE_CXX11_ABI={}",
            tc.abi.glibcxx_cxx11_abi
        ));
    }
    // Validate package and explicit project flags before committing the environment.
    cpuv_relocate::validate_profile(&metadata, &[])?;
    fs::write(path, serde_json::to_vec_pretty(&metadata)?)?;
    fs::write(
        env.join("cpuv.specs"),
        cpuv_relocate::generate_specs(&metadata),
    )?;
    Ok(inspection_ms)
}

fn main() {
    if install::is_setup() {
        match install::setup() {
            Ok(code) => std::process::exit(code),
            Err(_) => std::process::exit(1),
        }
    }
    let shim = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(runner::compiler_command))
        .unwrap_or(false);
    let result = if shim {
        cpuv_shim::run()
    } else {
        tokio::runtime::Runtime::new()
            .context("Starting cpuv runtime")
            .and_then(|runtime| runtime.block_on(execute(Cli::parse())))
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(1);
        }
    }
}

async fn execute(cli: Cli) -> Result<i32> {
    if let Action::Uninstall(options) = &cli.command {
        return uninstall::run(options);
    }
    if let Action::SelfCommand {
        command: SelfAction::Install(options),
    } = &cli.command
    {
        return install::install(options);
    }
    if let Action::SelfCommand {
        command: SelfAction::Update(options),
    } = &cli.command
    {
        return update::run(options, cli.offline).await;
    }
    if let Action::Toolchain { command } = &cli.command {
        if let Some(action) = command {
            return toolchains::execute(action, &cli);
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&cpuv_toolchain::probe(cli.compiler.as_deref())?)?
        );
        return Ok(0);
    }
    if let Action::Doctor { flags } = &cli.command {
        let tc = cpuv_toolchain::probe_with_flags(cli.compiler.as_deref(), flags)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&cpuv_toolchain::doctor(&tc, flags)?)?
        );
        return Ok(0);
    }
    fs::create_dir_all(&cli.project)?;
    let root = cpuv_toolchain::driver_path(cli.project.canonicalize()?);
    if let Action::Conan { arguments } = &cli.command {
        return source::execute(&root, &cli, arguments).await;
    }
    match &cli.command {
        Action::Init => init(&root, &cli)?,
        Action::Add { packages, needed } => add(&root, &cli, packages, *needed).await?,
        Action::Remove { packages } => sync(&root, &cli, Some((packages, true)), false).await?,
        Action::Sync => sync(&root, &cli, None, false).await?,
        Action::Lock => sync(&root, &cli, None, true).await?,
        Action::Run { command, no_run } => {
            let metadata_path = root.join(".cpuv/env/manifest.json");
            if metadata_path.is_file()
                && serde_json::from_slice::<cpuv_relocate::Environment>(&fs::read(&metadata_path)?)?
                    .link_model
                    != 2
            {
                eprintln!(
                    "Updating legacy compiler metadata to the authoritative package link model..."
                );
                if root.join("cpuv.lock").is_file() {
                    sync(&root, &cli, None, false).await?;
                } else {
                    let manifest = read_manifest(&root)?;
                    ensure!(
                        manifest.dependencies.is_empty(),
                        "Run cpuv sync first to restore the dependency lock"
                    );
                    let tc = cpuv_toolchain::probe(cli.compiler.as_deref())?;
                    cpuv_relocate::index_environment(
                        &root.join(".cpuv/env"),
                        &root.join(".cpuv/env"),
                        tc.compiler_path.clone(),
                        tc.abi_hash.clone(),
                        manifest.project.std.clone(),
                    )?;
                    configure_environment(&root.join(".cpuv/env"), &tc, &manifest, &[])?;
                    install_shim(&root)?;
                }
            }
            return runner::run(&root, command, *no_run);
        }
        Action::Clean => {
            runner::clean(&root)?;
        }
        Action::Shell { print } => {
            let script = root.join(".cpuv/activate.ps1");
            ensure!(script.is_file(), "Run cpuv init first");
            if *print {
                print!("{}", fs::read_to_string(script)?);
            } else {
                let startup = format!(
                    ". '{}'",
                    cpuv_relocate::portable(&script).replace('\'', "''")
                );
                return Ok(Command::new("powershell.exe")
                    .args([
                        "-NoLogo",
                        "-NoExit",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-Command",
                        &startup,
                    ])
                    .current_dir(root)
                    .status()?
                    .code()
                    .unwrap_or(1));
            }
        }
        Action::Info => {
            println!(
                "Project: {}\nCache: {}",
                root.display(),
                cache_dir(&cli)?.display()
            );
            if root.join("cpuv.lock").is_file() {
                if read_manifest(&root)?.backend == Backend::Conan {
                    source::info(&root)?;
                    return Ok(0);
                }
                let lock: Lock = toml::from_str(&fs::read_to_string(root.join("cpuv.lock"))?)?;
                println!("ABI: {}\nRepository: {}", lock.abi_hash, lock.repository);
                for p in lock.packages {
                    println!("{} {}", p.name, p.version);
                }
            }
        }
        Action::Toolchain { .. }
        | Action::Conan { .. }
        | Action::Doctor { .. }
        | Action::SelfCommand { .. }
        | Action::Uninstall(_) => unreachable!(),
    }
    Ok(0)
}
