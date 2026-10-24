mod common;
mod neural;
mod registry;
mod stages;
pub(crate) mod types;
mod vocoder;

use super::{Result, VoiceError, array, range, u32_at};

struct Entry {
    aliases: [u64; 12],
    id: [u8; 16],
}

pub(crate) struct Record {

    pub bytes: Vec<u8>,
    pub children: Vec<usize>,
}

struct Frame {
    record: Record,
    remaining: usize,
}

pub(crate) fn root_type(bytes: &[u8]) -> Result<[u8; 16]> {
    if u32_at(bytes, 0)? != 0x7fca00ff || !matches!(u32_at(bytes, 8)?, 0x7fca40ff | 0x7fca41ff) {
        return Err(VoiceError::new("Embedded DNNI root header is invalid"));
    }
    let alias = match u32_at(bytes, 4)? {
        1 => registry::tag_hash(&bytes[range(12, 8, bytes.len())?]),
        2 => u64::from_le_bytes(array(bytes, 12)?),
        version => {
            return Err(VoiceError::new(format!(
                "Unsupported embedded DNNI version {version}"
            )));
        }
    };
    Ok(registry::resolve(alias))
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<Record>> {
    if u32_at(bytes, 0)? != 0x7fca00ff {
        return Err(VoiceError::new("Embedded DNNI signature is invalid"));
    }
    let version = u32_at(bytes, 4)?;
    if !matches!(version, 1 | 2) {
        return Err(VoiceError::new(format!(
            "Unsupported embedded DNNI version {version}"
        )));
    }
    let mut offset = 8;
    let mut frames = vec![open(bytes, &mut offset, version)?];
    let mut records = Vec::new();
    while let Some(frame) = frames.last_mut() {
        if frame.remaining != 0 {
            frame.remaining -= 1;
            frames.push(open(bytes, &mut offset, version)?);
            continue;
        }
        let index = records.len();
        let frame = frames.pop().expect("current inline record exists");
        records.push(frame.record);
        if let Some(parent) = frames.last_mut() {
            parent.record.children.push(index);
        }
    }
    if offset != bytes.len() {
        return Err(VoiceError::new(
            "Embedded DNNI tree does not exactly consume its resource",
        ));
    }
    Ok(records)
}

fn open(bytes: &[u8], offset: &mut usize, version: u32) -> Result<Frame> {
    let header = &bytes[range(*offset, 20, bytes.len())?];
    if !matches!(u32_at(header, 0)?, 0x7fca40ff | 0x7fca41ff) {
        return Err(VoiceError::new("Embedded DNNI node marker is invalid"));
    }
    let identity = match version {
        1 => registry::tag_hash(&header[4..12]),
        2 => u64::from_le_bytes(array(header, 4)?),
        _ => unreachable!("embedded version checked"),
    };
    let stored_count = u32_at(header, 12)?;
    let children = if version == 1 {
        stored_count
    } else {
        ((identity as u32 ^ (identity >> 32) as u32 ^ stored_count) ^ 0xac5e7bd5) / 3
    } as usize;
    let length = u32_at(header, 16)? as usize;
    let payload_range = range(*offset + 20, length, bytes.len())?;
    if children > (bytes.len() - payload_range.end) / 20 {
        return Err(VoiceError::new(
            "Embedded DNNI child count exceeds its remaining records",
        ));
    }
    let payload = &bytes[payload_range.clone()];
    let mut record_bytes = Vec::with_capacity(16 + payload.len());
    record_bytes.extend_from_slice(&registry::resolve(identity));
    record_bytes.extend_from_slice(payload);
    *offset = payload_range.end;
    Ok(Frame {
        record: Record {
            bytes: record_bytes,

            children: Vec::new(),
        },
        remaining: children,
    })
}
