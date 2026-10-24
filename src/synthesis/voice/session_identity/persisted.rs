mod evaluation;
mod smbios;
#[cfg(test)]
mod session_retry_tests;

use blowfish::{
    BlowfishLE,
    cipher::{Block, BlockCipherDecrypt, KeyInit, zeroize::Zeroize},
};
use std::{fs::File, io::Read, path::Path};

use super::super::{Result, VoiceError};
use super::session::{SessionCondition, SessionProtection};

struct PrivateBytes(Vec<u8>);

impl Drop for PrivateBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(super) fn read_session_identity(
    path: &Path,
    voice_name: &str,
    model_version: u8,
    manual_key: Option<u64>,
) -> Result<SessionCondition> {
    const MAX_SESSION_BYTES: u64 = 1024 * 1024;
    let file = File::open(path)
        .map_err(|error| VoiceError::new(format!("无法打开所选会话文件：{error}")))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_SESSION_BYTES {
        return Err(VoiceError::new("请选择大小不超过 1 MiB 的会话文件。"));
    }
    let mut bytes = PrivateBytes(Vec::new());
    file.take(MAX_SESSION_BYTES + 1)
        .read_to_end(&mut bytes.0)
        .map_err(|error| VoiceError::new(format!("无法读取所选会话文件：{error}")))?;
    if bytes.0.len() as u64 > MAX_SESSION_BYTES {
        return Err(VoiceError::new("所选会话文件超过 1 MiB。"));
    }
    if bytes.0.is_empty() {
        return Err(VoiceError::new("所选会话文件的长度无效。"));
    }

    if evaluation::is_plaintext(&bytes.0) {
        return evaluation::identity(&bytes.0, voice_name, model_version)
            .map(|identity| SessionCondition { identity, protection: SessionProtection::Plaintext });
    }
    if !bytes.0.len().is_multiple_of(8) {
        return Err(VoiceError::new("所选会话文件的长度无效。"));
    }
    if let Ok(payload) = decode(&bytes.0, &[0; 8]) {
        if evaluation::is_session(&payload.0) || evaluation::is_plaintext(&payload.0) {
            return evaluation::identity(&payload.0, voice_name, model_version)
                .map(|identity| SessionCondition { identity, protection: SessionProtection::ZeroKey });
        }
    }
    if let Ok(mut key) = smbios::session_key() {
        let result = decode(&bytes.0, &key);
        let zero_key = key == [0; 8];
        key.zeroize();
        if let Ok(payload) = result {
            if evaluation::is_session(&payload.0) || evaluation::is_plaintext(&payload.0) {
                let identity = if zero_key {
                    evaluation::identity(&payload.0, voice_name, model_version)?
                } else {
                    super::derive_local_identity(evaluation::descriptor(&payload.0, voice_name)?, voice_name, model_version)?
                };
                return Ok(SessionCondition { identity, protection: if zero_key {SessionProtection::ZeroKey} else {SessionProtection::MachineKey} });
            }
        }
    }
    if let Some(key) = manual_key {
        let mut key_bytes = key.to_le_bytes();
        let result = decode(&bytes.0, &key_bytes);
        key_bytes.zeroize();
        if let Ok(payload) = result
            && (evaluation::is_session(&payload.0) || evaluation::is_plaintext(&payload.0))
        {
            let identity = if key == 0 {
                evaluation::identity(&payload.0, voice_name, model_version)?
            } else {
                super::derive_local_identity(evaluation::descriptor(&payload.0, voice_name)?, voice_name, model_version)?
            };
            return Ok(SessionCondition { identity, protection: if key == 0 {SessionProtection::ZeroKey} else {SessionProtection::ManualKey} });
        }
    }
    Err(VoiceError::decryption_exhausted())
}

fn decode(input: &[u8], key: &[u8; 8]) -> Result<PrivateBytes> {
    let mut bytes = PrivateBytes(input.to_vec());
    let cipher = BlowfishLE::new_from_slice(key)
        .map_err(|_| VoiceError::new("Unable to load session."))?;
    for chunk in bytes.0.as_chunks_mut::<8>().0 {
        let block: &mut Block<BlowfishLE> = chunk.into();
        cipher.decrypt_block(block);
    }
    let padding = if matches!(bytes.0.last(), Some(b'\n' | 0)) { 0 } else {
        let padding = usize::from(*bytes.0.last().ok_or_else(|| VoiceError::new("Unable to load session."))?);
        if !(1..=8).contains(&padding) || padding > bytes.0.len()
            || !bytes.0[bytes.0.len()-padding..].iter().all(|&v| usize::from(v) == padding) {
            return Err(VoiceError::new("Unable to load session."));
        }
        padding
    };
    let end = bytes.0.len()-padding;
    bytes.0[end..].zeroize();
    bytes.0.truncate(end);
    if let Some(end) = bytes.0.iter().position(|&v| v == 0) {
        bytes.0[end..].zeroize();
        bytes.0.truncate(end);
    }
    Ok(bytes)
}
