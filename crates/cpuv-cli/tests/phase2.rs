//! Native source/provider lifecycle acceptance; no library-specific engine behavior.
#![cfg(windows)]
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

struct Server {
    url: String,
    routes: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let routes: Arc<Mutex<BTreeMap<String, Vec<u8>>>> = Default::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (data, finished) = (routes.clone(), stop.clone());
        let worker = thread::spawn(move || {
            while !finished.load(Ordering::SeqCst) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut buffer = [0u8; 8192];
                    let n = stream.read(&mut buffer).unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..n]);
                    let route = request.split_whitespace().nth(1).unwrap_or("/");
                    let body = data.lock().unwrap().get(route).cloned();
                    let status = if body.is_some() {
                        "200 OK"
                    } else {
                        "404 Not Found"
                    };
                    let body = body.unwrap_or_default();
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(&body);
                } else {
                    thread::sleep(Duration::from_millis(10));
                }
            }
        });
        Self {
            url,
            routes,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.url.trim_start_matches("http://"));
        self.worker.take().unwrap().join().unwrap();
    }
}

struct Fixture {
    root: tempfile::TempDir,
    app: PathBuf,
    binary: PathBuf,
    compiler: PathBuf,
}
impl Fixture {
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .arg("--project")
            .arg(&self.app)
            .args(args)
            .current_dir(&self.app);
        for name in ["CPPFLAGS", "CFLAGS", "CXXFLAGS", "LDFLAGS", "CPUV_PROFILE"] {
            command.env_remove(name);
        }
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        eprintln!("Phase 2: {args:?}");
        let output = self.command(args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
    fn state(&self) -> Vec<String> {
        ["cpuv.toml", "cpuv.lock", ".cpuv/env/files.json"]
            .iter()
            .map(|p| format!("{:x}", Sha256::digest(fs::read(self.app.join(p)).unwrap())))
            .collect()
    }
    fn spec(&self, input: serde_json::Value, system: &str) -> PathBuf {
        let path = self.root.path().join("package.toml");
        let value = json!({"version":"1.0","source":{"input":input,"build":{"system":system},
            "interface":{"libraries":["phase2_fixture"],"cmake_file":"phase2fixture","cmake_target":"phase2fixture::phase2fixture"}}});
        fs::write(&path, toml::to_string_pretty(&value).unwrap()).unwrap();
        path
    }
    fn add(&self, specification: &Path) {
        self.run(&[
            "source",
            "add",
            "source-fixture",
            specification.to_str().unwrap(),
        ]);
        self.run(&["verify"]);
        self.run(&["run", "g++", "main.cpp"]);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = Command::new(&self.binary)
            .args(["cache", "forget"])
            .arg(&self.app)
            .output();
    }
}

#[test]
#[ignore = "Native Phase 2 gate requires CPUV_TEST_CXX and verified bootstrap tools"]
fn source_inputs_locks_generations_and_native_adapters() {
    let root = tempfile::Builder::new()
        .prefix("cpuv phase2 ")
        .tempdir()
        .unwrap();
    let app = root.path().join("app");
    fs::create_dir_all(&app).unwrap();
    let f = Fixture {
        root,
        app,
        binary: PathBuf::from(env!("CARGO_BIN_EXE_cpuv")),
        compiler: std::env::var_os("CPUV_TEST_CXX").unwrap().into(),
    };
    let library = f.root.path().join("library");
    fs::create_dir(&library).unwrap();
    fs::write(library.join("CMakeLists.txt"),"cmake_minimum_required(VERSION 3.18)\nproject(fixture LANGUAGES CXX)\nadd_library(phase2_fixture value.cpp)\ninstall(TARGETS phase2_fixture ARCHIVE DESTINATION lib LIBRARY DESTINATION lib RUNTIME DESTINATION bin)\ninstall(FILES value.h DESTINATION include)\n").unwrap();
    fs::write(library.join("value.h"), "int phase2_value();\n").unwrap();
    fs::write(
        library.join("value.cpp"),
        "#include \"value.h\"\nint phase2_value(){return 73;}\n",
    )
    .unwrap();
    fs::write(
        f.app.join("main.cpp"),
        "#include <value.h>\nint main(){return phase2_value()==73?0:1;}\n",
    )
    .unwrap();
    f.run(&[
        "--compiler",
        f.compiler.to_str().unwrap(),
        "--provider",
        "conan",
        "init",
    ]);
    let inspect = f.run(&["source", "inspect", library.to_str().unwrap()]);
    let hash: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    let spec = f.spec(
        json!({"kind":"local","path":"../library","sha256":hash["sha256"]}),
        "cmake",
    );
    f.add(&spec);
    let lock: toml::Value =
        toml::from_str(&fs::read_to_string(f.app.join("cpuv.lock")).unwrap()).unwrap();
    assert_eq!(lock["version"].as_integer(), Some(5));
    let before = f.state();
    f.run(&["--offline", "sync"]);
    assert_eq!(before[..2], f.state()[..2]);
    f.run(&["verify"]);
    let before = f.state();
    fs::write(library.join("value.h"), "modified\n").unwrap();
    let output = f.command(&["--offline", "sync"]).output().unwrap();
    assert!(!output.status.success());
    assert_eq!(before, f.state());
    fs::write(library.join("value.h"), "int phase2_value();\n").unwrap();

    let server = Server::new();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for entry in fs::read_dir(&library).unwrap() {
        let entry = entry.unwrap();
        if !entry.path().is_file() {
            continue;
        }
        zip.start_file(
            format!("source/{}", entry.file_name().to_string_lossy()),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&fs::read(entry.path()).unwrap()).unwrap();
    }
    let bytes = zip.finish().unwrap().into_inner();
    let sha = format!("{:x}", Sha256::digest(&bytes));
    server
        .routes
        .lock()
        .unwrap()
        .insert("/source.zip".into(), bytes);
    let spec=f.spec(json!({"kind":"archive","url":format!("{}/source.zip",server.url),"sha256":sha,"format":"zip","strip_components":1}),"cmake");
    f.add(&spec);
    f.run(&["--offline", "sync"]);

    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&library)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    git(&["init"]);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-m",
        "source fixture",
    ]);
    let revision = String::from_utf8(git(&["rev-parse", "HEAD"]).stdout)
        .unwrap()
        .trim()
        .to_owned();
    let spec = f.spec(
        json!({"kind":"git","url":"../library","revision":revision}),
        "cmake",
    );
    f.add(&spec);
    f.run(&["--offline", "sync"]);

    let generation: serde_json::Value =
        serde_json::from_slice(&f.run(&["generation", "list"]).stdout).unwrap();
    let identity = generation.as_array().unwrap().last().unwrap()["identity"]
        .as_str()
        .unwrap()
        .to_owned();
    f.run(&["generation", "use", &identity]);
    f.run(&["verify"]);
    f.run(&["run", "g++", "main.cpp"]);
    assert!(
        fs::read_dir(f.app.join(".cpuv/lock-backups"))
            .unwrap()
            .next()
            .is_some()
    );

    fs::write(f.app.join("CMakeLists.txt"),"cmake_minimum_required(VERSION 3.18)\nproject(consumer LANGUAGES CXX)\nfind_package(phase2fixture CONFIG REQUIRED)\nadd_executable(consumer main.cpp)\ntarget_link_libraries(consumer PRIVATE phase2fixture::phase2fixture)\nenable_testing()\nadd_test(NAME consumer COMMAND consumer)\n").unwrap();
    f.run(&["test"]);
    fs::write(
        f.app.join("Makefile"),
        "all:\n\t$(CXX) main.cpp -o make-consumer.exe\ntest: all\n\t.\\make-consumer.exe\n",
    )
    .unwrap();
    let mut manifest: toml::Value =
        toml::from_str(&fs::read_to_string(f.app.join("cpuv.toml")).unwrap()).unwrap();
    manifest.as_table_mut().unwrap().insert(
        "build".into(),
        toml::Value::Table(toml::Table::from_iter([(
            "system".into(),
            toml::Value::String("make".into()),
        )])),
    );
    fs::write(
        f.app.join("cpuv.toml"),
        toml::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    f.run(&["--offline", "lock"]);
    f.run(&["test"]);
    assert!(!f.app.join("make-consumer.exe").exists());
    assert!(
        f.app
            .join(".cpuv/build/make/source/make-consumer.exe")
            .is_file()
    );
    assert!(fs::read_dir(f.app.join(".cpuv/logs")).unwrap().count() > 0);
    let root_state = f.state();
    f.run(&[
        "--compiler",
        f.compiler.to_str().unwrap(),
        "--configuration",
        "Debug",
        "profile",
        "create",
        "debug",
    ]);
    let profile_manifest = f.app.join(".cpuv/profiles/debug/cpuv.toml");
    let mut profile: toml::Value =
        toml::from_str(&fs::read_to_string(&profile_manifest).unwrap()).unwrap();
    assert_eq!(profile["backend"].as_str(), Some("conan"));
    profile["dependencies"]["source-fixture"]
        .as_table_mut()
        .unwrap()
        .insert("linkage".into(), "shared".into());
    fs::write(&profile_manifest, toml::to_string_pretty(&profile).unwrap()).unwrap();
    f.run(&["--profile", "debug", "lock"]);
    f.run(&["--profile", "debug", "run", "g++", "main.cpp"]);
    f.run(&["--profile", "debug", "--offline", "sync"]);
    f.run(&["--profile", "debug", "verify"]);
    assert_eq!(root_state, f.state());
    let export = f.root.path().join("offline-transfer");
    f.run(&["offline", "export", export.to_str().unwrap()]);
    f.run(&["offline", "inspect", export.to_str().unwrap()]);
    let imported = f.root.path().join("imported");
    let cache = f.root.path().join("fresh-cache");
    f.run(&[
        "--offline",
        "--cache-dir",
        cache.to_str().unwrap(),
        "offline",
        "import",
        export.to_str().unwrap(),
        imported.to_str().unwrap(),
    ]);
    let imported_command = |args: &[&str]| {
        eprintln!("Imported project: {args:?}");
        let output = Command::new(&f.binary)
            .arg("--project")
            .arg(&imported)
            .arg("--cache-dir")
            .arg(&cache)
            .arg("--offline")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    imported_command(&["verify"]);
    imported_command(&["sync"]);
    imported_command(&["test"]);
    fs::rename(
        imported.join(".cpuv/env"),
        imported.join(".cpuv/missing-env"),
    )
    .unwrap();
    imported_command(&["sync"]);
    imported_command(&["verify"]);
    imported_command(&["test"]);
    let prune: serde_json::Value =
        serde_json::from_slice(&imported_command(&["cache", "prune"]).stdout).unwrap();
    assert!(prune["selected"].as_array().unwrap().is_empty());
    // A CMake install which omits a private shared library must fail before commit.
    let broken = f.root.path().join("incomplete-runtime");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("CMakeLists.txt"),"cmake_minimum_required(VERSION 3.18)\nproject(runtime LANGUAGES CXX)\nadd_library(private_fixture SHARED private.cpp)\nadd_library(public_fixture SHARED public.cpp)\ntarget_link_libraries(public_fixture PRIVATE private_fixture)\ninstall(TARGETS public_fixture ARCHIVE DESTINATION lib RUNTIME DESTINATION bin)\n").unwrap();
    fs::write(
        broken.join("private.cpp"),
        "extern \"C\" __declspec(dllexport) int fixture_private(){return 1;}\n",
    )
    .unwrap();
    fs::write(broken.join("public.cpp"),"extern \"C\" __declspec(dllimport) int fixture_private(); extern \"C\" __declspec(dllexport) int fixture_public(){return fixture_private();}\n").unwrap();
    let inspect: serde_json::Value = serde_json::from_slice(
        &f.run(&["source", "inspect", broken.to_str().unwrap()])
            .stdout,
    )
    .unwrap();
    let declaration = json!({"version":"1.0","source":{"input":{"kind":"local","path":"../incomplete-runtime","sha256":inspect["sha256"]},"build":{"system":"cmake"},"interface":{"libraries":["public_fixture"]}}});
    let specification = f.root.path().join("broken.toml");
    fs::write(
        &specification,
        toml::to_string_pretty(&declaration).unwrap(),
    )
    .unwrap();
    let state = f.state();
    let result = f
        .command(&[
            "source",
            "add",
            "runtime-fixture",
            specification.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Incomplete runtime closure"));
    assert_eq!(state, f.state());

    let sdk = f.root.path().join("sdk");
    fs::create_dir_all(sdk.join("include")).unwrap();
    fs::write(sdk.join("include/fixture_sdk.h"), "#define FIXTURE_SDK 1\n").unwrap();
    let inspect: serde_json::Value =
        serde_json::from_slice(&f.run(&["source", "inspect", sdk.to_str().unwrap()]).stdout)
            .unwrap();
    let tc: serde_json::Value =
        serde_json::from_slice(&fs::read(f.app.join(".cpuv/toolchain.json")).unwrap()).unwrap();
    let declaration = json!({"version":"1.0","source":{"input":{"kind":"sdk","path":"../sdk","sha256":inspect["sha256"],"target":tc["abi"]["target_triple"],"runtime":tc["abi"]["crt_runtime"]},"build":{"system":"copy"}}});
    let specification = f.root.path().join("sdk.toml");
    fs::write(
        &specification,
        toml::to_string_pretty(&declaration).unwrap(),
    )
    .unwrap();
    f.run(&[
        "source",
        "add",
        "sdk-fixture",
        specification.to_str().unwrap(),
    ]);
    f.run(&["verify"]);
    let disallowed = f.root.path().join("restricted-sdk-export");
    let result = f
        .command(&["offline", "export", disallowed.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!disallowed.exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("redistributable"));

    let make_source = f.root.path().join("make-package");
    fs::create_dir_all(&make_source).unwrap();
    fs::write(
        make_source.join("value.cpp"),
        "int make_package_value(){return 7;}\n",
    )
    .unwrap();
    fs::write(
        make_source.join("Makefile"),
        "all:\n\t$(CXX) $(CXXFLAGS) -c value.cpp -o value.o\n\tar rcs libmake_fixture.a value.o\n",
    )
    .unwrap();
    let inspect: serde_json::Value = serde_json::from_slice(
        &f.run(&["source", "inspect", make_source.to_str().unwrap()])
            .stdout,
    )
    .unwrap();
    let declaration = json!({"version":"1.0","source":{"input":{"kind":"local","path":"../make-package","sha256":inspect["sha256"]},"build":{"system":"make","install":{"build/libmake_fixture.a":"lib/libmake_fixture.a"}},"interface":{"libraries":["make_fixture"]}}});
    let specification = f.root.path().join("make-package.toml");
    fs::write(
        &specification,
        toml::to_string_pretty(&declaration).unwrap(),
    )
    .unwrap();
    f.run(&[
        "source",
        "add",
        "make-fixture",
        specification.to_str().unwrap(),
    ]);
    f.run(&["verify"]);
    f.run(&["run", "g++", "main.cpp"]);

    // MSYS2 transfers preserve pinned archives and keep shared-cache quarantine active.
    let package_name = "mingw-w64-ucrt-x86_64-offline-fixture";
    let mut package = tar::Builder::new(Vec::new());
    let bytes = b"#define CPUV_OFFLINE_FIXTURE 83\n";
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    package
        .append_data(&mut header, "ucrt64/include/offline_fixture.h", &bytes[..])
        .unwrap();
    let archive = zstd::stream::encode_all(Cursor::new(package.into_inner().unwrap()), 1).unwrap();
    let sha = format!("{:x}", Sha256::digest(&archive));
    let filename = format!("{package_name}-1.0-1-any.pkg.tar.zst");
    let description = format!(
        "%NAME%\n{package_name}\n\n%VERSION%\n1.0-1\n\n%FILENAME%\n{filename}\n\n%SHA256SUM%\n{sha}\n\n%CSIZE%\n{}\n",
        archive.len()
    );
    let mut database = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(description.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    database
        .append_data(
            &mut header,
            format!("{package_name}-1.0-1/desc"),
            description.as_bytes(),
        )
        .unwrap();
    let mut compressed = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    compressed
        .write_all(&database.into_inner().unwrap())
        .unwrap();
    server
        .routes
        .lock()
        .unwrap()
        .insert("/ucrt64.db".into(), compressed.finish().unwrap());
    server
        .routes
        .lock()
        .unwrap()
        .insert(format!("/{filename}"), archive);
    let msys = f.root.path().join("msys-project");
    fs::create_dir_all(&msys).unwrap();
    fs::write(
        msys.join("main.cpp"),
        "#include <offline_fixture.h>\nint main(){return CPUV_OFFLINE_FIXTURE==83?0:1;}\n",
    )
    .unwrap();
    let native = |project: &Path, cache: &Path, args: &[&str]| {
        eprintln!("MSYS2 transfer: {args:?}");
        let mut command = Command::new(&f.binary);
        command
            .arg("--project")
            .arg(project)
            .arg("--cache-dir")
            .arg(cache)
            .arg("--compiler")
            .arg(&f.compiler)
            .arg("--repository")
            .arg(&server.url)
            .args(args);
        for variable in [
            "CPPFLAGS",
            "CFLAGS",
            "CXXFLAGS",
            "LDFLAGS",
            "CPUV_FROM_SOURCE",
        ] {
            command.env_remove(variable);
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{args:?}\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        result
    };
    native(&msys, &cache, &["init"]);
    native(&msys, &cache, &["add", "offline-fixture"]);
    native(&msys, &cache, &["run", "g++", "main.cpp"]);
    let transfer = f.root.path().join("msys-transfer");
    native(
        &msys,
        &cache,
        &["offline", "export", transfer.to_str().unwrap()],
    );
    let msys_imported = f.root.path().join("msys-imported");
    let msys_cache = f.root.path().join("msys-fresh-cache");
    native(
        &msys,
        &msys_cache,
        &[
            "--offline",
            "offline",
            "import",
            transfer.to_str().unwrap(),
            msys_imported.to_str().unwrap(),
        ],
    );
    native(&msys_imported, &msys_cache, &["--offline", "sync"]);
    native(&msys_imported, &msys_cache, &["run", "g++", "main.cpp"]);
    // Imported environments are independent copies, so damage the owned CAS blob explicitly.
    let digest = blake3::hash(bytes).to_hex().to_string();
    fs::write(
        msys_cache
            .join("store/blobs")
            .join(&digest[..2])
            .join(&digest),
        "damaged shared cache",
    )
    .unwrap();
    native(&msys_imported, &msys_cache, &["verify"]);
    fs::write(
        msys_imported.join(".cpuv/env/include/offline_fixture.h"),
        "damaged project copy",
    )
    .unwrap();
    native(&msys_imported, &msys_cache, &["--offline", "sync"]);
    native(&msys_imported, &msys_cache, &["verify"]);
    native(&msys_imported, &msys_cache, &["run", "g++", "main.cpp"]);
    assert!(
        fs::read_dir(msys_cache.join("store/quarantine"))
            .unwrap()
            .count()
            >= 2
    );

    fs::write(export.join("project/main.cpp"), "tampered").unwrap();
    let rejected = f.root.path().join("rejected");
    let output = f
        .command(&[
            "--offline",
            "offline",
            "import",
            export.to_str().unwrap(),
            rejected.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!rejected.exists());
    println!(
        "PASS: Phase 2 local/archive/Git sources, frozen restoration, generations, CMake/CTest, Make, offline transfer/repair and cache retention"
    );
}
