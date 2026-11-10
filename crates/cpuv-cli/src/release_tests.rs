use super::*;

#[test]
fn dependency_edits_preserve_options_linkage_and_private_sources() {
    let dependency: Dependency =
        toml::from_str("version='1'\nlinkage='static'\n[options]\nfeature='True'\n").unwrap();
    let edited = dependency.edited(">=", "2", None).unwrap();
    let encoded = toml::to_string(&edited).unwrap();
    assert!(encoded.contains("static") && encoded.contains("True") && encoded.contains(">=2"));
    let reference = edited.edited("", "", Some("foo/3@team/stable")).unwrap();
    assert!(reference.edited("=", "4", None).is_err());
    assert!(reference == reference.edited("", "", None).unwrap());
    let replaced = reference.edited("", "", Some("foo/4@team/stable")).unwrap();
    let encoded = toml::to_string(&replaced).unwrap();
    assert!(
        encoded.contains("foo/4@team/stable")
            && encoded.contains("static")
            && encoded.contains("True")
    );
}

#[test]
fn runtime_paths_and_placeholders_use_original_source_root() {
    let root = tempfile::tempdir().unwrap();
    let plan = runtime::Plan {
        search_paths: vec!["plugins".into()],
        environment: BTreeMap::from([
            ("DATA".into(), "${root}/data".into()),
            ("PATH".into(), "${path}".into()),
        ]),
        ..Default::default()
    };
    let base = std::env::join_paths([root.path().join("compiler")]).unwrap();
    let path = plan.path(root.path(), &base).unwrap();
    let paths: Vec<_> = std::env::split_paths(&path).collect();
    assert_eq!(
        paths,
        [root.path().join("plugins"), root.path().join("compiler")]
    );
    let variables = plan.variables(root.path(), &path).unwrap();
    assert_eq!(variables["PATH"], path.to_string_lossy());
    assert_eq!(variables["DATA"], format!("{}/data", root.path().display()));
}

#[test]
fn adapter_arguments_cannot_replace_profile_owned_settings() {
    use source::native::{Build, System, validate_build};
    for (system, argument) in [
        (System::Cmake, "-DCMAKE_BUILD_TYPE=Debug"),
        (System::Cmake, "-DCMAKE_CXX_FLAGS_RELEASE=/MT"),
        (System::Cmake, "-D"),
        (System::Make, "CXX=another-compiler"),
        (System::Msbuild, "/P:CLToolExe=another.exe"),
        (System::Msbuild, "/property:Configuration=Debug"),
    ] {
        let mut build = Build {
            system,
            ..Default::default()
        };
        build.arguments = if argument == "-D" {
            vec![argument.into(), "CMAKE_CXX_STANDARD=11".into()]
        } else {
            vec![argument.into()]
        };
        assert!(validate_build(&build).is_err(), "accepted {argument}");
    }
    let build = Build {
        arguments: vec!["-DENABLE_EXAMPLES=ON".into()],
        ..Default::default()
    };
    validate_build(&build).unwrap();
}

#[test]
fn no_op_source_sync_preserves_input_timestamp_and_repairs_changed_copy() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("copied");
    let receipt = root.path().join("receipt.json");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("a.cpp"), "input").unwrap();
    source::native::sync_project_copy(&source, &target, &receipt).unwrap();
    let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000000);
    File::options()
        .write(true)
        .open(target.join("a.cpp"))
        .unwrap()
        .set_modified(old)
        .unwrap();
    source::native::sync_project_copy(&source, &target, &receipt).unwrap();
    assert_eq!(
        fs::metadata(target.join("a.cpp"))
            .unwrap()
            .modified()
            .unwrap(),
        old
    );
    fs::write(target.join("a.cpp"), "build modified input").unwrap();
    source::native::sync_project_copy(&source, &target, &receipt).unwrap();
    assert_eq!(fs::read(target.join("a.cpp")).unwrap(), b"input");
}

#[test]
fn artifact_identity_ignores_provider_payload_formatting() {
    let first = "version=1\npackages=[]\n[dependencies]\n";
    let second = "packages = []\nversion = 1\n[dependencies]\n";
    assert_eq!(artifacts::key(first), artifacts::key(second));
    assert_eq!(
        artifacts::location(Path::new("cache"), first, "request", "compiler"),
        artifacts::location(Path::new("cache"), second, "request", "compiler")
    );
}

#[test]
fn cache_inspection_allows_an_existing_reader() {
    let root = tempfile::tempdir().unwrap();
    let _reader = cache::lease(root.path(), false).unwrap();
    assert_eq!(
        cache::execute(&cache::Action::Inspect, root.path()).unwrap(),
        0
    );
    assert!(cache::lease(root.path(), true).is_err());
}
