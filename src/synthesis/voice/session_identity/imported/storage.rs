use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

use zeroize::Zeroizing;

use super::{Context, Result, VoiceContext, VoiceError, location};

const MAX_CONTEXT_BYTES: u64 = 1024 * 1024;

pub(super) fn load_saved(path: &Path) -> Result<Option<Context>> {
    let file = match location::open_read(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    location::verify(&file, path)?;
    decode(file).map(Some)
}

pub(super) fn load_source(path: &Path) -> Result<Context> {

    decode(File::open(path)?)
}

fn decode(file: File) -> Result<Context> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_CONTEXT_BYTES {
        return Err(VoiceError::new(
            "模型上下文必须是大小不超过 1 MiB 的 JSON 文件。",
        ));
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(MAX_CONTEXT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_CONTEXT_BYTES {
        return Err(VoiceError::new("模型上下文文件超过 1 MiB 大小限制。"));
    }

    let context: Context =
        serde_json::from_slice(&bytes).map_err(|_| VoiceError::new("模型上下文文件格式无效。"))?;
    super::validate(&context)?;
    Ok(context)
}

pub(super) fn save(path: &Path, entry: VoiceContext) -> Result<()> {
    let directory = path.parent().expect("context has its RynthV directory");
    fs::create_dir_all(directory)?;

    let lock_path = path.with_extension("lock");
    let context_lock = location::open_lock(&lock_path)?;
    location::verify(&context_lock, &lock_path)?;
    context_lock.lock()?;
    let mut context = load_saved(path)?.unwrap_or_default();
    if let Some(existing) = context.voices.iter_mut().find(|existing| {
        existing.voice_name == entry.voice_name && existing.model_version == entry.model_version
    }) {
        *existing = entry;
    } else {
        context.voices.push(entry);
    }
    let bytes = Zeroizing::new(
        serde_json::to_vec_pretty(&context).map_err(|_| VoiceError::new("无法编码模型上下文。"))?,
    );
    if bytes.len() as u64 > MAX_CONTEXT_BYTES {
        return Err(VoiceError::new("模型上下文文件超过 1 MiB 大小限制。"));
    }
    let temporary = directory.join(format!("context-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = location::create_temporary(&temporary)?;
        location::verify(&file, &temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        replace(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn replace(source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(source: *const u16, target: *const u16, flags: u32) -> i32;
    }

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();

    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), 0x1 | 0x8) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace(source: &Path, target: &Path) -> Result<()> {
    fs::rename(source, target)?;

    File::open(target.parent().expect("context has a parent directory"))?.sync_all()?;
    Ok(())
}
