use crate::synthesis::voice::{Result, VoiceError};

/// Five base features plus four-byte tag strings.
pub(super) struct Configuration {
    pub frame_seconds: f64,
    pub tags: Vec<String>,
}

impl Configuration {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        if payload.len() < 14 {
            return Err(VoiceError::new("Note frontend configuration is truncated"));
        }
        let frame_seconds = f64::from_le_bytes(payload[..8].try_into().unwrap());
        let width = u16::from_le_bytes(payload[8..10].try_into().unwrap()) as usize;
        let base_width = u16::from_le_bytes(payload[10..12].try_into().unwrap()) as usize;
        let count = u16::from_le_bytes(payload[12..14].try_into().unwrap()) as usize;
        if !frame_seconds.is_finite()
            || frame_seconds <= 0.0
            || base_width != 5
            || width != base_width + count
            || payload.len() != 14 + count * 4
        {
            return Err(VoiceError::new("Invalid note frontend configuration"));
        }
        let mut tags = Vec::with_capacity(count);
        for chunk in payload[14..].as_chunks::<4>().0 {
            let length = chunk.iter().position(|byte| *byte == 0).unwrap_or(4);
            tags.push(
                std::str::from_utf8(&chunk[..length])
                    .map_err(|_| VoiceError::new("Note frontend tag is not UTF-8"))?
                    .to_owned(),
            );
        }
        Ok(Self {
            frame_seconds,
            tags,
        })
    }

    pub fn width(&self) -> usize {
        5 + self.tags.len()
    }
}
