//! Maps the twelve registered aliases to a single canonical type.

use super::{Entry, common, neural, stages, vocoder};

pub(super) fn tag_hash(tag: &[u8]) -> u64 {
    tag.iter()
        .take(7)
        .take_while(|&&byte| byte != 0)
        .fold(0x0dcd59189d5a0f24, |value, &byte| {
            (value ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
}

pub(super) fn resolve(alias: u64) -> [u8; 16] {
    let entries = [
        common::ENTRIES,
        neural::ENTRIES,
        stages::ENTRIES,
        vocoder::ENTRIES,
    ];
    if let Some(Entry { id, .. }) = entries
        .into_iter()
        .flatten()
        .find(|entry| entry.aliases.contains(&alias))
    {
        return *id;
    }
    // Keep an unknown registration distinct without dispatching to a guessed
    // operator. Model preparation will report it as unsupported.
    let mut id = [0; 16];
    id[..8].copy_from_slice(b"SV1UNKN\0");
    id[8..].copy_from_slice(&alias.to_le_bytes());
    id
}
