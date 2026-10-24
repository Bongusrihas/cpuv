//! Native Phase 1 release gate. All packages and HTTP traffic are local fixtures.
#![cfg(windows)]
use cpuv_solver::{PREFIX, Package};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

type Routes = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;
struct Repository {
    routes: Routes,
    url: String,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Repository {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let routes: Routes = Default::default();
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (data, count, finished) = (routes.clone(), requests.clone(), stop.clone());
        let worker = thread::spawn(move || {
            while !finished.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(error) => panic!("HTTP fixture: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut chunk = [0; 1024];
                while request.len() < 8192 && !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&chunk[..n]),
                    }
                }
                let request = String::from_utf8_lossy(&request);
                let Some(path) = request.split_whitespace().nth(1) else {
                    continue;
                };
                count.fetch_add(1, Ordering::SeqCst);
                let body = data.lock().unwrap().get(path).cloned();
                let (status, body) = body.map_or(("404 Not Found", Vec::new()), |b| ("200 OK", b));
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            }
        });
        Self {
            routes,
            url,
            requests,
            stop,
            worker: Some(worker),
        }
    }
    fn snapshot(&self, packages: &[(Package, Vec<u8>)]) {
        let mut db = tar::Builder::new(Vec::new());
        let mut routes = self.routes.lock().unwrap();
        for (p, payload) in packages {
            let desc = format!(
                "%NAME%\n{}\n\n%VERSION%\n{}\n\n%FILENAME%\n{}\n\n%CSIZE%\n{}\n\n%SHA256SUM%\n{}\n\n%DEPENDS%\n{}\n\n%PROVIDES%\n{}\n\n%CONFLICTS%\n{}\n",
                p.name,
                p.version,
                p.filename,
                p.size,
                p.sha256,
                p.depends.join("\n"),
                p.provides.join("\n"),
                p.conflicts.join("\n")
            );
            append(
                &mut db,
                &format!("{}-{}/desc", p.name, p.version),
                desc.as_bytes(),
            );
            routes.insert(format!("/{}", p.filename), payload.clone());
        }
        routes.insert(
            "/ucrt64.db".into(),
            zstd::stream::encode_all(&db.into_inner().unwrap()[..], 1).unwrap(),
        );
    }
}
impl Drop for Repository {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.url.trim_start_matches("http://"));
        self.worker.take().unwrap().join().unwrap();
    }
}
fn append(tar: &mut tar::Builder<Vec<u8>>, name: &str, bytes: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, name, bytes).unwrap();
}
fn package(
    name: &str,
    version: &str,
    files: Vec<(&str, Vec<u8>)>,
    deps: &[&str],
    provides: &[&str],
) -> (Package, Vec<u8>) {
    let mut archive = tar::Builder::new(Vec::new());
    for (path, bytes) in files {
        append(&mut archive, &format!("ucrt64/{path}"), &bytes);
    }
    let payload = zstd::stream::encode_all(&archive.into_inner().unwrap()[..], 1).unwrap();
    (
        Package {
            name: format!("{PREFIX}{name}"),
            version: version.into(),
            filename: format!("{PREFIX}{name}-{version}-any.pkg.tar.zst"),
            sha256: format!("{:x}", Sha256::digest(&payload)),
            size: payload.len() as u64,
            depends: deps.iter().map(|n| format!("{PREFIX}{n}")).collect(),
            provides: provides.iter().map(|n| format!("{PREFIX}{n}")).collect(),
            conflicts: vec![],
        },
        payload,
    )
}
fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
fn succeed(command: &mut Command) -> Output {
    let output = command.output().unwrap();
    assert!(output.status.success(), "{command:?}\n{}", text(&output));
    output
}
struct ReaderProcess {
    child: Child,
    release: PathBuf,
}
impl Drop for ReaderProcess {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, "release");
        let started = Instant::now();
        while self.child.try_wait().ok().flatten().is_none()
            && started.elapsed() < Duration::from_secs(10)
        {
            thread::sleep(Duration::from_millis(50));
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

struct Fixture {
    temp: tempfile::TempDir,
    project: PathBuf,
    cache: PathBuf,
    compiler: PathBuf,
    repo: Repository,
}
impl Fixture {
    fn new() -> Self {
        let compiler = cpuv_toolchain::probe(Some(Path::new(
            &std::env::var_os("CPUV_TEST_CXX")
                .expect("Set CPUV_TEST_CXX to an installed UCRT64 g++.exe"),
        )))
        .unwrap();
        assert_eq!(
            compiler.provider.as_ref().map(|p| p.name.as_str()),
            Some("ucrt64")
        );
        let temp = tempfile::Builder::new()
            .prefix("cpuv-phase1-")
            .tempdir()
            .unwrap();
        let project = temp.path().join("project with spaces");
        fs::create_dir(&project).unwrap();
        let cache = temp.path().join("cache");
        Self {
            temp,
            project,
            cache,
            compiler: compiler.compiler_path,
            repo: Repository::new(),
        }
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cpuv"));
        command
            .arg("--project")
            .arg(&self.project)
            .arg("--compiler")
            .arg(&self.compiler)
            .arg("--cache-dir")
            .arg(&self.cache)
            .arg("--repository")
            .arg(&self.repo.url)
            .args(args)
            .env("NO_PROXY", "127.0.0.1,localhost");
        for variable in [
            "CPPFLAGS",
            "CFLAGS",
            "CXXFLAGS",
            "LDFLAGS",
            "CPUV_FROM_SOURCE",
        ] {
            command.env_remove(variable);
        }
        command
    }
    fn ok(&self, args: &[&str]) -> Output {
        succeed(&mut self.command(args))
    }
    fn fails(&self, args: &[&str], message: &str) {
        let output = self.command(args).output().unwrap();
        assert!(!output.status.success(), "Unexpected success: {args:?}");
        assert!(
            text(&output).contains(message),
            "{args:?}: {}",
            text(&output)
        );
    }
    fn native_command(&self, executable: &Path) -> Command {
        let mut command = Command::new(executable);
        let mut paths = vec![self.compiler.parent().unwrap().to_owned()];
        if let Some(path) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&path));
        }
        command.env("PATH", std::env::join_paths(paths).unwrap());
        command
    }
    fn raw(&self, args: &[&str]) {
        let mut command = self.native_command(&self.compiler);
        command.current_dir(self.temp.path()).args(args);
        succeed(&mut command);
    }
    fn object(&self, name: &str, code: &str) -> PathBuf {
        let source = self.temp.path().join(format!("{name}.cpp"));
        let object = self.temp.path().join(format!("{name}.o"));
        fs::write(&source, code).unwrap();
        self.raw(&[
            "-c",
            source.to_str().unwrap(),
            "-o",
            object.to_str().unwrap(),
        ]);
        object
    }
    fn archive(&self, name: &str, members: &[PathBuf]) -> Vec<u8> {
        let path = self.temp.path().join(format!("lib{name}.a"));
        let mut command = self.native_command(&self.compiler.with_file_name("ar.exe"));
        command.arg("rcs").arg(&path).args(members);
        succeed(&mut command);
        fs::read(path).unwrap()
    }
    fn state(&self) -> (Vec<u8>, Vec<u8>, cpuv_core::Inventory) {
        (
            fs::read(self.project.join("cpuv.toml")).unwrap(),
            fs::read(self.project.join("cpuv.lock")).unwrap(),
            cpuv_core::inventory(&self.project.join(".cpuv/env")).unwrap(),
        )
    }
}

