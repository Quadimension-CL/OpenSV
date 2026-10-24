use std::{
    fs::{File, OpenOptions},
    io::Read,
    os::windows::fs::OpenOptionsExt,
    path::Path,
};

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::{OnlineContextSource, Result, VoiceError};

mod access;
mod memory;
mod native;
mod process;

use native::Handle;

const EXECUTABLE_SHA256: [u8; 32] = [
    0x7d, 0x81, 0x04, 0x4d, 0xf4, 0x82, 0x7d, 0x21, 0xa1, 0xf2, 0x32, 0xd2, 0xeb, 0x42, 0xd3, 0x7d,
    0x5a, 0xc8, 0x76, 0x48, 0xab, 0xfb, 0x40, 0x6a, 0x34, 0xf0, 0x27, 0xe4, 0xe7, 0x4c, 0xa3, 0x5b,
];
const SESSION_SINGLETON_RVA: usize = 0x191cd28;

pub(super) fn sources() -> Result<Vec<OnlineContextSource>> {
    let mut sources = Vec::new();
    for process_id in process::candidate_process_ids()? {

        let Ok(process) = Handle::process(process_id, false) else {
            continue;
        };
        if !access::same_user_and_session(&process).unwrap_or(false) {
            continue;
        }
        let Ok(path) = process::image_path(&process) else {
            continue;
        };
        sources.push(OnlineContextSource {
            process_id,
            label: format!(
                "Synthesizer V Studio · PID {process_id} · {}",
                path.display()
            ),
        });
    }
    sources.sort_by_key(|source| source.process_id);
    Ok(sources)
}

pub(super) fn read_field(process_id: u32) -> Result<Zeroizing<Vec<u8>>> {
    let process = Handle::process(process_id, true)?;
    if !access::same_user_and_session(&process)? {
        return Err(VoiceError::new(
            "只能从当前 Windows 用户、当前会话的原编辑器导入模型上下文。",
        ));
    }
    let path = process::image_path(&process)?;

    let _verified_executable = verify_executable(&path)?;
    let (base, size) = process::main_module(process_id, &path)?;
    if SESSION_SINGLETON_RVA + size_of::<u64>() > size {
        return Err(VoiceError::new("原编辑器的主模块范围不符合已验证版本。"));
    }
    let slot = base
        .checked_add(SESSION_SINGLETON_RVA)
        .ok_or_else(|| VoiceError::new("原编辑器的主模块地址无效。"))?;
    let field = memory::session_field(&process, slot)?;

    if process::main_module(process_id, &path)? != (base, size)
        || !access::same_user_and_session(&process)?
    {
        return Err(VoiceError::new(
            "原编辑器进程在导入期间发生变化，请重新选择来源。",
        ));
    }
    Ok(field)
}

fn verify_executable(path: &Path) -> Result<File> {
    const FILE_SHARE_READ: u32 = 1;
    const MAXIMUM_EXECUTABLE_BYTES: u64 = 512 * 1024 * 1024;
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)?;
    if file.metadata()?.len() > MAXIMUM_EXECUTABLE_BYTES {
        return Err(VoiceError::new(
            "所选原编辑器的可执行文件大小超出已支持范围。",
        ));
    }
    let mut hash = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    if hash.finalize()[..] != EXECUTABLE_SHA256 {
        return Err(VoiceError::new(
            "所选原编辑器版本尚未支持上下文导入；当前只支持已验证的 Windows x64 2.2.1。",
        ));
    }
    Ok(file)
}
