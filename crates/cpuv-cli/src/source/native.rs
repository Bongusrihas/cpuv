//! Declarative source inputs. Build rules are selected by build system, never package name.
use super::*;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Input {
    Archive {
        url: String,
        sha256: String,
        format: String,
        #[serde(default = "super::one")]
        strip_components: usize,
    },
    Git {
        url: String,
        revision: String,
        #[serde(default)]
        submodules: bool,
    },
    Local {
        path: PathBuf,
        sha256: String,
    },
    Sdk {
        path: PathBuf,
        sha256: String,
        target: String,
        runtime: String,
    },
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum System {
    #[default]
    Cmake,
    Meson,
    Make,
    Msbuild,
    Copy,
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Build {
    #[serde(default)]
    pub system: System,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub test_arguments: Vec<String>,
    #[serde(default)]
    pub project: Option<PathBuf>,
    /// Explicit source-to-prefix copies for build systems without an install target.
    #[serde(default)]
    pub install: BTreeMap<PathBuf, PathBuf>,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Interface {
    #[serde(default)]
    pub libraries: Vec<String>,
    #[serde(default)]
    pub system_libraries: Vec<String>,
    #[serde(default)]
    pub defines: Vec<String>,
    #[serde(default)]
    pub cmake_file: Option<String>,
    #[serde(default)]
    pub cmake_target: Option<String>,
    #[serde(default)]
    pub include_dirs: Vec<String>,
    #[serde(default)]
    pub library_dirs: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub input: Input,
    #[serde(default)]
    pub build: Build,
    #[serde(default)]
    pub interface: Interface,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub license_files: Vec<PathBuf>,
    /// Must be explicit for SDKs; export never assumes redistribution rights.
    #[serde(default)]
    pub redistributable: bool,
}

#[derive(Subcommand)]
pub enum Action {
    /// Compute the SHA-256 snapshot identity for a local source/SDK directory.
    Inspect { directory: PathBuf },
    /// Add a dependency declaration from a project-owned TOML specification.
    Add {
        name: String,
        specification: PathBuf,
    },
}

pub async fn execute(action: &Action, root: &Path, cli: &Cli) -> Result<i32> {
    match action {
        Action::Inspect { directory } => {
            let files = files(directory)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "sha256": snapshot_hash(&files)?, "files":files.len(),
                    "excluded":[".git",".cpuv"]
                }))?
            );
        }
        Action::Add {
            name,
            specification,
        } => {
            let dependency: Dependency = toml::from_str(&fs::read_to_string(specification)?)
                .context("Expected a dependency declaration with version and [source.input]")?;
            ensure!(
                matches!(
                    dependency,
                    Dependency::Detailed {
                        source: Some(_),
                        ..
                    }
                ),
                "A source dependency needs a [source.input] declaration"
            );
            reference(name, &dependency)?;
            super::super::sync_request(root, cli, None, true, Some((name.clone(), dependency)))
                .await?;
        }
    }
    Ok(0)
}

fn files(root: &Path) -> Result<BTreeMap<String, String>> {
    ensure!(
        root.is_dir(),
        "Source directory is missing: {}",
        root.display()
    );
    let mut result = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| ![".git", ".cpuv"].iter().any(|n| e.file_name() == *n))
    {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "Source snapshots cannot contain symlinks"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "Source snapshots cannot contain reparse points"
            );
        }
        if entry.file_type().is_file() {
            cpuv_core::regular(entry.path())?;
            let relative = entry.path().strip_prefix(root)?;
            cpuv_store::safe_relative(relative)?;
            let key = relative
                .to_str()
                .context("Source paths must be Unicode")?
                .replace('\\', "/");
            result.insert(key, archive::digest(entry.path())?);
        }
    }
    ensure!(!result.is_empty(), "Source snapshot is empty");
    Ok(result)
}

fn snapshot_hash(files: &BTreeMap<String, String>) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(files)?)))
}

