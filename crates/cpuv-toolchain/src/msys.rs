//! Discover MSYS2 installations through environment, registration and root markers.
use super::*;
use std::collections::BTreeSet;

fn add_root(roots: &mut BTreeSet<PathBuf>, path: &Path) {
    for root in path.ancestors().take(4) {
        if root.join("usr/bin/msys-2.0.dll").is_file() || root.join("msys2.exe").is_file() {
            roots.insert(root.to_owned());
            break;
        }
    }
}

pub(super) fn discover() -> Result<PathBuf> {
    let mut roots = BTreeSet::new();
    let mut candidates = BTreeSet::new();
    for name in [
        "MSYSTEM_PREFIX",
        "MINGW_PREFIX",
        "CPUV_MSYS2_ROOT",
        "MSYS2_ROOT",
    ] {
        if let Some(path) = std::env::var_os(name) {
            let path = PathBuf::from(path);
            let compiler = path.join("bin/g++.exe");
            if compiler.is_file() {
                candidates.insert(driver_path(compiler.canonicalize()?));
            }
            add_root(&mut roots, &path);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            add_root(&mut roots, &directory);
        }
    }
    #[cfg(windows)]
    for path in registration::installations() {
        add_root(&mut roots, &path);
    }
    // Bounded discovery of portable installs: inspect only immediate children,
    // selected by MSYS2's marker files, without assuming a drive or folder name.
    let mut parents: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    if let Some(drive) = std::env::var_os("SystemDrive") {
        parents.push(PathBuf::from(drive).join(std::path::MAIN_SEPARATOR_STR));
    }
    for parent in parents {
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.take(512).flatten() {
                add_root(&mut roots, &entry.path());
            }
        }
    }
    for root in roots {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.take(512).flatten() {
                let compiler = entry.path().join("bin/g++.exe");
                if compiler.is_file() {
                    candidates.insert(driver_path(compiler.canonicalize()?));
                }
            }
        }
    }
    let mut native = Vec::new();
    let mut preferred = Vec::new();
    for candidate in candidates {
        if let Ok(tc) = probe_with_flags(Some(&candidate), &[]) {
            if tc.provider.is_some() {
                preferred.push(candidate.clone());
            }
            native.push(candidate);
        }
    }
    let choices = if preferred.is_empty() {
        native
    } else {
        preferred
    };
    match choices.as_slice() {
        [path] => Ok(path.clone()),
        [] => bail!(
            "No native GCC found on PATH or in discovered MSYS2 installations. Select --compiler, CPUV_CXX, or CPUV_MSYS2_ROOT"
        ),
        _ => bail!(
            "Multiple GCC installations found: {}. Select one with --compiler or CPUV_CXX",
            choices
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(windows)]
mod registration {
    use super::*;
    use windows_sys::Win32::{Foundation::ERROR_SUCCESS, System::Registry::*};

    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    fn open(root: HKEY, name: &str, view: u32) -> Option<Key> {
        let mut key = std::ptr::null_mut();
        let status =
            unsafe { RegOpenKeyExW(root, wide(name).as_ptr(), 0, KEY_READ | view, &mut key) };
        (status == ERROR_SUCCESS).then_some(Key(key))
    }
    fn text(key: &Key, name: &str) -> Option<String> {
        let mut kind = 0;
        let mut bytes = 0;
        let name = wide(name);
        if unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                std::ptr::null_mut(),
                &mut bytes,
            )
        } != ERROR_SUCCESS
            || ![REG_SZ, REG_EXPAND_SZ].contains(&kind)
            || bytes > 65536
            || bytes % 2 != 0
        {
            return None;
        }
        let mut data = vec![0_u16; bytes as usize / 2 + 1];
        if unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                data.as_mut_ptr().cast(),
                &mut bytes,
            )
        } != ERROR_SUCCESS
        {
            return None;
        }
        let length = data
            .iter()
            .position(|&value| value == 0)
            .unwrap_or(data.len());
        String::from_utf16(&data[..length]).ok()
    }
    pub(super) fn installations() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
                let parent = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
                if let Some(key) = open(root, parent, view) {
                    for index in 0..8192 {
                        let mut name = vec![0_u16; 512];
                        let mut length = name.len() as u32;
                        if unsafe {
                            RegEnumKeyExW(
                                key.0,
                                index,
                                name.as_mut_ptr(),
                                &mut length,
                                std::ptr::null(),
                                std::ptr::null_mut(),
                                std::ptr::null_mut(),
                                std::ptr::null_mut(),
                            )
                        } != ERROR_SUCCESS
                        {
                            break;
                        }
                        let name = String::from_utf16_lossy(&name[..length as usize]);
                        if let Some(entry) = open(root, &format!(r"{parent}\{name}"), view)
                            && text(&entry, "DisplayName")
                                .is_some_and(|name| name.to_ascii_lowercase().contains("msys2"))
                        {
                            if let Some(path) = text(&entry, "InstallLocation") {
                                paths.push(PathBuf::from(path.trim_matches('"')));
                            }
                            if let Some(icon) = text(&entry, "DisplayIcon") {
                                paths.push(PathBuf::from(
                                    icon.split(',').next().unwrap_or(&icon).trim_matches('"'),
                                ));
                            }
                        }
                    }
                }
                if let Some(key) = open(
                    root,
                    r"Software\Microsoft\Windows\CurrentVersion\App Paths\msys2.exe",
                    view,
                ) && let Some(path) = text(&key, "")
                {
                    paths.push(PathBuf::from(path.trim_matches('"')));
                }
            }
        }
        paths
    }
}