#[test]
#[ignore = "Native Windows release gate; requires CPUV_TEST_CXX pointing to UCRT64 GCC"]
fn phase1_local_repository_native_consumers_and_recovery() {
    let f = Fixture::new();
    println!("Phase 1 fixture: {}", f.temp.path().display());
    let c1 = f.object("c1", "extern \"C\" int c_one(){return 3;}");
    let c2 = f.object("c2", "extern \"C\" int c_two(){return 5;}");
    let a = f.object(
        "a",
        "extern \"C\" int c_one(); extern \"C\" int a(){return c_one();}",
    );
    let b = f.object(
        "b",
        "extern \"C\" int c_two(); extern \"C\" int b(){return c_two();}",
    );
    let plugin = f.object(
        "plugin",
        "extern int registered; struct R{R(){registered=7;}} registration;",
    );
    let runtime = f.temp.path().join("runtime.cpp");
    let dll = f.temp.path().join("RUNTIME.DLL");
    let import = f.temp.path().join("libruntime.dll.a");
    fs::write(
        &runtime,
        "extern \"C\" __declspec(dllexport) int runtime_value(){return 11;}",
    )
    .unwrap();
    f.raw(&[
        "-shared",
        runtime.to_str().unwrap(),
        "-o",
        dll.to_str().unwrap(),
        &format!("-Wl,--out-implib,{}", import.display()),
    ]);
    let pc = |name: &str, libs: &str, requires: &str| {
        format!("prefix=/ucrt64\nlibdir=${{prefix}}/lib\nName: {name}\nDescription: Native phase one fixture\nVersion: 1.0\nLibs: -L${{libdir}} {libs}\nRequires: {requires}\n").into_bytes()
    };
    let implementation = |version: &str| {
        package("implementation",version,vec![
        ("include/fixture.hpp",b"#if !defined(FOO) || !defined(BAR)\n#error missing expanded flags\n#endif\nextern \"C\" int a();extern \"C\" int b();extern \"C\" int runtime_value();\n".to_vec()),
        ("lib/pkgconfig/api.PC",b"prefix=/ucrt64\nflags=-DFOO -DBAR\nName: api\nDescription: Native phase one fixture\nVersion: 1.0\nCflags: -I${prefix}/include ${flags}\nRequires: a, b, runtime\nLibs: -L${prefix}/lib -Wl,--whole-archive -lplugin -Wl,--no-whole-archive\n".to_vec())], &["a","b","runtime","plugin"], &["virtual=1.0"])
    };
    let mut packages = vec![
        implementation("1.0-1"),
        package(
            "a",
            "1.0-1",
            vec![
                ("lib/liba.a", f.archive("a", &[a])),
                ("lib/pkgconfig/a.pc", pc("a", "-la", "c")),
            ],
            &["c"],
            &[],
        ),
        package(
            "b",
            "1.0-1",
            vec![
                ("lib/libb.a", f.archive("b", &[b])),
                ("lib/pkgconfig/b.pc", pc("b", "-lb", "c")),
            ],
            &["c"],
            &[],
        ),
        package(
            "c",
            "1.0-1",
            vec![
                ("lib/libc.a", f.archive("c", &[c1, c2])),
                ("lib/pkgconfig/c.pc", pc("c", "-lc", "")),
            ],
            &[],
            &[],
        ),
        package(
            "plugin",
            "1.0-1",
            vec![("lib/libplugin.a", f.archive("plugin", &[plugin]))],
            &[],
            &[],
        ),
        package(
            "runtime",
            "1.0-1",
            vec![
                ("bin/RUNTIME.DLL", fs::read(&dll).unwrap()),
                ("lib/libruntime.dll.a", fs::read(import).unwrap()),
                ("lib/pkgconfig/runtime.pc", pc("runtime", "-lruntime", "")),
            ],
            &[],
            &[],
        ),
    ];
    f.repo.snapshot(&packages);
    println!("Checking frozen contracts and native virtual/diamond/whole-archive consumers");
    f.ok(&["init"]);
    f.fails(&["--offline", "sync"], "No cpuv.lock");
    f.ok(&["add", "virtual>=1"]);
    fs::write(f.project.join("main.cpp"),"#include <fixture.hpp>\nint registered=0;int main(){return a()==3&&b()==5&&runtime_value()==11&&registered==7?0:1;}\n").unwrap();
    f.ok(&["run", "g++", "main.cpp"]);
    f.ok(&["verify"]);
    let x1 = f.object(
        "cycle_entry",
        "extern \"C\" int cycle_two();extern \"C\" int cycle_one(){return cycle_two();}",
    );
    let x2 = f.object("cycle_tail", "extern \"C\" int cycle_tail(){return 13;}");
    let y = f.object(
        "cycle_two",
        "extern \"C\" int cycle_tail();extern \"C\" int cycle_two(){return cycle_tail();}",
    );
    packages.extend([
        package(
            "cycle-x",
            "1.0-1",
            vec![
                ("lib/libcycle_x.a", f.archive("cycle_x", &[x1, x2])),
                (
                    "lib/pkgconfig/cycle-x.pc",
                    pc("cycle-x", "-lcycle_x", "cycle-y"),
                ),
            ],
            &["cycle-y"],
            &[],
        ),
        package(
            "cycle-y",
            "1.0-1",
            vec![
                ("lib/libcycle_y.a", f.archive("cycle_y", &[y])),
                (
                    "lib/pkgconfig/cycle-y.pc",
                    pc("cycle-y", "-lcycle_y", "cycle-x"),
                ),
            ],
            &["cycle-x"],
            &[],
        ),
    ]);
    f.repo.snapshot(&packages);
    f.ok(&["--refresh", "add", "cycle-x"]);
    fs::write(
        f.project.join("cycle.cpp"),
        "extern \"C\" int cycle_one();int registered=0;int main(){return cycle_one()==13?0:1;}",
    )
    .unwrap();
    f.ok(&["run", "g++", "cycle.cpp"]);
    let pkgconf = ["pkgconf.exe", "pkg-config.exe"]
        .into_iter()
        .map(|name| f.compiler.with_file_name(name))
        .find(|p| p.is_file())
        .expect("Install matching pkgconf for the reference metadata check");
    let reference = succeed(
        Command::new(pkgconf)
            .args(["--cflags", "api"])
            .env(
                "PKG_CONFIG_LIBDIR",
                f.project.join(".cpuv/env/lib/pkgconfig"),
            )
            .env_remove("PKG_CONFIG_PATH"),
    );
    let reference = shlex::split(std::str::from_utf8(&reference.stdout).unwrap()).unwrap();
    let env = f.project.join(".cpuv/env");
    let metadata: cpuv_relocate::Environment =
        serde_json::from_slice(&fs::read(env.join("manifest.json")).unwrap()).unwrap();
    let actual = cpuv_relocate::compiler_args(&metadata, &env, &["main.cpp".into()]).unwrap();
    for flag in reference {
        if let Some(include) = flag.strip_prefix("-I") {
            let expected = Path::new(include).canonicalize().unwrap();
            assert!(
                actual
                    .windows(2)
                    .any(|pair| (pair[0] == "-isystem" || pair[0] == "-I")
                        && Path::new(&pair[1])
                            .canonicalize()
                            .is_ok_and(|p| p == expected)),
                "Reference include {include:?} missing from {actual:?}"
            );
        } else {
            assert!(
                actual.contains(&flag.clone().into()),
                "Reference flag {flag:?} missing from {actual:?}"
            );
        }
    }
    // The same resolved graph with a different request must regenerate receipts.
    f.ok(&["add", "a"]);
    f.ok(&["verify"]);
    let initial = f.state();
    let requests = f.repo.requests.load(Ordering::SeqCst);
    f.ok(&["--refresh", "sync"]);
    assert_eq!(requests, f.repo.requests.load(Ordering::SeqCst));
    assert_eq!(initial, f.state());
    println!("Checking stdout, case collisions and standalone shim DLL deployment");
    let stdout = f.ok(&["run", "g++", "-E", "main.cpp", "-o", "-"]);
    assert!(String::from_utf8_lossy(&stdout.stdout).contains("registered"));
    let dependencies = f.ok(&["run", "g++", "-M", "main.cpp", "-MF", "-"]);
    assert!(String::from_utf8_lossy(&dependencies.stdout).contains("fixture.hpp"));
    assert!(!f.project.join(".cpuv/build/-").exists());
    let before = cpuv_core::inventory(&f.project.join(".cpuv/build")).unwrap();
    f.fails(
        &["run", "g++", "-c", "left/A.cpp", "right/a.cpp"],
        "basename",
    );
    assert_eq!(
        before,
        cpuv_core::inventory(&f.project.join(".cpuv/build")).unwrap()
    );
    f.fails(
        &["run", "g++", "-c", "left/Ä.cpp", "right/ä.cpp"],
        "basename",
    );

    let standalone = f.project.join("standalone");
    fs::create_dir(&standalone).unwrap();
    let executable = standalone.join("consumer.exe");
    let shim = f.project.join(".cpuv/bin/g++.exe");
    succeed(
        Command::new(&shim)
            .current_dir(&f.project)
            .args(["main.cpp", "-dumpdir", "standalone/", "-o"])
            .arg(&executable),
    );
    assert!(standalone.join("RUNTIME.DLL").is_file());
    succeed(Command::new(&executable).current_dir(&standalone).env(
        "PATH",
        PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32"),
    ));
    let files = cpuv_core::inventory(&standalone).unwrap();
    succeed(
        Command::new(&shim)
            .current_dir(&standalone)
            .arg("-dumpmachine"),
    );
    assert_eq!(files, cpuv_core::inventory(&standalone).unwrap());
    println!("Checking metadata, DLL, copied-header and CAS corruption recovery offline");
    for file in [
        "manifest.json",
        "link-plan.json",
        "cpuv.specs",
        "bin/RUNTIME.DLL",
    ] {
        let file = f.project.join(".cpuv/env").join(file);
        fs::remove_file(&file).unwrap();
        fs::write(file, "corrupt generated/copied file").unwrap();
        f.ok(&["--offline", "sync"]);
        f.ok(&["verify"]);
    }
    // Deliberately edit a hardlink, damaging the shared blob as well as payload.
    fs::write(
        f.project.join(".cpuv/env/include/fixture.hpp"),
        "damaged shared header",
    )
    .unwrap();
    f.fails(&["run", "g++", "main.cpp"], "Environment files changed");
    f.ok(&["--offline", "sync"]);
    f.ok(&["run", "g++", "main.cpp"]);
    assert!(
        fs::read_dir(f.cache.join("store/quarantine"))
            .unwrap()
            .count()
            >= 2
    );
    println!("Checking download failure and changed-manifest rollback");
    let original = f.state();
    let broken = package(
        "broken",
        "1.0-1",
        vec![("include/broken.h", b"valid archive".to_vec())],
        &[],
        &[],
    );
    packages.push(broken.clone());
    f.repo.snapshot(&packages);
    f.repo
        .routes
        .lock()
        .unwrap()
        .insert(format!("/{}", broken.0.filename), vec![0; broken.1.len()]);
    f.fails(&["--refresh", "add", "broken"], "SHA-256 mismatch");
    assert_eq!(original, f.state());
    let mut changed: toml::Value =
        toml::from_str(std::str::from_utf8(&original.0).unwrap()).unwrap();
    changed["dependencies"]
        .as_table_mut()
        .unwrap()
        .insert("unlocked".into(), "*".into());
    fs::write(
        f.project.join("cpuv.toml"),
        toml::to_string(&changed).unwrap(),
    )
    .unwrap();
    f.fails(&["--offline", "sync"], "Lock disagrees");
    fs::write(f.project.join("cpuv.toml"), &original.0).unwrap();
    f.fails(&["remove", "not-a-dependency"], "not a direct dependency");
    assert_eq!(original, f.state());
    let mut corrupted_lock: toml::Value =
        toml::from_str(std::str::from_utf8(&original.1).unwrap()).unwrap();
    let entries = corrupted_lock["packages"].as_array_mut().unwrap();
    entries.push(entries[0].clone());
    fs::write(
        f.project.join("cpuv.lock"),
        toml::to_string(&corrupted_lock).unwrap(),
    )
    .unwrap();
    f.fails(&["--offline", "sync"], "duplicate or unreachable");
    fs::write(f.project.join("cpuv.lock"), &original.1).unwrap();
    let mut conflict = package(
        "conflict",
        "1.0-1",
        vec![("include/conflict.h", b"conflict fixture".to_vec())],
        &[],
        &[],
    );
    conflict.0.conflicts.push(format!("{PREFIX}virtual"));
    packages.push(conflict);
    f.repo.snapshot(&packages);
    f.fails(&["--refresh", "add", "conflict"], "Conflicting packages");
    assert_eq!(original, f.state());
    println!("Checking reader/writer exclusion for arbitrary project processes");
    let ready = f.temp.path().join("ready");
    let release = f.temp.path().join("release");
    let script = format!(
        "[IO.File]::WriteAllText('{}','ready');while(-not(Test-Path -LiteralPath '{}')){{Start-Sleep -Milliseconds 50}}",
        ready.display(),
        release.display()
    );
    let reader = f
        .command(&["run", "powershell.exe", "-NoProfile", "-Command", &script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut reader = ReaderProcess {
        child: reader,
        release: release.clone(),
    };
    let started = Instant::now();
    while !ready.exists() {
        assert!(
            reader.child.try_wait().unwrap().is_none(),
            "Reader exited before acquiring its lease"
        );
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "Reader did not acquire lease"
        );
        thread::sleep(Duration::from_millis(50));
    }
    let output = f.command(&["--offline", "sync"]).output().unwrap();
    fs::write(&release, "release").unwrap();
    assert!(reader.child.wait().unwrap().success());
    assert!(!output.status.success() && text(&output).contains("Another cpuv operation"));
    assert_eq!(original, f.state());
    println!("Checking the standalone shim holds its lease for compiler lifetime");
    let waiter_source = f.temp.path().join("waiter.cpp");
    let waiter = f.temp.path().join("waiter.exe");
    fs::write(&waiter_source, r#"#include <windows.h>
#include <cstdlib>
#include <cstdio>
int main(){const char* ready=std::getenv("CPUV_TEST_READY");const char* release=std::getenv("CPUV_TEST_RELEASE");if(!ready||!release)return 2;FILE* f=std::fopen(ready,"wb");if(!f)return 3;std::fclose(f);while(GetFileAttributesA(release)==INVALID_FILE_ATTRIBUTES)Sleep(50);return 0;}
"#).unwrap();
    f.raw(&[
        waiter_source.to_str().unwrap(),
        "-static-libgcc",
        "-static-libstdc++",
        "-o",
        waiter.to_str().unwrap(),
    ]);
    let manifest_path = f.project.join(".cpuv/env/manifest.json");
    let original_metadata = fs::read(&manifest_path).unwrap();
    let mut metadata: cpuv_relocate::Environment =
        serde_json::from_slice(&original_metadata).unwrap();
    metadata.compiler = waiter.clone();
    metadata.tool_identity = cpuv_core::ToolIdentity::capture([waiter]).unwrap();
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
    cpuv_core::seal(&f.project.join(".cpuv/env")).unwrap();
    let shim_ready = f.temp.path().join("shim-ready");
    let shim_release = f.temp.path().join("shim-release");
    let child = Command::new(&shim)
        .current_dir(&f.project)
        .args(["-c", "main.cpp"])
        .env("CPUV_TEST_READY", &shim_ready)
        .env("CPUV_TEST_RELEASE", &shim_release)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut compiler = ReaderProcess {
        child,
        release: shim_release.clone(),
    };
    let started = Instant::now();
    while !shim_ready.exists() {
        assert!(
            compiler.child.try_wait().unwrap().is_none(),
            "Shim exited before entering compiler"
        );
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "Shim did not start compiler"
        );
        thread::sleep(Duration::from_millis(50));
    }
    let result = f.command(&["--offline", "sync"]).output().unwrap();
    fs::write(&shim_release, "release").unwrap();
    assert!(compiler.child.wait().unwrap().success());
    assert!(!result.status.success() && text(&result).contains("Another cpuv operation"));
    fs::write(&manifest_path, original_metadata).unwrap();
    cpuv_core::seal(&f.project.join(".cpuv/env")).unwrap();
    assert_eq!(original, f.state());
    println!("Checking intentional graph update and remove");
    packages[0] = implementation("2.0-1");
    f.repo.snapshot(&packages);
    f.ok(&["--offline", "sync"]);
    assert_eq!(original.1, fs::read(f.project.join("cpuv.lock")).unwrap());
    f.ok(&["lock"]);
    assert_ne!(original.1, fs::read(f.project.join("cpuv.lock")).unwrap());
    f.ok(&["run", "g++", "main.cpp"]);
    f.ok(&["remove", "a"]);
    f.ok(&["verify"]);
    println!("PASS: Phase 1 local repository lifecycle and native regression gate");
}