fn copy_snapshot(from: &Path, to: &Path, entries: &BTreeMap<String, String>) -> Result<()> {
    for (relative, digest) in entries {
        let relative = cpuv_store::safe_relative(Path::new(relative))?;
        let source = from.join(&relative);
        cpuv_core::regular(&source)?;
        let destination = to.join(relative);
        if destination.is_file() && archive::digest(&destination)? == *digest {
            continue;
        }
        fs::create_dir_all(
            destination
                .parent()
                .context("Source destination has no parent")?,
        )?;
        fs::copy(source, &destination)?;
        ensure!(
            archive::digest(&destination)? == *digest,
            "Source changed while being copied"
        );
    }
    Ok(())
}

pub(crate) fn project_copy(source: &Path, destination: &Path) -> Result<()> {
    copy_snapshot(source, destination, &files(source)?)
}

/// Reconcile only files copied from the project; build-generated outputs retain ownership.
pub(crate) fn sync_project_copy(source: &Path, destination: &Path, receipt: &Path) -> Result<()> {
    let current = files(source)?;
    let previous: BTreeMap<String, String> = if receipt.exists() {
        cpuv_core::regular(receipt)?;
        serde_json::from_slice(&fs::read(receipt)?)?
    } else {
        if destination.exists() {
            // Legacy trees have no ownership record. Preserve them rather than guessing.
            cpuv_core::quarantine::preserve(
                destination,
                &destination
                    .parent()
                    .context("Missing build parent")?
                    .join("source-backups"),
            )?;
        }
        BTreeMap::new()
    };
    // Validate all deletions/collisions before mutating the copied tree.
    for (relative, digest) in &previous {
        cpuv_core::contained(destination, Path::new(relative))?;
        let target = destination.join(relative);
        if !current.contains_key(relative) && target.exists() {
            cpuv_core::regular(&target)?;
            ensure!(
                archive::digest(&target)? == *digest,
                "Copied source {} was modified by a build; preserve or move it before removing its project source",
                target.display()
            );
        }
    }
    for (relative, digest) in &current {
        cpuv_core::contained(destination, Path::new(relative))?;
        let target = destination.join(relative);
        if target.exists() && !previous.contains_key(relative) {
            cpuv_core::regular(&target)?;
            ensure!(
                archive::digest(&target)? == *digest,
                "Project source conflicts with a build-generated file: {}",
                target.display()
            );
        }
    }
    for relative in previous.keys().filter(|p| !current.contains_key(*p)) {
        let target = destination.join(relative);
        if target.exists() {
            fs::remove_file(target)?;
        }
    }
    copy_snapshot(source, destination, &current)?;
    cpuv_core::atomic_write(receipt, &serde_json::to_vec_pretty(&current)?)
}

fn validate_digest(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit()),
        "Expected a 64-digit SHA-256"
    );
    Ok(())
}

