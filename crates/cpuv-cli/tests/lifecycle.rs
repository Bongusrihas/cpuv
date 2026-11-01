//! Uses only isolated executable copies and a loopback release server.
#![cfg(windows)]
mod support;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

fn run(executable: &Path, args: &[&str], success: bool) -> Output {
    eprintln!("Lifecycle: {args:?}");
    let output = Command::new(executable).args(args).output().unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "{args:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn setup_update_rejection_replacement_and_uninstall_preserve_unowned_files() {
    let root = tempfile::Builder::new()
        .prefix("cpuv lifecycle ")
        .tempdir()
        .unwrap();
    let setup = root.path().join("cpuv-setup.exe");
    let bytes = fs::read(env!("CARGO_BIN_EXE_cpuv")).unwrap();
    fs::write(&setup, &bytes).unwrap();
    let installation = root.path().join("portable installation");
    run(
        &setup,
        &["--install-dir", installation.to_str().unwrap(), "--no-path"],
        true,
    );
    let executable = installation.join("cpuv.exe");
    assert_eq!(fs::read(&executable).unwrap(), bytes);
    fs::write(installation.join("unrelated.txt"), "preserve user data").unwrap();
    fs::write(installation.join("cpuv.cmd"), "unowned launcher").unwrap();
    if std::env::consts::ARCH != "x86_64" {
        let output = run(
            &executable,
            &[
                "self",
                "update",
                "--force",
                "--version",
                env!("CARGO_PKG_VERSION"),
            ],
            false,
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("Windows x64"));
        uninstall(&executable, &installation);
        return;
    }
    let server = support::Server::new();
    let version = env!("CARGO_PKG_VERSION");
    let asset = format!("/download/v{version}/cpuv-setup.exe");
    let checksum = format!("{asset}.sha256");
    let args = [
        "self",
        "update",
        "--force",
        "--version",
        version,
        "--release-base-url",
        &server.url,
    ];
    // Missing release, corrupt checksum, wrong target version: never replace an installation.
    run(&executable, &args, false);
    assert_eq!(fs::read(&executable).unwrap(), bytes);
    server.routes.lock().unwrap().insert(
        checksum.clone(),
        format!("{}  cpuv-setup.exe\n", "0".repeat(64)).into_bytes(),
    );
    server
        .routes
        .lock()
        .unwrap()
        .insert(asset.clone(), bytes.clone());
    let output = run(&executable, &args, false);
    assert!(String::from_utf8_lossy(&output.stderr).contains("checksum mismatch"));
    assert_eq!(fs::read(&executable).unwrap(), bytes);
    let declared = format!("{:x}  cpuv-setup.exe\n", Sha256::digest(&bytes)).into_bytes();
    {
        let mut routes = server.routes.lock().unwrap();
        routes.insert(checksum, declared.clone());
        routes.insert("/download/v999.0.0/cpuv-setup.exe".into(), bytes.clone());
        routes.insert("/download/v999.0.0/cpuv-setup.exe.sha256".into(), declared);
    }
    let output = run(
        &executable,
        &[
            "self",
            "update",
            "--version",
            "999.0.0",
            "--release-base-url",
            &server.url,
        ],
        false,
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("version differs"));
    assert_eq!(fs::read(&executable).unwrap(), bytes);
    // Exercise native self-replacement even when reinstalling the same version.
    run(&executable, &args, true);
    assert_eq!(fs::read(&executable).unwrap(), bytes);
    run(&executable, &["--version"], true);
    assert!(server.requests.load(std::sync::atomic::Ordering::SeqCst) > 0);
    uninstall(&executable, &installation);
}

fn uninstall(executable: &Path, installation: &Path) {
    run(executable, &["uninstall", "--dry-run"], true);
    assert!(executable.is_file());
    run(executable, &["uninstall"], true);
    let deadline = Instant::now() + Duration::from_secs(15);
    while executable.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    assert!(!executable.exists(), "Native self-delete did not complete");
    assert_eq!(
        fs::read_to_string(installation.join("unrelated.txt")).unwrap(),
        "preserve user data"
    );
    assert_eq!(
        fs::read_to_string(installation.join("cpuv.cmd")).unwrap(),
        "unowned launcher"
    );
}
