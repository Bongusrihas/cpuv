//! Per-user native installation; no PowerShell, Rust, or administrator rights.
use anyhow::{Context, Result, ensure};
use clap::{Args, Parser};
use std::{fs, io::IsTerminal, path::PathBuf};

#[derive(Args, Debug, Default)]
pub struct Options {
    /// Install in this directory instead of the current user's .local/bin.
    #[arg(long, env = "CPUV_INSTALL_DIR")]
    pub install_dir: Option<PathBuf>,
    /// Leave the user's PATH unchanged.
    #[arg(long)]
    pub no_path: bool,
}

#[derive(Parser)]
#[command(
    name = "cpuv-setup",
    version,
    about = "Install cpuv for the current Windows user"
)]
struct Setup {
    #[command(flatten)]
    options: Options,
}

pub fn is_setup() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|name| name.eq_ignore_ascii_case("cpuv-setup"))
        })
        .unwrap_or(false)
}

pub fn setup() -> Result<i32> {
    let result = install(&Setup::parse().options);
    if let Err(error) = &result {
        eprintln!("Installation failed: {error:#}");
    }
    #[cfg(windows)]
    if std::io::stdin().is_terminal() {
        let mut processes = [0_u32; 2];
        // An Explorer launch has its own console; retain the result on screen.
        if unsafe {
            windows_sys::Win32::System::Console::GetConsoleProcessList(processes.as_mut_ptr(), 2)
        } == 1
        {
            println!("Press Enter to close.");
            let _ = std::io::stdin().read_line(&mut String::new());
        }
    }
    result
}

pub fn install(options: &Options) -> Result<i32> {
    ensure!(cfg!(windows), "The standalone installer supports Windows");
    let directory = match &options.install_dir {
        Some(path) => path.clone(),
        None => PathBuf::from(std::env::var_os("USERPROFILE").context("USERPROFILE is not set")?)
            .join(".local/bin"),
    };
    fs::create_dir_all(&directory).context("Creating the installation directory")?;
    let directory = cpuv_toolchain::driver_path(directory.canonicalize()?);
    let destination = directory.join("cpuv.exe");
    let source = cpuv_toolchain::driver_path(std::env::current_exe()?.canonicalize()?);
    if source != destination {
        let staged = tempfile::NamedTempFile::new_in(&directory)?;
        fs::copy(&source, staged.path()).context("Staging cpuv.exe")?;
        staged
            .persist(&destination)
            .map_err(|error| error.error)
            .context("Replacing cpuv.exe; close any running cpuv programs and retry")?;
    }
    #[cfg(windows)]
    if !options.no_path {
        windows::add_to_user_path(&directory)?;
    }
    println!(
        "Installed cpuv {} in {}",
        env!("CARGO_PKG_VERSION"),
        directory.display()
    );
    if options.no_path {
        println!(
            "Add this directory to PATH, or run {} directly.",
            destination.display()
        );
    } else {
        println!("Open a new terminal, then use: cpuv init");
    }
    Ok(0)
}