pub fn validate_build(build: &Build) -> Result<()> {
    let reserved = [
        "CMAKE_C_COMPILER",
        "CMAKE_CXX_COMPILER",
        "CMAKE_TOOLCHAIN_FILE",
        "CMAKE_INSTALL_PREFIX",
        "CMAKE_MAKE_PROGRAM",
        "CMAKE_BUILD_TYPE",
        "CMAKE_CXX_STANDARD",
        "CMAKE_CXX_STANDARD_REQUIRED",
        "CMAKE_CXX_EXTENSIONS",
        "CMAKE_MSVC_RUNTIME_LIBRARY",
        "CMAKE_C_FLAGS",
        "CMAKE_CXX_FLAGS",
        "CC",
        "CXX",
        "CLTOOLPATH",
        "CLTOOLEXE",
        "PLATFORMTOOLSET",
        "VCTOOLSVERSION",
        "WINDOWSTARGETPLATFORMVERSION",
        "LINKTOOLPATH",
        "LINKTOOLEXE",
        "FORCEIMPORTBEFORECPPTARGETS",
        "CONFIGURATION",
        "PLATFORM",
    ];
    if let Some(path) = &build.project {
        cpuv_store::safe_relative(path)?;
    }
    for (name, value) in &build.variables {
        ensure!(
            !name.is_empty()
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && !value.contains(['\r', '\n', '\0']),
            "Invalid build variable"
        );
        ensure!(
            !reserved.contains(&name.to_ascii_uppercase().as_str())
                && !name.to_ascii_uppercase().starts_with("CMAKE_C_FLAGS_")
                && !name.to_ascii_uppercase().starts_with("CMAKE_CXX_FLAGS_"),
            "Compiler/profile setting {name} is managed by cpuv"
        );
    }
    for argument in build.arguments.iter().chain(&build.test_arguments) {
        ensure!(!argument.contains('\0'), "Invalid build argument");
        ensure!(
            !["--config", "/p", "/property", "-p", "-property"]
                .iter()
                .any(|flag| argument.eq_ignore_ascii_case(flag)
                    || argument.starts_with(&format!("{flag}="))),
            "Build configuration is owned by the profile"
        );
        let normalized = argument.to_ascii_uppercase();
        let key = normalized
            .trim_start_matches("/P:")
            .trim_start_matches("-P:")
            .trim_start_matches("/PROPERTY:")
            .trim_start_matches("-PROPERTY:")
            .trim_start_matches("-D")
            .split(['=', ':'])
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        ensure!(
            !reserved.contains(&key.as_str())
                && !key.starts_with("CMAKE_C_FLAGS_")
                && !key.starts_with("CMAKE_CXX_FLAGS_")
                && !(normalized.contains(';')
                    && ["/P:", "-P:", "/PROPERTY:", "-PROPERTY:"]
                        .iter()
                        .any(|p| normalized.starts_with(p))),
            "Compiler/profile setting {key} is managed by cpuv"
        );
    }
    if build.system == System::Cmake {
        for argument in &build.arguments {
            let key = argument
                .trim_start_matches("-D")
                .split(['=', ':'])
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            ensure!(
                !reserved.contains(&key.as_str())
                    && !["--toolchain", "--preset", "-C", "-U", "-G", "-S", "-B"]
                        .iter()
                        .any(|s| argument.starts_with(s)),
                "CMake compiler/generator/source/build paths are owned by the profile"
            );
        }
    }
    Ok(())
}

fn validate(spec: &Spec) -> Result<()> {
    validate_build(&spec.build)?;
    ensure!(
        spec.build.test_arguments.is_empty(),
        "Package builds do not use project test_arguments; declare tests in the upstream build or run cpuv test on a consumer"
    );
    if let Input::Local { path, .. } | Input::Sdk { path, .. } = &spec.input {
        ensure!(
            !path.components().any(|c| matches!(
                c,
                std::path::Component::Prefix(_) | std::path::Component::RootDir
            )),
            "Local source/SDK paths must be project-relative; copy the input under the project or use a project-relative path"
        );
    }
    if let Input::Git { url, .. } = &spec.input {
        ensure!(
            !Path::new(url).components().any(|c| matches!(
                c,
                std::path::Component::Prefix(_) | std::path::Component::RootDir
            )) || url.starts_with("https://")
                || url.starts_with("http://"),
            "Git declarations require HTTPS or a project-relative repository path"
        );
    }
    match &spec.input {
        Input::Archive { sha256, .. } | Input::Local { sha256, .. } | Input::Sdk { sha256, .. } => {
            validate_digest(sha256)?
        }
        Input::Git { revision, .. } => ensure!(
            revision.len() == 40 && revision.bytes().all(|c| c.is_ascii_hexdigit()),
            "Git sources require an immutable 40-digit commit, not a branch or tag"
        ),
    }
    for path in spec
        .license_files
        .iter()
        .chain(spec.build.install.keys())
        .chain(spec.build.install.values())
    {
        cpuv_store::safe_relative(path)?;
    }
    if let Some(path) = &spec.build.project {
        cpuv_store::safe_relative(path)?;
    }
    ensure!(
        spec.build.system != System::Msbuild || spec.build.project.is_some(),
        "MSBuild sources need build.project"
    );
    ensure!(
        !matches!(spec.build.system, System::Make | System::Msbuild)
            || !spec.build.install.is_empty(),
        "Make/MSBuild sources require explicit build.install mappings"
    );
    for key in spec.build.variables.keys() {
        ensure!(
            !key.is_empty() && key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'),
            "Invalid build variable"
        );
        ensure!(
            ![
                "CMAKE_C_COMPILER",
                "CMAKE_CXX_COMPILER",
                "CMAKE_TOOLCHAIN_FILE",
                "CMAKE_INSTALL_PREFIX",
                "CMAKE_MAKE_PROGRAM"
            ]
            .contains(&key.as_str()),
            "Compiler/tool/prefix settings are owned by the selected profile: {key}"
        );
    }
    for directory in spec
        .interface
        .include_dirs
        .iter()
        .chain(&spec.interface.library_dirs)
    {
        if directory != "." {
            cpuv_store::safe_relative(Path::new(directory))?;
        }
    }
    for name in spec
        .interface
        .libraries
        .iter()
        .chain(&spec.interface.system_libraries)
    {
        ensure!(
            !name.is_empty() && !name.starts_with('-') && !name.contains(['/', '\\', '\r', '\n']),
            "Library interfaces must contain names, not paths or command-line flags"
        );
    }
    for tool in &spec.build.tools {
        super::reference_name(tool)?;
    }
    Ok(())
}

