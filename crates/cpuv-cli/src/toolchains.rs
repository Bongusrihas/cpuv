//! Native GCC management through discovered MSYS2 installations.
use super::*;

#[derive(Subcommand)]
pub enum Action {
    /// List discovered GCC installations and their observed ABI/provider.
    List,
    /// Install GCC and its upstream dependencies using an existing MSYS2 host.
    Install {
        /// MSYS2 installation root; inferred only when exactly one is discovered.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Supported native GCC binary environment: ucrt64 or mingw64.
        #[arg(long, default_value = "ucrt64")]
        environment: String,
        /// Upstream package(s); defaults to this environment's GCC package.
        #[arg(long = "package")]
        packages: Vec<String>,
        /// Select the resulting g++ as cpuv's default compiler.
        #[arg(long = "use")]
        select: bool,
        /// Print the exact package transaction without changing files.
        #[arg(long)]
        dry_run: bool,
    },
    /// Select a verified compiler without changing Windows PATH.
    Use { compiler: PathBuf },
    /// Clear cpuv's selected compiler and resume environment/PATH discovery.
    Reset,
}

fn select(compiler: &Path) -> Result<()> {
    let tc = cpuv_toolchain::probe(Some(compiler))?;
    cpuv_toolchain::doctor(&tc, &[])?;
    let file = cpuv_toolchain::selection_file()?;
    fs::create_dir_all(file.parent().context("No toolchain storage parent")?)?;
    write_atomic(
        &file,
        &serde_json::to_vec_pretty(&serde_json::json!({"compiler":tc.compiler_path}))?,
    )?;
    println!(
        "Selected GCC {}: {} ({})",
        tc.gcc_version,
        tc.compiler_path.display(),
        tc.runtime_tag()
    );
    println!(
        "Explicit --compiler, CPUV_CXX, and CXX take precedence for new toolchain discovery. Existing projects retain their saved compiler; use --compiler with cpuv lock or create a separate profile to change it."
    );
    Ok(())
}

pub fn execute(action: &Action, cli: &Cli) -> Result<i32> {
    match action {
        Action::List => {
            let compilers = cpuv_toolchain::installed()?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"compilers":compilers,"msys2_roots":cpuv_toolchain::msys2_roots()})
                )?
            );
        }
        Action::Use { compiler } => select(compiler)?,
        Action::Reset => {
            let file = cpuv_toolchain::selection_file()?;
            match fs::remove_file(file) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
            println!("Cleared cpuv compiler selection; environment and PATH discovery are active.");
        }
        Action::Install {
            root,
            environment,
            packages,
            select: use_compiler,
            dry_run,
        } => {
            let provider = cpuv_toolchain::Provider::msys2(environment)?;
            let root = match root {
                Some(root) => root.clone(),
                None => match cpuv_toolchain::msys2_roots().as_slice() {
                    [root] => root.clone(),
                    [] => bail!(
                        "No MSYS2 installation discovered. Install MSYS2 first or pass --root to its installation directory; cpuv does not guess an installation drive."
                    ),
                    roots => bail!(
                        "Multiple MSYS2 installations discovered: {}. Select one with --root",
                        roots
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                },
            };
            let root = cpuv_toolchain::driver_path(
                root.canonicalize().context("MSYS2 root does not exist")?,
            );
            let pacman = root.join("usr/bin/pacman.exe");
            ensure!(
                pacman.is_file() && root.join("usr/bin/msys-2.0.dll").is_file(),
                "Selected root is not an MSYS2 host with pacman"
            );
            let names = if packages.is_empty() {
                vec![format!("{}gcc", provider.package_prefix)]
            } else {
                packages
                    .iter()
                    .map(|name| cpuv_solver::normalize_for(name, &provider.package_prefix))
                    .collect()
            };
            for name in &names {
                ensure!(
                    name.starts_with(&provider.package_prefix)
                        && name
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"-_.+".contains(&c)),
                    "Invalid package or a package from another environment: {name}"
                );
            }
            let compiler = root.join(environment).join("bin/g++.exe");
            println!(
                "MSYS2 root: {}\nEnvironment: {}\nPackages: {}",
                root.display(),
                environment,
                names.join(", ")
            );
            println!(
                "Command: {} -S --needed --noconfirm {}",
                pacman.display(),
                names.join(" ")
            );
            if *dry_run {
                return Ok(0);
            }
            let mut paths = vec![root.join("usr/bin"), root.join(environment).join("bin")];
            if let Some(value) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&value));
            }
            let path = std::env::join_paths(paths)?;
            let command = || {
                let mut value = Command::new(&pacman);
                value
                    .env("PATH", &path)
                    .env("MSYSTEM", environment.to_ascii_uppercase())
                    .current_dir(&root);
                value
            };
            let installed = command()
                .arg("-Q")
                .args(&names)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()?
                .success();
            if !installed {
                ensure!(
                    !cli.offline,
                    "Requested toolchain packages are missing; installation requires an online MSYS2 transaction"
                );
                ensure!(
                    command()
                        .args(["-S", "--needed", "--noconfirm"])
                        .args(&names)
                        .status()?
                        .success(),
                    "MSYS2 toolchain installation failed. Update the selected MSYS2 host with its normal full system upgrade before retrying; cpuv never performs a partial -Sy upgrade."
                );
            }
            ensure!(
                compiler.is_file(),
                "The requested packages did not provide {}; request the upstream GCC package for this environment",
                compiler.display()
            );
            let tc = cpuv_toolchain::probe(Some(&compiler))?;
            ensure!(
                tc.provider
                    .as_ref()
                    .is_some_and(|value| value.name == *environment),
                "Installed compiler does not match the requested binary environment"
            );
            cpuv_toolchain::doctor(&tc, &[])?;
            println!(
                "Ready: GCC {} at {}",
                tc.gcc_version,
                tc.compiler_path.display()
            );
            if *use_compiler {
                select(&compiler)?;
            }
        }
    }
    Ok(0)
}
