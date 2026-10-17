//! Explicit MSYS2 host-tool requests stay separate from project MinGW libraries.
use anyhow::{Context, Result, ensure};
use cpuv_toolchain::Toolchain;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

pub struct Manager {
    executable: PathBuf,
    compiler_bin: PathBuf,
    root: PathBuf,
}
impl Manager {
    pub fn discover(toolchain: &Toolchain) -> Result<Self> {
        ensure!(
            toolchain.provider.is_some(),
            "base-devel requires an MSYS2 compiler installation; it is host tooling, not a MinGW library"
        );
        let root = toolchain
            .sysroot
            .parent()
            .context("MSYS2 compiler has no installation parent")?
            .to_owned();
        let executable = root.join("usr/bin/pacman.exe");
        ensure!(
            executable.is_file(),
            "MSYS2 pacman not found next to this compiler; install base-devel using the compiler's own MSYS2 installation"
        );
        Ok(Self {
            executable,
            compiler_bin: toolchain.compiler_path.parent().unwrap().to_owned(),
            root,
        })
    }
    fn command(&self) -> Result<Command> {
        let mut command = Command::new(&self.executable);
        let mut paths = vec![self.root.join("usr/bin"), self.compiler_bin.clone()];
        if let Some(path) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&path));
        }
        command
            .env("PATH", std::env::join_paths(paths)?)
            .current_dir(&self.root);
        Ok(command)
    }
    pub fn ensure_base_devel(&self, offline: bool) -> Result<()> {
        // Query first: this also lets an already installed host prerequisite work offline.
        if self
            .command()?
            .args(["-Q", "base-devel"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success()
        {
            eprintln!("base-devel is already installed in this MSYS2 installation; skipped.");
            return Ok(());
        }
        ensure!(
            !offline,
            "base-devel is missing from the host MSYS2 installation; installing it requires an online pacman transaction"
        );
        eprintln!(
            "Installing explicitly requested MSYS2 host tooling: pacman -S --needed --noconfirm base-devel"
        );
        ensure!(
            self.command()?
                .args(["-S", "--needed", "--noconfirm", "base-devel"])
                .status()
                .context("Starting the compiler's MSYS2 pacman")?
                .success(),
            "MSYS2 could not install base-devel. Update that MSYS2 installation and retry; project libraries remain installed separately"
        );
        Ok(())
    }
}
