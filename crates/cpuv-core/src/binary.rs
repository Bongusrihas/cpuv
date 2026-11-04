//! Read-only inspection without allocating an archive-sized heap buffer on Windows.
use super::*;
use std::ops::Deref;

pub struct Data {
    #[cfg(windows)]
    mapping: Option<Mapping>,
    #[cfg(not(windows))]
    bytes: Vec<u8>,
}

impl Data {
    pub fn open(path: &Path) -> Result<Self> {
        let mib: u64 = std::env::var("CPUV_MAX_BINARY_MIB").map_or(Ok(4096), |s| s.parse())?;
        ensure!(
            (1..=1_048_576).contains(&mib),
            "CPUV_MAX_BINARY_MIB must be between 1 and 1048576"
        );
        Self::open_with_limit(path, mib * 1024 * 1024)
    }

    pub fn open_with_limit(path: &Path, limit: u64) -> Result<Self> {
        regular(path)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // Deny writes/deletion through every hardlink while any borrowed slice is alive.
            options.share_mode(1); // FILE_SHARE_READ
        }
        let file = options
            .open(path)
            .with_context(|| format!("Opening immutable binary {}", path.display()))?;
        let length = file.metadata()?.len();
        ensure!(
            length <= limit && length <= isize::MAX as u64,
            "Binary {} exceeds inspection limit; adjust CPUV_MAX_BINARY_MIB",
            path.display()
        );
        #[cfg(windows)]
        {
            Ok(Self {
                mapping: if length == 0 {
                    None
                } else {
                    Some(Mapping::new(file, length as usize)?)
                },
            })
        }
        #[cfg(not(windows))]
        {
            let mut bytes = Vec::new();
            file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() as u64 <= limit,
                "Binary grew beyond inspection limit"
            );
            Ok(Self { bytes })
        }
    }
}

impl Deref for Data {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        #[cfg(windows)]
        {
            self.mapping.as_ref().map_or(&[], |mapping| {
                // The non-null read-only view covers exactly length bytes. Its file handle
                // excludes writers, and the view cannot be unmapped until this borrow ends.
                unsafe { std::slice::from_raw_parts(mapping.view.Value.cast(), mapping.length) }
            })
        }
        #[cfg(not(windows))]
        {
            &self.bytes
        }
    }
}

#[cfg(windows)]
struct Mapping {
    _file: File,
    view: windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS,
    length: usize,
}

#[cfg(windows)]
impl Mapping {
    fn new(file: File, length: usize) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::CloseHandle, System::Memory::*};
        // The owned file remains alive for the view's entire lifetime; no writable views exist.
        let handle = unsafe {
            CreateFileMappingW(
                file.as_raw_handle() as _,
                std::ptr::null(),
                PAGE_READONLY,
                0,
                0,
                std::ptr::null(),
            )
        };
        ensure!(
            !handle.is_null(),
            "Creating binary mapping: {}",
            std::io::Error::last_os_error()
        );
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, length) };
        let error = std::io::Error::last_os_error();
        unsafe {
            CloseHandle(handle);
        }
        ensure!(!view.Value.is_null(), "Mapping binary: {error}");
        Ok(Self {
            _file: file,
            view,
            length,
        })
    }
}

#[cfg(windows)]
impl Drop for Mapping {
    fn drop(&mut self) {
        // Unmap before the owned file closes and permits writers again.
        unsafe {
            windows_sys::Win32::System::Memory::UnmapViewOfFile(self.view);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspection_bounds_and_file_lifetime() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("fixture.lib");
        fs::write(&path, b"archive data").unwrap();
        assert!(Data::open_with_limit(&path, 3).is_err());
        let data = Data::open_with_limit(&path, 1024).unwrap();
        assert_eq!(&*data, b"archive data");
        #[cfg(windows)]
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        drop(data);
        fs::write(&path, b"").unwrap();
        assert!(Data::open_with_limit(&path, 0).unwrap().is_empty());
    }
}
