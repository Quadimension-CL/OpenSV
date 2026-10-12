//! Explicit RynthV model contexts, either machine-bound or manually supplied.

mod location;
mod storage;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use super::{Result, VoiceError, platform};

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Context {
    voices: Vec<VoiceContext>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VoiceContext {
    voice_name: String,
    model_version: u8,
    identity: u64,
    /// None is an explicitly supplied portable condition. Session-file
    /// and process imports remain tied to their original local user and processor.
    #[serde(default)]
    machine_fingerprint: Option<String>,
}

pub(super) fn read(voice_name: &str, model_version: u8) -> Result<Option<u64>> {
    let Some(context) = storage::load_saved(&context_path()?)? else {
        return Ok(None);
    };
    let Some(entry) = find(&context, voice_name, model_version) else {
        return Ok(None);
    };
    if let Some(fingerprint) = &entry.machine_fingerprint {
        #[cfg(not(windows))]
        {
            let _ = fingerprint;
            return Err(VoiceError::new(
                "该模型上下文绑定原 Windows 机器，请使用手动导入上下文文件将所选声库保存为便携上下文。",
            ));
        }
        #[cfg(windows)]
        if fingerprint != &machine_fingerprint()? {
            return Err(VoiceError::new(
                "模型上下文与当前 Windows 用户或处理器不匹配。请重新获取本机上下文或手动导入。",
            ));
        }
    }
    Ok(Some(entry.identity))
}

pub(super) fn read_source(voice_name: &str, model_version: u8, path: &Path) -> Result<u64> {
    let context = storage::load_source(path)?;
    find(&context, voice_name, model_version)
        .map(|entry| entry.identity)
        .ok_or_else(|| {
            VoiceError::new(format!(
                "所选文件中没有声库 {voice_name}（模型版本 {model_version}）的模型上下文记录。"
            ))
        })
}

pub(super) fn save_bound(voice_name: &str, model_version: u8, identity: u64) -> Result<PathBuf> {
    save(
        voice_name,
        model_version,
        identity,
        Some(machine_fingerprint()?),
    )
}

pub(super) fn save_manual(voice_name: &str, model_version: u8, identity: u64) -> Result<PathBuf> {
    save(voice_name, model_version, identity, None)
}

fn save(
    voice_name: &str,
    model_version: u8,
    identity: u64,
    machine_fingerprint: Option<String>,
) -> Result<PathBuf> {
    let entry = VoiceContext {
        voice_name: voice_name.to_owned(),
        model_version,
        identity,
        machine_fingerprint,
    };
    let path = context_path()?;
    storage::save(&path, entry)?;
    Ok(path)
}

fn find<'a>(context: &'a Context, voice_name: &str, model_version: u8) -> Option<&'a VoiceContext> {
    context
        .voices
        .iter()
        .find(|entry| entry.voice_name == voice_name && entry.model_version == model_version)
}

fn validate(context: &Context) -> Result<()> {
    for (index, entry) in context.voices.iter().enumerate() {
        let invalid_fingerprint = entry.machine_fingerprint.as_ref().is_some_and(|value| {
            value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
        if entry.voice_name.trim().is_empty()
            || !matches!(entry.model_version, 1 | 2)
            || invalid_fingerprint
            || context.voices[..index].iter().any(|previous| {
                previous.voice_name == entry.voice_name
                    && previous.model_version == entry.model_version
            })
        {
            return Err(VoiceError::new(
                "模型上下文记录格式无效或包含重复声库记录。",
            ));
        }
    }
    Ok(())
}

fn context_path() -> Result<PathBuf> {
    crate::platform::context_directory()
        .map(|directory| directory.join("context.json"))
        .map_err(VoiceError::new)
}

fn machine_fingerprint() -> Result<String> {
    let mut machine = platform::machine_identity()?;
    let digest = Sha256::digest(machine.as_bytes());
    machine.zeroize();
    Ok(super::super::hex(&digest))
}
