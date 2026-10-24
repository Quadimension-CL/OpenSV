use super::{Result, VoiceError};
use std::path::{Path, PathBuf};
use serde::Serialize;
mod retry_keys;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionProtection { Plaintext, ZeroKey, MachineKey, ManualKey }

#[derive(Clone, Copy, Debug, Serialize)]
pub struct SessionCondition {
    pub identity: u64,
    pub protection: SessionProtection,
}

pub const fn supports_session_import() -> bool {
    cfg!(all(windows, target_arch = "x86_64"))
}

pub fn import_session_context(voice_name: &str, model_version: u8, path: &Path) -> Result<PathBuf> {
    let identity = resolve_imported_identity(voice_name, model_version, path)?;
    super::imported::save_session(voice_name, model_version, identity, path)
}

pub fn import_session_context_with_key(voice_name: &str, model_version: u8, path: &Path, key: u64) -> Result<PathBuf> {
    let condition = resolve_session_condition_with_key(voice_name, model_version, path, key)?;
    if condition.protection == SessionProtection::ManualKey { retry_keys::remember(path, key)?; }
    super::imported::save_session(voice_name, model_version, condition.identity, path)
}

pub(super) fn resolve_imported_identity(voice_name: &str, model_version: u8, path: &Path) -> Result<u64> {
    match resolve_session_condition(voice_name, model_version, path) {
        Err(error) if error.is_session_decryption_exhausted() => {
            if let Some(key) = retry_keys::get(path)? {
                resolve_session_condition_with_key(voice_name, model_version, path, *key).map(|c| c.identity)
            } else { Err(error) }
        }
        result => result.map(|c| c.identity),
    }
}

pub fn resolve_session_identity(voice_name: &str, model_version: u8, path: &Path) -> Result<u64> {
    resolve_session_condition(voice_name, model_version, path).map(|condition| condition.identity)
}

pub fn resolve_session_condition(voice_name: &str, model_version: u8, path: &Path) -> Result<SessionCondition> {
    resolve(voice_name, model_version, path, None)
}

pub fn resolve_session_condition_with_key(voice_name: &str, model_version: u8, path: &Path, manual_key: u64) -> Result<SessionCondition> {
    resolve(voice_name, model_version, path, Some(manual_key))
}

fn resolve(voice_name: &str, model_version: u8, path: &Path, manual_key: Option<u64>) -> Result<SessionCondition> {
    super::validate_voice_name(voice_name)?;
    super::model_suffix(model_version)?;
    if path.as_os_str().is_empty() {
        return Err(VoiceError::new(
            "请先选择 Synthesizer V Studio 2 的本地会话文件。",
        ));
    }
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        super::persisted::read_session_identity(path, voice_name, model_version, manual_key)
            .map_err(|error| error.with_context("无法从所选会话文件读取模型上下文"))
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = manual_key;
        Err(VoiceError::new(
            "本地会话文件导入仅支持 Windows x64；当前平台可导入上下文文件或手动输入。",
        ))
    }
}
