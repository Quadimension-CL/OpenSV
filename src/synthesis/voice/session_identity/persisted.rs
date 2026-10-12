//! Read only an explicitly selected session file; never persist its contents.

mod smbios;

use blowfish::{
    BlowfishLE,
    cipher::{Block, BlockCipherDecrypt, KeyInit, zeroize::Zeroize},
};
use std::{fs::File, io::Read, path::Path};

use super::super::{Result, VoiceError};

// All error paths drop these buffers, including malformed session/firmware data.
struct PrivateBytes(Vec<u8>);

impl Drop for PrivateBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(super) fn read_session_field(path: &Path) -> Result<Option<Vec<u8>>> {
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
    if bytes.0.is_empty() || !bytes.0.len().is_multiple_of(8) {
        return Err(VoiceError::new("所选会话文件的长度无效。"));
    }
    let mut key = smbios::session_key()?;
    let cipher =
        BlowfishLE::new_from_slice(&key).expect("the session cipher uses an eight-byte key");
    key.zeroize();
    for chunk in bytes.0.as_chunks_mut::<8>().0 {
        let block: &mut Block<BlowfishLE> = chunk.into();
        cipher.decrypt_block(block);
    }
    // Padding length is stored in the final byte; other padding bytes are ignored.
    let padding = usize::from(*bytes.0.last().expect("nonempty buffer"));
    if !(1..=8).contains(&padding) || padding > bytes.0.len() {
        return Err(VoiceError::new(
            "无法解码所选会话文件，请选择本机 Synthesizer V Studio 2 保存的 session 文件。",
        ));
    }
    let payload = &bytes.0[..bytes.0.len() - padding];
    if std::str::from_utf8(payload).is_err() {
        return Err(VoiceError::new("所选会话文件包含无效的 UTF-8 数据。"));
    }
    let payload = &payload[..payload
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(payload.len())];
    let mut field_index = 0;
    let mut start = 0;
    let mut cursor = 0;
    while cursor <= payload.len() {
        if cursor == payload.len() || matches!(payload[cursor], b'\r' | b'\n') {
            let field = &payload[start..cursor];
            if field_index == 5 {
                if field.is_empty() {
                    return Err(VoiceError::new("所选会话文件的模型上下文字段为空。"));
                }
                if std::str::from_utf8(field).is_err() {
                    return Err(VoiceError::new(
                        "所选会话文件的模型上下文字段包含无效的 UTF-8 数据。",
                    ));
                }
                return Ok(Some(field.to_vec()));
            }
            field_index += 1;
            if payload.get(cursor) == Some(&b'\r') && payload.get(cursor + 1) == Some(&b'\n') {
                cursor += 1;
            }
            start = cursor + 1;
        }
        cursor += 1;
    }
    if field_index == 5 {
        // Online sessions omit the sixth field by design; this is not damage.
        Ok(None)
    } else {
        Err(VoiceError::new("所选会话文件缺少必需字段。"))
    }
}
