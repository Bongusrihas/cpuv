//! Verified development transfers contain project files, deployed libraries and pinned tools.
use super::*;

#[derive(Subcommand)]
pub enum Action {
    /// Export the current locked development environment to a new directory.
    Export {
        destination: PathBuf,
        /// Optional redistributable compiler tree; licensed SDK prerequisites remain external.
        #[arg(long)]
        toolchain_root: Option<PathBuf>,
    },
    /// Verify the entire transfer and display its external prerequisites.
    Inspect { directory: PathBuf },
    /// Restore into a new project and the selected CPUv cache without network access.
    Import {
        directory: PathBuf,
        destination: PathBuf,
    },
}

fn receipt(directory: &Path) -> Result<serde_json::Value> {
    cpuv_core::verify(directory).context("Offline transfer is incomplete or modified")?;
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("transfer.json"))?)?;
    ensure!(
        receipt["schema"] == 1,
        "Unsupported offline transfer schema"
    );
    Ok(receipt)
}

fn cache_copy(cache: &Path, relative: &Path, destination: &Path) -> Result<()> {
    cpuv_core::safe_relative(relative)?;
    let source = cache.join(relative);
    if source.is_dir() {
        cpuv_core::generations::copy_tree(&source, &destination.join(relative))?;
    } else {
        cpuv_core::regular(&source)?;
        fs::create_dir_all(
            destination
                .join(relative)
                .parent()
                .context("Missing cache parent")?,
        )?;
        fs::copy(source, destination.join(relative))?;
    }
    Ok(())
}