pub fn reference(name: &str, dependency: &Dependency) -> Result<String> {
    let Dependency::Detailed {
        source: Some(spec),
        reference,
        version,
        ..
    } = dependency
    else {
        bail!("Missing source declaration");
    };
    validate(spec)?;
    ensure!(
        reference.is_none(),
        "Choose a source input or a catalog reference, not both"
    );
    ensure!(
        !version.is_empty()
            && version != "*"
            && version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._+-".contains(&c)),
        "Source dependencies require an exact version"
    );
    super::reference_name(&format!("{name}/{version}"))?;
    let digest = blake3::hash(&serde_json::to_vec(spec)?)
        .to_hex()
        .to_string();
    Ok(format!("{name}/{version}@cpuv/source-{}", &digest[..20]))
}

fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "core.hooksPath=NUL",
            "-c",
            "protocol.ext.allow=never",
            "-C",
        ])
        .arg(root)
        .env("GIT_TERMINAL_PROMPT", "0");
    command
}

fn git_url(url: &str) -> Result<bool> {
    if Path::new(url).is_absolute() && Path::new(url).is_dir() {
        return Ok(true);
    }
    let parsed = reqwest::Url::parse(url)?;
    ensure!(
        cpuv_fetch::policy::allowed_url(&parsed),
        "Git sources require HTTPS, a loopback server, or an explicit local repository"
    );
    Ok(false)
}

fn check_size(
    root: &Path,
    entries: &BTreeMap<String, String>,
    limits: &cpuv_fetch::Limits,
) -> Result<()> {
    ensure!(
        entries.len() as u64 <= limits.entries,
        "Source snapshot exceeds the file-count limit"
    );
    let bytes = entries.keys().try_fold(0u64, |n, p| -> Result<u64> {
        n.checked_add(fs::metadata(root.join(p))?.len())
            .context("Source size overflow")
    })?;
    ensure!(
        bytes <= limits.expanded_bytes,
        "Source snapshot exceeds the expanded size limit"
    );
    Ok(())
}

