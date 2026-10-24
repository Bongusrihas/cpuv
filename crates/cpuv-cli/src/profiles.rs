//! Named, isolated project environments. Source files stay in the original project.
use super::*;

#[derive(Subcommand)]
pub enum Action {
    /// Create an isolated profile using --compiler and --configuration.
    Create { name: String },
    /// List named profiles and their selected compilers.
    List,
}
#[derive(Serialize, Deserialize)]
pub struct Settings {
    pub compiler: PathBuf,
    pub configuration: String,
}

pub fn directory(root: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty()
            && name.len() <= 64
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
        "Profile names use letters, digits, underscore and hyphen (maximum 64 characters)"
    );
    cpuv_store::safe_relative(Path::new(name))?;
    Ok(root.join(".cpuv/profiles").join(name))
}

pub fn project_root(root: &Path) -> Result<PathBuf> {
    if root.join("profile.json").is_file()
        && root.parent().is_some_and(|p| {
            p.file_name().is_some_and(|n| n == "profiles")
                && p.parent()
                    .is_some_and(|p| p.file_name().is_some_and(|n| n == ".cpuv"))
        })
    {
        ensure!(
            root.parent()
                .is_some_and(|p| p.file_name().is_some_and(|n| n == "profiles")),
            "Invalid profile directory"
        );
        Ok(root
            .ancestors()
            .nth(3)
            .context("Missing profile source project")?
            .to_owned())
    } else {
        Ok(root.to_owned())
    }
}

pub fn execute(action: &Action, root: &Path, cli: &Cli) -> Result<i32> {
    match action {
        Action::List => {
            let parent = root.join(".cpuv/profiles");
            let mut profiles = BTreeMap::new();
            if parent.is_dir() {
                for entry in fs::read_dir(parent)? {
                    let entry = entry?;
                    if entry.path().join("profile.json").is_file() {
                        let settings: Settings =
                            serde_json::from_slice(&fs::read(entry.path().join("profile.json"))?)?;
                        profiles.insert(entry.file_name().to_string_lossy().to_string(), settings);
                    }
                }
            }
            println!("{}", serde_json::to_string_pretty(&profiles)?);
        }
        Action::Create { name } => {
            let _guard = project_lock(root)?;
            let directory = directory(root, name)?;
            ensure!(!directory.exists(), "Profile {name} already exists");
            let tc = cpuv_toolchain::probe(cli.compiler.as_deref())?;
            fs::create_dir_all(directory.parent().unwrap())?;
            let stage = tempfile::Builder::new()
                .prefix("profile-")
                .tempdir_in(directory.parent().unwrap())?;
            fs::write(
                stage.path().join("profile.json"),
                serde_json::to_vec_pretty(&Settings {
                    compiler: tc.compiler_path.clone(),
                    configuration: cli
                        .configuration
                        .clone()
                        .unwrap_or_else(|| "Release".into()),
                })?,
            )?;
            init(stage.path(), cli)?;
            if root.join("cpuv.toml").is_file() {
                let original = read_manifest(root)?;
                let mut manifest = read_manifest(stage.path())?;
                manifest.project = original.project;
                manifest.project.configuration = cli
                    .configuration
                    .clone()
                    .unwrap_or_else(release_configuration);
                manifest.dependencies = original.dependencies;
                manifest.link = original.link;
                let env = stage.path().join(".cpuv/env");
                let mut metadata: cpuv_relocate::Environment =
                    serde_json::from_slice(&fs::read(env.join("manifest.json"))?)?;
                fs::write(env.join("request.toml"), toml::to_string_pretty(&manifest)?)?;
                metadata.request_hash = request_hash(&manifest)?;
                metadata.standard = manifest.project.std.clone();
                metadata.configuration = manifest.project.configuration.clone();
                fs::write(
                    env.join("manifest.json"),
                    serde_json::to_vec_pretty(&metadata)?,
                )?;
                cpuv_core::seal(&env)?;
                write_atomic(
                    &stage.path().join("cpuv.toml"),
                    toml::to_string_pretty(&manifest)?.as_bytes(),
                )?;
            }
            // Publish only after activation points to the final profile path.
            write_atomic(
                &stage.path().join(".cpuv/activate.ps1"),
                activation(&directory, &tc)?.as_bytes(),
            )?;
            fs::rename(stage.path(), &directory)?;
            println!(
                "Created profile {name}. Run cpuv --profile {name} lock to resolve its dependencies."
            );
        }
    }
    Ok(0)
}