pub async fn execute(action: &Action, root: &Path, cli: &Cli) -> Result<i32> {
    match action {
        Action::Inspect { directory } => {
            println!("{}", serde_json::to_string_pretty(&receipt(directory)?)?)
        }
        Action::Export {
            destination,
            toolchain_root,
        } => {
            let _guard = cpuv_core::lease(root, false)?;
            ensure!(
                !destination.exists(),
                "Select a new offline export directory"
            );
            let manifest = read_manifest(root)?;
            maintenance::validate_request(
                root,
                &serde_json::from_slice(&fs::read(root.join(".cpuv/env/manifest.json"))?)?,
            )?;
            for (name, dependency) in &manifest.dependencies {
                if let Dependency::Detailed {
                    source: Some(spec), ..
                } = dependency
                {
                    ensure!(
                        !matches!(spec.input, source::native::Input::Sdk { .. })
                            || spec.redistributable,
                        "SDK {name} has not been declared redistributable; keep it as an external prerequisite in a separate profile"
                    );
                }
            }
            let destination_absolute = cpuv_core::resolved_destination(destination)?;
            ensure!(
                !destination_absolute.starts_with(profiles::project_root(root)?.canonicalize()?),
                "Export outside the project source tree"
            );
            let parent = destination
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            fs::create_dir_all(parent)?;
            let temp = tempfile::Builder::new()
                .prefix("cpuv-offline-")
                .tempdir_in(parent)?;
            let stage = temp.path();
            let tc: Toolchain =
                serde_json::from_slice(&fs::read(root.join(".cpuv/toolchain.json"))?)?;
            let payload = cpuv_core::locks::read(&root.join("cpuv.lock"))?;
            let cache = cache_dir(cli)?;
            artifacts::store(root, cli, &payload, &root.join(".cpuv/env"), &tc)?;
            cpuv_core::generations::copy_tree(
                &artifacts::location(
                    &cache,
                    &payload,
                    &request_hash(&manifest)?,
                    &cpuv_toolchain::identity(&tc)?.portable_digest()?,
                ),
                &stage.join("artifact"),
            )?;
            source::native::project_copy(&profiles::project_root(root)?, &stage.join("project"))?;
            // A selected profile carries its own manifest/lock, while source files belong to the project.
            for name in ["cpuv.toml", "cpuv.lock"] {
                fs::copy(root.join(name), stage.join("project").join(name))?;
            }
            fs::create_dir_all(stage.join("provider"))?;
            source::offline_transfer(root, cli, &stage.join("provider"), false).await?;
            let mut paths = Vec::new();
            let system = manifest.build.as_ref().map(|b| &b.system);
            for name in transfer_bootstrap(&manifest.backend, system) {
                paths.push(source::tools::selected(name, cli, &manifest.tools).await?);
            }
            for (name, artifact) in &manifest.tools {
                paths.push(source::managed::tool(name, artifact, cli).await?);
            }
            for executable in paths {
                let relative = executable.strip_prefix(&cache)?;
                let components: Vec<_> = relative.components().take(3).collect();
                let directory: PathBuf = components.iter().collect();
                if !stage.join("cache").join(&directory).exists() {
                    cache_copy(&cache, &directory, &stage.join("cache"))?;
                }
            }
            let metadata: cpuv_relocate::Environment =
                serde_json::from_slice(&fs::read(root.join(".cpuv/env/manifest.json"))?)?;
            for path in metadata.tool_identity.files.keys() {
                if let Ok(relative) = path.strip_prefix(&cache) {
                    cache_copy(&cache, relative, &stage.join("cache"))?;
                }
            }
            for dependency in manifest.dependencies.values() {
                if let Dependency::Detailed {
                    source: Some(spec), ..
                } = dependency
                {
                    let key = blake3::hash(&serde_json::to_vec(&spec.input)?)
                        .to_hex()
                        .to_string();
                    cache_copy(
                        &cache,
                        &PathBuf::from("catalog/source-inputs").join(key),
                        &stage.join("cache"),
                    )?;
                }
            }
            if manifest.backend == Backend::Auto {
                let locked: Lock = toml::from_str(&payload)?;
                for package in &locked.packages {
                    let relative =
                        PathBuf::from("archives").join(format!("{}.pkg.tar.zst", package.sha256));
                    cpuv_fetch::verify_file(&cache.join(&relative), &package.sha256, package.size)
                        .await
                        .with_context(|| {
                            format!(
                                "Offline export needs the verified archive for {} {}",
                                package.name, package.version
                            )
                        })?;
                    cache_copy(&cache, &relative, &stage.join("cache"))?;
                }
            }
            let bundled_compiler = if let Some(toolchain_root) = toolchain_root {
                let toolchain_root = cpuv_toolchain::driver_path(toolchain_root.canonicalize()?);
                ensure!(
                    !cpuv_toolchain::driver_path(stage.canonicalize()?)
                        .starts_with(&toolchain_root),
                    "Export outside --toolchain-root so the compiler snapshot cannot include its own output"
                );
                let relative = tc
                    .compiler_path
                    .strip_prefix(&toolchain_root)
                    .context("--toolchain-root must contain the selected compiler")?;
                ensure!(
                    tc.driver != "msvc",
                    "Microsoft SDK/compiler redistribution is not inferred; keep these prerequisites external"
                );
                let key = cpuv_toolchain::identity(&tc)?.portable_digest()?;
                let directory = PathBuf::from("toolchains/bundles")
                    .join(&key)
                    .join("prefix");
                cpuv_core::generations::copy_tree(
                    &toolchain_root,
                    &stage.join("cache").join(&directory),
                )?;
                Some(directory.join(relative))
            } else {
                None
            };
            cpuv_core::verify(&root.join(".cpuv/env"))?;
            metadata.tool_identity.verify()?;
            fs::write(
                stage.join("transfer.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "schema":1,"provider":manifest.backend,"compiler":tc.compiler_path,
                    "compiler_version":tc.gcc_version,"target":tc.abi.target_triple,"bundled_compiler":bundled_compiler,
                    "compiler_identity":cpuv_toolchain::identity(&tc)?.portable_digest()?,
                    "prerequisites":"Install a compiler and SDK with identical recorded contents; select it with --compiler. Licensed compilers/SDKs are not redistributed.",
                    "scope":"Locked dependency binaries, project sources, pinned build tools, source snapshots and Conan cache archive. Project builds and lock restoration work offline; rebuilding dependencies additionally requires their provider's source/build-tool closure."
                }))?,
            )?;
            cpuv_core::seal(stage)?;
            cpuv_core::verify(stage)?;
            fs::rename(stage, destination)?;
            println!("Verified development export: {}", destination.display());
        }
        Action::Import {
            directory,
            destination,
        } => {
            let details = receipt(directory)?;
            ensure!(
                cli.offline,
                "Use --offline for development import; no network access is required"
            );
            ensure!(
                !destination.exists(),
                "Import requires a new project directory"
            );
            let cache = cache_dir(cli)?;
            // Validate every collision before adding files to the cache.
            let inventory = cpuv_core::inventory(&directory.join("cache"))?;
            for (relative, digest) in &inventory {
                cpuv_core::contained(&cache, relative)?;
                let target = cache.join(relative);
                if target.exists() {
                    ensure!(
                        cpuv_core::digest(&target)? == *digest,
                        "Conflicting offline cache file: {}",
                        relative.display()
                    );
                }
            }
            for relative in inventory.keys() {
                let target = cache.join(relative);
                if !target.exists() {
                    fs::create_dir_all(target.parent().context("Missing cache destination")?)?;
                    let mut temporary = tempfile::NamedTempFile::new_in(target.parent().unwrap())?;
                    std::io::copy(
                        &mut File::open(directory.join("cache").join(relative))?,
                        &mut temporary,
                    )?;
                    temporary.persist_noclobber(&target).map_err(|e| e.error)?;
                }
            }
            let bundled = details["bundled_compiler"].as_str().map(PathBuf::from);
            if let Some(relative) = &bundled {
                cpuv_core::safe_relative(relative)?;
            }
            let bundled = bundled.map(|p| cache.join(p));
            let selected = cli.compiler.as_deref().or(bundled.as_deref());
            let tc = cpuv_toolchain::probe(selected)?;
            ensure!(
                details["compiler_identity"] == cpuv_toolchain::identity(&tc)?.portable_digest()?,
                "Compiler/SDK prerequisites do not match the offline transfer; inspect it and select matching --compiler"
            );
            let parent = destination
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            fs::create_dir_all(parent)?;
            let temporary = tempfile::Builder::new()
                .prefix("cpuv-import-")
                .tempdir_in(parent)?;
            cpuv_core::generations::copy_tree(&directory.join("project"), temporary.path())?;
            fs::rename(temporary.path(), destination)?;
            let imported = cpuv_toolchain::driver_path(destination.canonicalize()?);
            // Retain its tool/artifact references even if activation or provider restoration fails.
            cache::register(&cache, &imported)?;
            let result = async {
                fs::create_dir_all(imported.join(".cpuv"))?;
                let manifest = read_manifest(&imported)?;
                let payload = cpuv_core::locks::read(&imported.join("cpuv.lock"))?;
                let _lease = project_lock(&imported)?;
                let stage = artifacts::restore(
                    &imported,
                    cli,
                    &manifest,
                    &tc,
                    &payload,
                    &directory.join("artifact"),
                )?;
                artifacts::commit(&imported, stage)?;
                artifacts::store(&imported, cli, &payload, &imported.join(".cpuv/env"), &tc)?;
                Ok::<(), anyhow::Error>(())
            }
            .await;
            if let Err(error) = result {
                bail!(
                    "Import could not activate {}; files are preserved for diagnosis: {error:#}",
                    imported.display()
                );
            }
            source::offline_transfer(&imported, cli, &directory.join("provider"), true).await
                .with_context(|| format!("Project and environment restored at {}, but provider cache restoration failed; preserve the transfer's provider archive for repair", imported.display()))?;
            println!(
                "Offline development environment ready: {}",
                imported.display()
            );
        }
    }
    Ok(0)
}

fn transfer_bootstrap(
    backend: &Backend,
    system: Option<&source::native::System>,
) -> &'static [&'static str] {
    use source::native::System;
    if *backend == Backend::Conan {
        // Provider cache restoration constructs the build profiles even for Make/MSBuild consumers.
        &["cmake", "ninja", "conan"]
    } else {
        match system {
            None | Some(System::Cmake) => &["cmake", "ninja"],
            Some(System::Meson) => &["ninja"],
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transfers_include_the_provider_closure_not_only_project_build_tools() {
        assert_eq!(
            transfer_bootstrap(&Backend::Conan, Some(&source::native::System::Make)),
            &["cmake", "ninja", "conan"]
        );
        assert!(transfer_bootstrap(&Backend::Auto, Some(&source::native::System::Make)).is_empty());
        assert_eq!(
            transfer_bootstrap(&Backend::Vcpkg, None),
            &["cmake", "ninja"]
        );
    }
}