async fn snapshot(root: &Path, cli: &Cli, spec: &Spec, base: &Path, log: &Path) -> Result<PathBuf> {
    validate(spec)?;
    let key = blake3::hash(&serde_json::to_vec(&spec.input)?)
        .to_hex()
        .to_string();
    let parent = base.join("source-inputs");
    fs::create_dir_all(&parent)?;
    let guard = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(parent.join(format!("{key}.lock")))?;
    guard.lock_exclusive()?;
    let cached = parent.join(&key);
    let receipt = cached.join("source-files.json");
    if receipt.is_file() {
        let expected: BTreeMap<String, String> = serde_json::from_slice(&fs::read(&receipt)?)?;
        ensure!(
            files(&cached.join("src"))? == expected,
            "Source cache changed; preserve it for diagnosis and restore from its pinned input"
        );
        if let Input::Local { path, sha256 } | Input::Sdk { path, sha256, .. } = &spec.input {
            let input = root.join(path);
            if input.is_dir() {
                ensure!(
                    snapshot_hash(&files(&input)?)?.eq_ignore_ascii_case(sha256),
                    "Local source/SDK changed; inspect it and update its declared SHA-256 explicitly"
                );
            }
        }
        check_size(&cached.join("src"), &expected, &cli.transfer_limits())?;
        return Ok(cached.join("src"));
    }
    let stage = tempfile::Builder::new()
        .prefix("source-")
        .tempdir_in(&parent)?;
    let source = stage.path().join("src");
    match &spec.input {
        Input::Archive {
            url,
            sha256,
            format,
            strip_components,
        } => {
            let archive_spec = Archive {
                url: url.clone(),
                sha256: sha256.clone(),
                archive: format.clone(),
                strip_components: *strip_components,
                include: vec![],
                exclude: vec![],
            };
            let downloaded = archive::download(
                &base.join("archives"),
                &archive_spec,
                cli.offline,
                &cli.transfer_limits(),
            )
            .await?;
            archive::extract(&downloaded, &archive_spec, &source, &cli.transfer_limits())?;
        }
        Input::Local { path, sha256 } | Input::Sdk { path, sha256, .. } => {
            let input = root.join(path);
            let entries = files(&input)?;
            check_size(&input, &entries, &cli.transfer_limits())?;
            ensure!(
                snapshot_hash(&entries)?.eq_ignore_ascii_case(sha256),
                "Local source/SDK SHA-256 mismatch; use cpuv source inspect PATH"
            );
            copy_snapshot(&input, &source, &entries)?;
        }
        Input::Git {
            url,
            revision,
            submodules,
        } => {
            let location = if !url.contains("://") && root.join(url).is_dir() {
                cpuv_toolchain::driver_path(root.join(url).canonicalize()?)
                    .to_string_lossy()
                    .replace('\\', "/")
            } else {
                url.clone()
            };
            let local = git_url(&location)?;
            ensure!(
                !cli.offline || local,
                "Pinned Git snapshot is unavailable offline: {revision}"
            );
            let checkout = stage.path().join("checkout");
            fs::create_dir_all(&checkout)?;
            run(git_command(&checkout).arg("init"), log)?;
            run(
                git_command(&checkout).args(["remote", "add", "origin", &location]),
                log,
            )?;
            run(
                git_command(&checkout).args(["fetch", "--depth=1", "origin", revision]),
                log,
            )?;
            run(
                git_command(&checkout).args(["checkout", "--detach", "FETCH_HEAD"]),
                log,
            )?;
            let head = git_command(&checkout)
                .args(["rev-parse", "HEAD"])
                .bounded_output()?;
            ensure!(
                head.status.success()
                    && String::from_utf8_lossy(&head.stdout)
                        .trim()
                        .eq_ignore_ascii_case(revision),
                "Git checkout does not match the declared commit"
            );
            if *submodules {
                // Parent commits pin submodule commits. Disable executable transports and
                // local-file transports for remote inputs, including nested submodules.
                run(
                    git_command(&checkout).args([
                        "-c",
                        if local {
                            "protocol.file.allow=always"
                        } else {
                            "protocol.file.allow=never"
                        },
                        "submodule",
                        "update",
                        "--init",
                        "--recursive",
                        "--depth=1",
                    ]),
                    log,
                )?;
            }
            let entries = files(&checkout)?;
            check_size(&checkout, &entries, &cli.transfer_limits())?;
            copy_snapshot(&checkout, &source, &entries)?;
        }
    }
    let entries = files(&source)?;
    check_size(&source, &entries, &cli.transfer_limits())?;
    fs::write(
        stage.path().join("source-files.json"),
        serde_json::to_vec_pretty(&entries)?,
    )?;
    fs::rename(stage.path(), &cached)?;
    Ok(cached.join("src"))
}

pub(super) struct RecipeContext<'a> {
    pub base: &'a Path,
    pub home: &'a Path,
    pub path: &'a std::ffi::OsStr,
    pub conan: &'a Path,
    pub log: &'a Path,
}

