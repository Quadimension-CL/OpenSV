use std::{ffi::c_void, os::windows::ffi::OsStringExt, path::PathBuf, ptr};

#[link(name = "shell32")]
unsafe extern "system" {
    fn SHGetSpecialFolderPathW(
        window: *mut c_void,
        path: *mut u16,
        folder: i32,
        create: i32,
    ) -> i32;
}

pub(super) fn roaming_directory() -> Result<PathBuf, String> {
    let mut buffer = [0u16; 260];
    // SAFETY: The buffer has MAX_PATH characters. CSIDL_APPDATA is read only;
    // create=false ensures this lookup cannot create application directories.
    if unsafe { SHGetSpecialFolderPathW(ptr::null_mut(), buffer.as_mut_ptr(), 0x1a, 0) } == 0 {
        return Err("无法定位当前用户的漫游配置目录。".into());
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .ok_or("当前用户的漫游配置目录路径无效。")?;
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..end])))
}
