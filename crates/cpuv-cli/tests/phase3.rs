//! Representative native consumers and failed-provider preservation on a selected compiler.
#![cfg(windows)]
mod support;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::Ordering,
    time::Instant,
};

struct Fixture {
    root: tempfile::TempDir,
    app: PathBuf,
    timings: Vec<serde_json::Value>,
}
impl Fixture {
    fn invoke(&mut self, args: &[&str], success: bool) -> Output {
        eprintln!("Phase 3: {args:?}");
        let mut command = Command::new(env!("CARGO_BIN_EXE_cpuv"));
        command
            .arg("--project")
            .arg(&self.app)
            .args(args)
            .current_dir(&self.app);
        for name in [
            "CPPFLAGS",
            "CFLAGS",
            "CXXFLAGS",
            "LDFLAGS",
            "CPUV_PROFILE",
            "CPUV_CXX",
            "CXX",
        ] {
            command.env_remove(name);
        }
        let start = Instant::now();
        let output = command.output().unwrap();
        if let Some(directory) = std::env::var_os("CPUV_PHASE3_EVIDENCE") {
            fs::create_dir_all(&directory).unwrap();
            let identity = blake3::hash(self.app.to_string_lossy().as_bytes()).to_hex();
            fs::write(
                Path::new(&directory)
                    .join(format!("command-{identity}-{:02}.log", self.timings.len())),
                format!(
                    "{args:?}\n{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ),
            )
            .unwrap();
        }
        self.timings.push(json!({"operation":args.join(" "),"milliseconds":start.elapsed().as_secs_f64()*1000.0,"success":output.status.success()}));
        assert_eq!(
            output.status.success(),
            success,
            "{args:?}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
    fn state(&self) -> Vec<Vec<u8>> {
        ["cpuv.toml", "cpuv.lock", ".cpuv/env/files.json"]
            .iter()
            .map(|p| fs::read(self.app.join(p)).unwrap())
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = Command::new(env!("CARGO_BIN_EXE_cpuv"))
            .args(["cache", "forget"])
            .arg(&self.app)
            .output();
    }
}

#[test]
#[ignore = "Requires CPUV_TEST_CXX and verified Conan/CMake bootstrap tools"]
fn c_cpp_headers_components_codegen_gui_and_provider_outage() {
    let root = tempfile::Builder::new()
        .prefix("cpuv phase3 ")
        .tempdir()
        .unwrap();
    let app = root.path().join("consumer");
    fs::create_dir(&app).unwrap();
    let mut f = Fixture {
        root,
        app,
        timings: Vec::new(),
    };
    let compiler =
        PathBuf::from(std::env::var_os("CPUV_TEST_CXX").expect("CPUV_TEST_CXX required"));
    f.invoke(
        &[
            "--compiler",
            compiler.to_str().unwrap(),
            "--provider",
            "conan",
            "init",
        ],
        true,
    );
    let sources = [
        (
            "CMakeLists.txt",
            r#"cmake_minimum_required(VERSION 3.20)
project(matrix LANGUAGES C)
add_library(matrix_static STATIC static.c)
add_library(matrix_shared SHARED shared.c)
install(TARGETS matrix_static matrix_shared ARCHIVE DESTINATION lib LIBRARY DESTINATION lib RUNTIME DESTINATION bin)
install(FILES matrix.h header_only.h DESTINATION include)
"#,
        ),
        ("static.c", "int static_value(void){return 31;}\n"),
        (
            "shared.c",
            "__declspec(dllexport) int shared_value(void){return 42;}\n",
        ),
        ("header_only.h", "#define HEADER_ONLY_VALUE 73\n"),
        (
            "matrix.h",
            "#ifdef __cplusplus\nextern \"C\" {\n#endif\nint static_value(void);\n__declspec(dllimport) int shared_value(void);\n#ifdef __cplusplus\n}\n#endif\n",
        ),
        ("LICENSE", "Synthetic test fixture; public domain.\n"),
    ];
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in sources {
        zip.start_file(
            format!("source/{name}"),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(contents.as_bytes()).unwrap();
    }
    let archive = zip.finish().unwrap().into_inner();
    let server = support::Server::new();
    server
        .routes
        .lock()
        .unwrap()
        .insert("/source.zip".into(), archive.clone());
    let specification = json!({"version":"1.0","source":{"input":{"kind":"archive","url":format!("{}/source.zip",server.url),"sha256":format!("{:x}",Sha256::digest(&archive)),"format":"zip","strip_components":1},"build":{"system":"cmake"},"interface":{"libraries":["matrix_static","matrix_shared"],"cmake_file":"matrix","cmake_target":"matrix::all"},"license_files":["LICENSE"],"redistributable":true}});
    let spec = f.root.path().join("source.toml");
    fs::write(&spec, toml::to_string_pretty(&specification).unwrap()).unwrap();
    f.invoke(
        &["source", "add", "matrix-fixture", spec.to_str().unwrap()],
        true,
    );
    fs::write(f.app.join("main.c"),"#include <matrix.h>\n#include <header_only.h>\n#include \"generated.h\"\nint main(void){return static_value()+shared_value()==HEADER_ONLY_VALUE && GENERATED_VALUE==73?0:1;}\n").unwrap();
    fs::copy(f.app.join("main.c"), f.app.join("main.cpp")).unwrap();
    fs::write(f.app.join("gui.c"),"#include <windows.h>\n#include <matrix.h>\nint WINAPI WinMain(HINSTANCE a,HINSTANCE b,LPSTR c,int d){(void)a;(void)b;(void)c;(void)d;return static_value()+shared_value()==73?0:1;}\n").unwrap();
    fs::write(f.app.join("data.txt"), "resource fixture").unwrap();
    fs::write(f.app.join("app.rc"), "1 RCDATA \"data.txt\"\n").unwrap();
    fs::write(
        f.app.join("generate.cmake"),
        "file(WRITE \"${OUTPUT}\" \"#define GENERATED_VALUE 73\\n\")\n",
    )
    .unwrap();
    fs::write(f.app.join("CMakeLists.txt"),r#"cmake_minimum_required(VERSION 3.20)
project(consumers LANGUAGES C CXX RC)
find_package(matrix CONFIG REQUIRED)
add_custom_command(OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/generated.h" COMMAND "${CMAKE_COMMAND}" "-DOUTPUT=${CMAKE_CURRENT_BINARY_DIR}/generated.h" -P "${CMAKE_CURRENT_SOURCE_DIR}/generate.cmake" DEPENDS generate.cmake VERBATIM)
enable_testing()
foreach(language IN ITEMS c cpp)
  add_executable(consumer_${language} main.${language} "${CMAKE_CURRENT_BINARY_DIR}/generated.h")
  target_include_directories(consumer_${language} PRIVATE "${CMAKE_CURRENT_BINARY_DIR}")
  target_link_libraries(consumer_${language} PRIVATE matrix::all)
  add_test(NAME consumer_${language} COMMAND consumer_${language})
endforeach()
add_executable(consumer_gui WIN32 gui.c app.rc)
target_link_libraries(consumer_gui PRIVATE matrix::all)
add_test(NAME consumer_gui COMMAND consumer_gui)
"#).unwrap();
    f.invoke(&["test"], true);
    assert!(f.app.join(".cpuv/build/cmake/generated.h").is_file());
    let build = f.app.join(".cpuv/build/cmake/consumer_gui.exe");
    assert!(build.is_file());
    // Verify both the GUI subsystem and a nonempty resource data directory in the produced PE.
    let pe = fs::read(build).unwrap();
    let offset = u32::from_le_bytes(pe[0x3c..0x40].try_into().unwrap()) as usize + 24;
    assert_eq!(
        u16::from_le_bytes(pe[offset + 68..offset + 70].try_into().unwrap()),
        2
    );
    let directories = offset
        + if u16::from_le_bytes(pe[offset..offset + 2].try_into().unwrap()) == 0x20b {
            112
        } else {
            96
        };
    assert!(u32::from_le_bytes(pe[directories + 20..directories + 24].try_into().unwrap()) > 0);
    let before = f.state();
    // An uncached provider request returning 503 must leave the installed graph usable.
    let mut unavailable = specification.clone();
    unavailable["source"]["input"]["url"] = json!(format!("{}/unavailable.zip", server.url));
    unavailable["source"]["input"]["sha256"] = json!("a".repeat(64));
    fs::write(&spec, toml::to_string_pretty(&unavailable).unwrap()).unwrap();
    let requests_before_outage = server.requests.load(Ordering::SeqCst);
    let failed = f.invoke(
        &["source", "add", "matrix-fixture", spec.to_str().unwrap()],
        false,
    );
    assert!(
        server.requests.load(Ordering::SeqCst) > requests_before_outage,
        "The failed operation did not reach the unavailable provider: {}",
        String::from_utf8_lossy(&failed.stderr)
    );
    assert_eq!(before, f.state());
    server.routes.lock().unwrap().clear();
    let requests = server.requests.load(Ordering::SeqCst);
    for _ in 0..3 {
        f.invoke(&["verify"], true);
        f.invoke(&["--offline", "sync"], true);
    }
    assert_eq!(
        server.requests.load(Ordering::SeqCst),
        requests,
        "Offline restore contacted the provider"
    );
    assert_eq!(before[..2], f.state()[..2]);
    f.invoke(&["--offline", "test"], true);
    if let Some(directory) = std::env::var_os("CPUV_PHASE3_EVIDENCE") {
        fs::create_dir_all(&directory).unwrap();
        let identity = blake3::hash(compiler.to_string_lossy().as_bytes())
            .to_hex()
            .to_string();
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(f.app.join(".cpuv/toolchain.json")).unwrap()).unwrap();
        fs::write(Path::new(&directory).join(format!("consumer-{identity}.json")),serde_json::to_vec_pretty(&json!({"schema":1,"result":"passed","toolchain":metadata,"measurements":f.timings})).unwrap()).unwrap();
    }
    eprintln!(
        "PASS: C/C++, header-only, static/shared components, generated code, GUI/resources, outage preservation and offline no-network restoration"
    );
}