pub(super) async fn prepare(
    root: &Path,
    cli: &Cli,
    manifest: &Manifest,
    tc: &Toolchain,
    context: &RecipeContext<'_>,
) -> Result<()> {
    let RecipeContext {
        base,
        home,
        path,
        conan,
        log,
    } = *context;
    for (name, dependency) in &manifest.dependencies {
        let Dependency::Detailed {
            source: Some(spec), ..
        } = dependency
        else {
            continue;
        };
        let reference = reference(name, dependency)?;
        ensure!(
            spec.build.system != System::Msbuild || tc.driver == "msvc",
            "MSBuild source packages require the MSVC or clang-cl driver"
        );
        if let Input::Sdk {
            target, runtime, ..
        } = &spec.input
        {
            ensure!(
                *target == tc.abi.target_triple && *runtime == tc.abi.crt_runtime,
                "SDK {name} target/runtime disagrees with the selected compiler"
            );
            ensure!(
                spec.build.system == System::Copy,
                "SDK declarations use the copy build system"
            );
        }
        let source = snapshot(
            &super::super::profiles::project_root(root)?,
            cli,
            spec,
            base,
            log,
        )
        .await?;
        let temporary = tempfile::Builder::new()
            .prefix("recipe-")
            .tempdir_in(base)?;
        let entries = files(&source)?;
        copy_snapshot(&source, &temporary.path().join("source"), &entries)?;
        let requires = spec
            .requires
            .iter()
            .map(|requirement| {
                manifest.dependencies.get(requirement).map_or_else(
                    || {
                        super::reference_name(requirement)?;
                        Ok(requirement.clone())
                    },
                    |dependency| super::reference(requirement, dependency),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        fs::write(
            temporary.path().join("recipe.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "spec":spec,"requires":requires,
                "msvc_toolset":if tc.abi.compiler_family=="clang" {"ClangCL".to_owned()} else {format!("v{}",tc.abi.stdlib_release)}
            }))?,
        )?;
        fs::write(
            temporary.path().join("conanfile.py"),
            include_str!("native_recipe.py"),
        )?;
        let (package, channel) = reference
            .split_once('@')
            .context("Invalid source reference")?;
        let (package_name, version) = package.split_once('/').context("Invalid source version")?;
        let (user, channel) = channel.split_once('/').context("Invalid source channel")?;
        run(
            command(conan, home, path)
                .arg("export")
                .arg(temporary.path())
                .arg(format!("--name={package_name}"))
                .arg(format!("--version={version}"))
                .arg(format!("--user={user}"))
                .arg(format!("--channel={channel}")),
            log,
        )?;
    }
    Ok(())
}

pub fn verify_local_inputs(root: &Path, manifest: &Manifest) -> Result<()> {
    for dependency in manifest.dependencies.values() {
        if let Dependency::Detailed {
            source: Some(spec), ..
        } = dependency
            && let Input::Local { path, sha256 } | Input::Sdk { path, sha256, .. } = &spec.input
            && root.join(path).is_dir()
        {
            ensure!(
                snapshot_hash(&files(&root.join(path))?)?.eq_ignore_ascii_case(sha256),
                "Local source/SDK changed; update its declared hash explicitly"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_reconciliation_removes_deleted_inputs_but_preserves_generated_outputs() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("project");
        let target = root.path().join("build/source");
        let receipt = root.path().join("build/source-files.json");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(source.join("old.cpp"), "old input").unwrap();
        sync_project_copy(&source, &target, &receipt).unwrap();
        fs::write(target.join("generated.obj"), "build output").unwrap();
        fs::rename(source.join("old.cpp"), source.join("new.cpp")).unwrap();
        sync_project_copy(&source, &target, &receipt).unwrap();
        assert!(!target.join("old.cpp").exists());
        assert!(target.join("new.cpp").is_file());
        assert_eq!(
            fs::read_to_string(target.join("generated.obj")).unwrap(),
            "build output"
        );
        fs::write(target.join("new.cpp"), "build modified input").unwrap();
        fs::rename(source.join("new.cpp"), source.join("renamed.cpp")).unwrap();
        assert!(sync_project_copy(&source, &target, &receipt).is_err());
        assert_eq!(
            fs::read_to_string(target.join("new.cpp")).unwrap(),
            "build modified input"
        );
    }
    #[test]
    fn snapshot_identity_covers_file_names_and_content() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("header.h"), "one").unwrap();
        let before = snapshot_hash(&files(root.path()).unwrap()).unwrap();
        fs::rename(root.path().join("header.h"), root.path().join("other.h")).unwrap();
        assert_ne!(before, snapshot_hash(&files(root.path()).unwrap()).unwrap());
        fs::create_dir(root.path().join(".git")).unwrap();
        let saved = snapshot_hash(&files(root.path()).unwrap()).unwrap();
        fs::write(root.path().join(".git/config"), "ignored").unwrap();
        assert_eq!(saved, snapshot_hash(&files(root.path()).unwrap()).unwrap());
    }
}
