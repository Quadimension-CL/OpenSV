//! Windows JUCE unique-device source used to derive the session file key.
//! This is separate from the SID/CPUID input used by the duration identity hash.

use super::{PrivateBytes, Result, VoiceError};

pub(super) fn session_key() -> Result<[u8; 8]> {
    let firmware = firmware_table()?;
    if firmware.0.len() < 8 {
        return Err(invalid_firmware());
    }
    let length =
        u32::from_le_bytes(firmware.0[4..8].try_into().expect("four-byte length")) as usize;
    let mut remaining = firmware.0.get(8..8 + length).ok_or_else(invalid_firmware)?;
    let mut hash = 0u64;
    while !remaining.is_empty() {
        if remaining.len() < 4 {
            return Err(invalid_firmware());
        }
        let formatted_length = usize::from(remaining[1]);
        if formatted_length < 4 || formatted_length > remaining.len() {
            return Err(invalid_firmware());
        }
        let mut cursor = formatted_length;
        let mut strings = Vec::new();
        loop {
            let tail = remaining.get(cursor..).ok_or_else(invalid_firmware)?;
            let length = tail
                .iter()
                .position(|byte| *byte == 0)
                .ok_or_else(invalid_firmware)?;
            if length == 0 {
                cursor = (formatted_length + 2).max(cursor + 1).min(remaining.len());
                break;
            }
            strings.push(&tail[..length]);
            cursor += length + 1;
        }
        let field = |offset: usize| -> &[u8] {
            remaining
                .get(offset)
                .and_then(|index| usize::from(*index).checked_sub(1))
                .and_then(|index| strings.get(index).copied())
                .unwrap_or_default()
        };
        match remaining[0] {
            1 => {
                append_line(&mut hash, field(4));
                append_line(&mut hash, field(5));
                // Iterate the stored 16 bytes, not GUID field order.
                if remaining.len() > 0x18 {
                    const HEX: &[u8; 16] = b"0123456789ABCDEF";
                    for byte in &remaining[8..24] {
                        hash = hash
                            .wrapping_mul(101)
                            .wrapping_add(u64::from(HEX[(byte >> 4) as usize]));
                        hash = hash
                            .wrapping_mul(101)
                            .wrapping_add(u64::from(HEX[(byte & 15) as usize]));
                    }
                }
                hash = hash.wrapping_mul(101).wrapping_add(10);
            }
            2 => {
                for offset in 4..=8 {
                    append_line(&mut hash, field(offset));
                }
            }
            4 => {
                for offset in [7, 0x10, 0x21, 0x22] {
                    append_line(&mut hash, field(offset));
                }
            }
            _ => {}
        }
        remaining = &remaining[cursor..];
    }
    // Format the wrapped value as signed decimal, without padding.
    let decimal = PrivateBytes((hash as i64).to_string().into_bytes());
    Ok(hash_utf8(0, &decimal.0).to_le_bytes())
}

fn append_line(hash: &mut u64, bytes: &[u8]) {
    *hash = hash_utf8(*hash, bytes).wrapping_mul(101).wrapping_add(10);
}

// Matches JUCE's UTF-8 codepoint walk, including its treatment of a malformed
// continuation sequence. Device strings are not trimmed or case-normalized.
fn hash_utf8(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut cursor = 0;
    while let Some(&first) = bytes.get(cursor).filter(|byte| **byte != 0) {
        cursor += 1;
        let mut value = u32::from(first);
        if first >= 0x80 {
            let mut mask = 0x7fu32;
            let mut bit = 0x40u32;
            let mut count = 0;
            while value & bit != 0 && bit > 8 {
                mask >>= 1;
                count += 1;
                bit >>= 1;
            }
            value &= mask;
            for _ in 0..count {
                let Some(&next) = bytes.get(cursor).filter(|byte| **byte & 0xc0 == 0x80) else {
                    break;
                };
                value = (value << 6) | u32::from(next & 0x3f);
                cursor += 1;
            }
        }
        hash = hash.wrapping_mul(101).wrapping_add(u64::from(value));
    }
    hash
}

fn invalid_firmware() -> VoiceError {
    VoiceError::new(
        "The system firmware data needed for the installed session is unavailable or invalid",
    )
}

#[cfg(windows)]
fn firmware_table() -> Result<PrivateBytes> {
    use std::{ffi::c_void, ptr};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetSystemFirmwareTable(provider: u32, table: u32, buffer: *mut c_void, size: u32)
        -> u32;
    }

    const PROVIDER: u32 = 0x52534d42;
    const TABLE: u32 = 0x52534454;
    // SAFETY: A null buffer requests only the size of the read-only firmware table.
    let size = unsafe { GetSystemFirmwareTable(PROVIDER, TABLE, ptr::null_mut(), 0) };
    if !(8..=16 * 1024 * 1024).contains(&size) {
        return Err(invalid_firmware());
    }
    let mut data = PrivateBytes(vec![0; size as usize]);
    // SAFETY: data owns exactly size writable bytes. No firmware is modified.
    let written =
        unsafe { GetSystemFirmwareTable(PROVIDER, TABLE, data.0.as_mut_ptr().cast(), size) };
    if written != size {
        return Err(invalid_firmware());
    }
    Ok(data)
}

#[cfg(not(windows))]
fn firmware_table() -> Result<PrivateBytes> {
    Err(VoiceError::new(
        "Installed voice session keys are currently supported on Windows only",
    ))
}
