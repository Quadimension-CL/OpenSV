//! Explicit, read-only import from a supported running online session.
//!
//! Normal voice loading never invokes this reader. Only the derived model
//! condition reaches RynthV's context file; the original field is zeroized.

use std::path::PathBuf;

use super::{Result, VoiceError};

#[cfg(all(windows, target_arch = "x86_64"))]
mod windows;

#[derive(Clone, Debug)]
pub struct OnlineContextSource {
    pub process_id: u32,
    pub label: String,
}

/// Whether this build can explicitly read the original editor's online session.
pub const fn supports_online_import() -> bool {
    cfg!(all(windows, target_arch = "x86_64"))
}

/// Lists only original-editor processes owned by this user in this session.
/// Selecting a process does not imply that its binary version is supported;
/// the import validates the executable before reading its model context.
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

/// Performs a user-requested import for one voice from one explicitly selected
/// process. This neither modifies authorization state nor contacts a server.
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
