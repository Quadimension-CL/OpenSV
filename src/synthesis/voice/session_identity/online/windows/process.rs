use std::{
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
};

use super::{Result, VoiceError, native::*};

pub(super) const EDITOR_NAME: &str = "synthv-studio.exe";

pub(super) fn candidate_process_ids() -> Result<Vec<u32>> {
    let snapshot = Handle::snapshot(SNAP_PROCESS, 0)?;
    let mut entry = ProcessEntry::new();
    let mut processes = Vec::new();
    // SAFETY: entry is a writable PROCESSENTRY32W with its required size set.
    let mut found = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    while found != 0 {
        if wide_path(&entry.exe_file)
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(EDITOR_NAME)
        {
            processes.push(entry.process_id);
        }
        // SAFETY: The snapshot and output record remain valid across iteration.
        found = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
    // SAFETY: Read the enumeration's last-error value immediately after failure.
    if unsafe { GetLastError() } != ERROR_NO_MORE_FILES {
        return Err(api_error("Process32NextW"));
    }
    Ok(processes)
}

pub(super) fn image_path(process: &Handle) -> Result<PathBuf> {
    let mut buffer = vec![0u16; 32768];
    let mut length = buffer.len() as u32;
    // SAFETY: buffer is a writable length-code-unit UTF-16 output.
    if unsafe { QueryFullProcessImageNameW(process.0, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
        return Err(api_error("QueryFullProcessImageNameW"));
    }
    if length == 0 || length as usize >= buffer.len() {
        return Err(VoiceError::new("所选进程的可执行文件路径无效。"));
    }
    let path = PathBuf::from(OsString::from_wide(&buffer[..length as usize]));
    if !path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(EDITOR_NAME))
    {
        return Err(VoiceError::new("所选进程不是 Synthesizer V Studio。"));
    }
    Ok(path)
}

/// The executable's loaded base comes from the OS module snapshot, never from
/// its preferred PE image base or a presumed address without ASLR.
pub(super) fn main_module(process_id: u32, executable: &Path) -> Result<(usize, usize)> {
    let snapshot = Handle::snapshot(SNAP_MODULE, process_id)?;
    let mut entry = ModuleEntry::new();
    // SAFETY: entry is a writable MODULEENTRY32W with its required size set.
    if unsafe { Module32FirstW(snapshot.0, &mut entry) } == 0 {
        return Err(api_error("Module32FirstW"));
    }
    let module_path = wide_path(&entry.exe_path);
    let module_name = wide_path(&entry.module_name);
    if entry.process_id != process_id
        || !module_name
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(EDITOR_NAME)
        || !std::fs::canonicalize(module_path)?
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(
                &std::fs::canonicalize(executable)?
                    .as_os_str()
                    .to_string_lossy(),
            )
        || entry.base_address.is_null()
        || entry.image_size == 0
    {
        return Err(VoiceError::new(
            "无法确认所选原编辑器的主模块，未读取模型上下文。",
        ));
    }
    Ok((entry.base_address as usize, entry.image_size as usize))
}

fn wide_path(text: &[u16]) -> PathBuf {
    let end = text
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(text.len());
    PathBuf::from(OsString::from_wide(&text[..end]))
}