#[cfg(windows)]
pub(crate) mod windows {
    use super::*;
    use anyhow::bail;
    use std::path::Path;
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ,
            REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegQueryValueExW,
            RegSetValueExW,
        },
        UI::WindowsAndMessaging::{
            HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
        },
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    impl Key {
        fn open(subkey: &str) -> Result<Self> {
            let mut key = std::ptr::null_mut();
            let name = wide(subkey);
            // Buffers outlive the synchronous API call; the returned key is closed by Drop.
            let status = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    name.as_ptr(),
                    0,
                    std::ptr::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_QUERY_VALUE | KEY_SET_VALUE,
                    std::ptr::null(),
                    &mut key,
                    std::ptr::null_mut(),
                )
            };
            ensure!(
                status == ERROR_SUCCESS,
                "Opening user environment: {}",
                std::io::Error::from_raw_os_error(status as i32)
            );
            Ok(Self(key))
        }
        fn read(&self) -> Result<(String, u32)> {
            let name = wide("Path");
            let mut kind = 0;
            let mut bytes = 0;
            let status = unsafe {
                RegQueryValueExW(
                    self.0,
                    name.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    std::ptr::null_mut(),
                    &mut bytes,
                )
            };
            if status == ERROR_FILE_NOT_FOUND {
                return Ok((String::new(), REG_EXPAND_SZ));
            }
            ensure!(
                status == ERROR_SUCCESS,
                "Reading user PATH: Windows error {status}"
            );
            ensure!(
                [REG_SZ, REG_EXPAND_SZ].contains(&kind) && bytes <= 1024 * 1024 && bytes % 2 == 0,
                "User PATH has an unsupported registry format or size; it was not modified"
            );
            let mut data = vec![0_u16; (bytes / 2 + 1) as usize];
            let status = unsafe {
                RegQueryValueExW(
                    self.0,
                    name.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    data.as_mut_ptr().cast(),
                    &mut bytes,
                )
            };
            ensure!(
                status == ERROR_SUCCESS,
                "Reading user PATH: Windows error {status}"
            );
            let length = data.iter().position(|&c| c == 0).unwrap_or(data.len());
            let text = String::from_utf16(&data[..length])
                .context("User PATH contains invalid UTF-16; it was not modified")?;
            Ok((text, kind))
        }
        fn write(&self, path: &str, kind: u32) -> Result<()> {
            let name = wide("Path");
            let value = wide(path);
            ensure!(
                value.len() <= 32767,
                "Updated user PATH is too long; it was not modified"
            );
            let status = unsafe {
                RegSetValueExW(
                    self.0,
                    name.as_ptr(),
                    0,
                    kind,
                    value.as_ptr().cast(),
                    (value.len() * 2) as u32,
                )
            };
            if status != ERROR_SUCCESS {
                bail!("Updating user PATH: Windows error {status}");
            }
            Ok(())
        }
    }
    fn prepend(path: &str, directory: &str) -> String {
        let equivalent = |entry: &str| {
            entry
                .trim()
                .trim_matches('"')
                .trim_end_matches(['\\', '/'])
                .eq_ignore_ascii_case(directory.trim_end_matches(['\\', '/']))
        };
        let mut entries = vec![directory];
        entries.extend(
            path.split(';')
                .filter(|entry| !entry.is_empty() && !equivalent(entry)),
        );
        entries.join(";")
    }
    fn normalized_entry(entry: &str) -> Option<String> {
        use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
        if entry.trim().trim_matches('"').is_empty() {
            return None;
        }
        let input = wide(entry.trim().trim_matches('"'));
        let length = unsafe { ExpandEnvironmentStringsW(input.as_ptr(), std::ptr::null_mut(), 0) };
        if length == 0 || length > 32768 {
            return None;
        }
        let mut output = vec![0_u16; length as usize];
        let written =
            unsafe { ExpandEnvironmentStringsW(input.as_ptr(), output.as_mut_ptr(), length) };
        if written == 0 || written > length {
            return None;
        }
        let expanded = String::from_utf16(&output[..written.saturating_sub(1) as usize]).ok()?;
        let path = Path::new(&expanded);
        if !path.is_absolute() {
            return None;
        }
        let resolved = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        Some(
            cpuv_toolchain::driver_path(resolved)
                .to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_lowercase(),
        )
    }
    pub(crate) struct PathEdit {
        old: String,
        updated: String,
        kind: u32,
    }
    impl PathEdit {
        pub(crate) fn restore(self) -> Result<()> {
            let key = Key::open("Environment")?;
            ensure!(
                key.read()? == (self.updated, self.kind),
                "User PATH changed concurrently; it was not overwritten during recovery"
            );
            key.write(&self.old, self.kind)?;
            broadcast();
            Ok(())
        }
    }
    pub(crate) fn remove_from_user_path(directories: &[PathBuf]) -> Result<Option<PathEdit>> {
        if directories.is_empty() {
            return Ok(None);
        }
        let key = Key::open("Environment")?;
        let (old, kind) = key.read()?;
        let normalized: Vec<_> = directories
            .iter()
            .filter_map(|p| p.to_str().and_then(normalized_entry))
            .collect();
        let updated = old
            .split(';')
            .filter(|entry| !normalized_entry(entry).is_some_and(|path| normalized.contains(&path)))
            .collect::<Vec<_>>()
            .join(";");
        if updated == old {
            return Ok(None);
        }
        key.write(&updated, kind)?;
        broadcast();
        Ok(Some(PathEdit { old, updated, kind }))
    }
    fn broadcast() {
        let message = wide("Environment");
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                message.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                2000,
                std::ptr::null_mut(),
            );
        }
    }
    pub fn add_to_user_path(directory: &Path) -> Result<()> {
        let key = Key::open("Environment")?;
        let (old, kind) = key.read()?;
        let directory = directory
            .to_str()
            .context("Installation directory is not valid Unicode")?;
        let updated = prepend(&old, directory);
        if old != updated {
            let backup = Path::new(directory)
                .parent()
                .context("Missing install parent")?
                .join("user-path-before-install.txt");
            if !backup.exists() {
                fs::write(backup, &old).context("Backing up user PATH")?;
            }
            key.write(&updated, kind)?;
        }
        let message = wide("Environment");
        // Notify Windows; existing terminal processes still need to be reopened.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                message.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                2000,
                std::ptr::null_mut(),
            );
        }
        Ok(())
    }
}
