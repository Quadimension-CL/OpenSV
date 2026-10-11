//! Read only the existing installation's session identity; never persist its contents.

mod smbios;

use blowfish::{
    BlowfishLE,
    cipher::{Block, BlockCipherDecrypt, KeyInit, zeroize::Zeroize},
};
use std::{fs, path::PathBuf};

use super::super::{Result, VoiceError};

// All error paths drop these buffers, including malformed session/firmware data.
struct PrivateBytes(Vec<u8>);

impl Drop for PrivateBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(super) fn read_session_field() -> Result<Option<Vec<u8>>> {
    let path = session_path()?;
    let mut bytes = PrivateBytes(fs::read(path).map_err(|error| {
        VoiceError::new(format!("Cannot read the installed voice session: {error}"))
    })?);
    if bytes.0.is_empty() || !bytes.0.len().is_multiple_of(8) {
        return Err(VoiceError::new(
            "The installed voice session has invalid length",
        ));
    }
    let mut key = smbios::session_key()?;
    let cipher = BlowfishLE::new_from_slice(&key)
        .expect("the recovered session cipher uses an eight-byte key");
    key.zeroize();
    for chunk in bytes.0.as_chunks_mut::<8>().0 {
        let block: &mut Block<BlowfishLE> = chunk.into();
        cipher.decrypt_block(block);
    }
    // Padding length is stored in the final byte; other padding bytes are ignored.
    let padding = usize::from(*bytes.0.last().expect("nonempty buffer"));
    if !(1..=8).contains(&padding) || padding > bytes.0.len() {
        return Err(VoiceError::new(
            "The installed voice session could not be decoded",
        ));
    }
    let payload = &bytes.0[..bytes.0.len() - padding];
    if std::str::from_utf8(payload).is_err() {
        return Err(VoiceError::new(
            "The installed voice session contains invalid UTF-8",
        ));
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
                    return Err(VoiceError::new(
                        "The installed voice session identity field is empty",
                    ));
                }
                if std::str::from_utf8(field).is_err() {
                    return Err(VoiceError::new(
                        "The installed voice session identity field contains invalid UTF-8",
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
        Err(VoiceError::new(
            "The installed voice session is missing required fixed fields",
        ))
    }
}

fn session_path() -> Result<PathBuf> {
    Ok(roaming_directory()?
        .join("Dreamtonics")
        .join("Synthesizer V Studio 2")
        .join("license")
        .join("session"))
}

/// Returns the Windows roaming application-data root without creating folders.
#[cfg(windows)]
pub(super) fn roaming_directory() -> Result<PathBuf> {
    use std::{ffi::c_void, os::windows::ffi::OsStringExt, ptr};

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHGetSpecialFolderPathW(
            window: *mut c_void,
            path: *mut u16,
            folder: i32,
            create: i32,
        ) -> i32;
    }

    let mut buffer = [0u16; 260];
    // SAFETY: The buffer has MAX_PATH characters. CSIDL_APPDATA is read only;
    // create=false ensures this lookup cannot create application directories.
    if unsafe { SHGetSpecialFolderPathW(ptr::null_mut(), buffer.as_mut_ptr(), 0x1a, 0) } == 0 {
        return Err(VoiceError::new(
            "The roaming application-data directory is unavailable",
        ));
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .ok_or_else(|| VoiceError::new("The application-data directory path is invalid"))?;
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..end])))
}

#[cfg(not(windows))]
pub(super) fn roaming_directory() -> Result<PathBuf> {
    Err(VoiceError::new(
        "Installed voice sessions are currently supported on Windows only",
    ))
}
