//! Read-only diagnostics for the selected environment and its locked inputs.
use super::*;

pub fn verify(root: &Path) -> Result<i32> {
    let _guard = cpuv_core::lease(root, false)?;
    let env = root.join(".cpuv/env");
    cpuv_core::verify(&env)?;
    let metadata: cpuv_relocate::Environment =
        serde_json::from_slice(&fs::read(env.join("manifest.json"))?)?;
    metadata.tool_identity.verify()?;
    validate_request(root, &metadata)?;
    println!("Verified environment contents, compiler/SDK contents, and project request.");
    Ok(0)
}

pub fn validate_request(root: &Path, metadata: &cpuv_relocate::Environment) -> Result<()> {
    let manifest = read_manifest(root)?;
    ensure!(
        metadata.request_hash == request_hash(&manifest)?,
        "Project request changed or metadata needs migration; run cpuv lock to regenerate the environment"
    );
    ensure!(
        manifest.project.std == metadata.standard
            && manifest.project.configuration == metadata.configuration,
        "Project standard/configuration changed; run cpuv lock to update this environment"
    );
    if root.join("cpuv.lock").is_file() {
        #[derive(Deserialize)]
        struct Request {
            dependencies: BTreeMap<String, Dependency>,
        }
        let lock: Request = toml::from_str(&fs::read_to_string(root.join("cpuv.lock"))?)?;
        ensure!(
            lock.dependencies == manifest.dependencies,
            "Dependencies changed; run cpuv lock to update this environment"
        );
    } else {
        ensure!(
            manifest.dependencies.is_empty(),
            "Missing dependency lock; run cpuv lock first"
        );
    }
    Ok(())
}

pub fn plan(root: &Path) -> Result<i32> {
    let _guard = cpuv_core::lease(root, false)?;
    let manifest = read_manifest(root)?;
    let tc: Toolchain = serde_json::from_slice(&fs::read(root.join(".cpuv/toolchain.json"))?)?;
    let env = root.join(".cpuv/env");
    let metadata: cpuv_relocate::Environment =
        serde_json::from_slice(&fs::read(env.join("manifest.json"))?)?;
    let request = validate_request(root, &metadata)
        .err()
        .map(|e| format!("{e:#}"));
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema":1, "project":profiles::project_root(root)?, "environment":env,
            "compiler":tc.compiler_path, "linker":tc.linker, "target":tc.abi.target_triple,
            "driver":tc.driver, "configuration":manifest.project.configuration,
            "standard":manifest.project.std, "provider":manifest.backend,
            "requested_dependencies":manifest.dependencies, "request_error":request,
            "ordered_link_items":metadata.link_items, "rescan_group":metadata.link_group,
            "environment_files":cpuv_core::verify(&env).map_or_else(|e| format!("{e:#}"), |_| "verified".into()),
            "toolchain_files":metadata.tool_identity.verify().map_or_else(|e| format!("{e:#}"), |_| "verified".into()),
            "note":"Installed plan only; this command does not resolve, download, or modify dependencies."
        }))?
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_link_selection_changes_the_request_identity() {
        let mut manifest = Manifest {
            backend: Backend::Auto,
            project: Project {
                name: "test".into(),
                version: "1".into(),
                std: "c++20".into(),
                configuration: "Release".into(),
            },
            toolchain: PinnedToolchain {
                target: "test".into(),
                compiler: "test".into(),
                abi: "test".into(),
            },
            dependencies: BTreeMap::new(),
            link: Default::default(),
            vcpkg: None,
        };
        let previous = request_hash(&manifest).unwrap();
        manifest.link.libraries.push("plugin".into());
        assert_ne!(previous, request_hash(&manifest).unwrap());
    }
    #[test]
    fn unknown_dependency_fields_are_reported_instead_of_ignored() {
        let result =
            toml::from_str::<BTreeMap<String, Dependency>>("fmt = { refernece = \"fmt/11.2.0\" }");
        assert!(result.is_err());
    }
}
