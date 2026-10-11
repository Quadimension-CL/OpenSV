use std::path::PathBuf;

#[cfg(windows)]
pub(super) fn default_dictionaries() -> Option<PathBuf> {
    std::env::var_os("ProgramFiles")
        .filter(|value| !value.is_empty())
        .map(|value| PathBuf::from(value).join("Synthesizer V Studio 2 Pro/clf-data"))
        .filter(|path| path.is_dir())
}

#[cfg(not(windows))]
pub(super) fn default_dictionaries() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
pub(super) fn default_sv1_databases() -> Option<PathBuf> {
    documents_directory()
        .map(|path| path.join("Dreamtonics/Synthesizer V Studio/databases"))
        .filter(|path| path.is_dir())
}

#[cfg(not(windows))]
pub(super) fn default_sv1_databases() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn documents_directory() -> Option<PathBuf> {
    use std::{
        ffi::{OsString, c_void},
        os::windows::ffi::OsStringExt,
    };

    #[repr(C)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    // FOLDERID_Documents respects relocated and OneDrive-managed Documents.
    const DOCUMENTS: Guid = Guid {
        data1: 0xfdd39ad0,
        data2: 0x238f,
        data3: 0x46af,
        data4: [0xad, 0xb4, 0x6c, 0x85, 0x48, 0x03, 0x69, 0xc7],
    };
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHGetKnownFolderPath(
            folder: *const Guid,
            flags: u32,
            token: *mut c_void,
            path: *mut *mut u16,
        ) -> i32;
    }
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoTaskMemFree(memory: *mut c_void);
    }

    let mut path = std::ptr::null_mut();
    // SAFETY: The GUID and output pointer remain valid for the call. A successful
    // result owns a null-terminated UTF-16 allocation, released exactly once.
    unsafe {
        let result = SHGetKnownFolderPath(&DOCUMENTS, 0, std::ptr::null_mut(), &mut path);
        if result < 0 || path.is_null() {
            if !path.is_null() {
                CoTaskMemFree(path.cast());
            }
            return None;
        }
        let mut length = 0;
        while *path.add(length) != 0 {
            length += 1;
        }
        let value = OsString::from_wide(std::slice::from_raw_parts(path, length));
        CoTaskMemFree(path.cast());
        Some(PathBuf::from(value))
    }
}
