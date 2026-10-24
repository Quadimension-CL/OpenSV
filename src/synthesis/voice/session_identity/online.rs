use std::path::PathBuf;

use super::{Result, VoiceError};

#[cfg(all(windows, target_arch = "x86_64"))]
mod windows;

#[derive(Clone, Debug)]
pub struct OnlineContextSource {
    pub process_id: u32,
    pub label: String,
}

pub const fn supports_online_import() -> bool {
    cfg!(all(windows, target_arch = "x86_64"))
}

pub fn online_context_sources() -> Result<Vec<OnlineContextSource>> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        windows::sources()
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        Err(unsupported_platform())
    }
}

pub fn import_online_context(
    voice_name: &str,
    model_version: u8,
    process_id: u32,
) -> Result<PathBuf> {
    super::validate_voice_name(voice_name)?;
    super::model_suffix(model_version)?;
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        let raw = windows::read_field(process_id)?;
        let identity = super::derive_local_identity(&raw, voice_name, model_version)?;
        drop(raw);
        super::save_imported_identity(voice_name, model_version, identity)
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = process_id;
        Err(unsupported_platform())
    }
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn unsupported_platform() -> VoiceError {
    VoiceError::new("在线模型上下文导入目前仅支持 Windows x64。")
}
