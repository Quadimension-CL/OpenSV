use super::{Result, VoiceError};
use std::path::{Path, PathBuf};

pub const fn supports_session_import() -> bool {
    cfg!(all(windows, target_arch = "x86_64"))
}

/// Reads an explicitly selected session file and saves the derived condition
/// for this voice in RynthV's context file.
pub fn import_session_context(voice_name: &str, model_version: u8, path: &Path) -> Result<PathBuf> {
    super::validate_voice_name(voice_name)?;
    super::model_suffix(model_version)?;
    if path.as_os_str().is_empty() {
        return Err(VoiceError::new(
            "请先选择 Synthesizer V Studio 2 的本地会话文件。",
        ));
    }
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        let field = super::persisted::read_session_field(path)
            .map_err(|error| VoiceError::new(format!("无法从所选会话文件读取模型上下文：{error}")))?
            .ok_or_else(|| {
                VoiceError::new(
                    "所选会话文件未保存可用的模型上下文，请选择“从原版获取”、上下文文件或手动输入。",
                )
            })?;
        let field = zeroize::Zeroizing::new(field);
        let identity = super::derive_local_identity(&field, voice_name, model_version)?;
        drop(field);
        super::save_imported_identity(voice_name, model_version, identity)
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        Err(VoiceError::new(
            "本地会话文件导入仅支持 Windows x64；当前平台可导入上下文文件或手动输入。",
        ))
    }
}
