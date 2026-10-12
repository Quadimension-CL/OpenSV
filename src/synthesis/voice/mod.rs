//! Strict, read-only loading of DNNI v3 and SV1 AI NOFS resources.
//!
//! Immutable parameters are shared by the native stage programs.
//! Decoded model resources are not serialized.

mod archive;
mod catalog;
mod config;
mod duration_frontend;
pub(crate) mod embedded;
mod family;
mod format;
mod inventory;
mod nofs;
mod phones;
mod preparation;
mod prepared;
pub mod session_identity;
mod stream;
mod tensor;
mod tree;

pub use catalog::{VoiceAvailability, VoiceCatalog, VoiceCatalogEntry};
pub use duration_frontend::{DURATION_ROOT_TYPE_ID, DurationConfiguration};
pub use family::VoiceFamily;
pub use inventory::VoiceInventory;
pub use phones::{PHONE_SET_TYPE_ID, PhoneLookup, PhoneSet, ResolvedPhone, resolve_explicit};
pub use preparation::{PreparationSummary, PreparedVoice};
pub use prepared::{PreparedModule, PreparedNode, PreparedTree, PreparedTreeSummary};
pub use tensor::{TensorLayout, TensorStorage};

use std::{fmt, ops::Range};

#[derive(Clone, Debug)]
pub struct VoiceError(String);

impl VoiceError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for VoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for VoiceError {}

impl From<std::io::Error> for VoiceError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, VoiceError>;

fn range(start: usize, length: usize, total: usize) -> Result<Range<usize>> {
    let end = start
        .checked_add(length)
        .filter(|end| *end <= total)
        .ok_or_else(|| VoiceError::new("DNNI byte range exceeds its buffer"))?;
    Ok(start..end)
}

fn array<const N: usize>(bytes: &[u8], start: usize) -> Result<[u8; N]> {
    Ok(bytes[range(start, N, bytes.len())?]
        .try_into()
        .expect("checked fixed-size range"))
}

fn u32_at(bytes: &[u8], start: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(array(bytes, start)?))
}

fn product(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b)
        .ok_or_else(|| VoiceError::new("DNNI size multiplication overflow"))
}

fn sum(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| VoiceError::new("DNNI size addition overflow"))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[(byte >> 4) as usize] as char);
        text.push(DIGITS[(byte & 15) as usize] as char);
    }
    text
}
mod binding;
