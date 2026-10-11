//! Duration-root dimensions and input feature widths.

use serde::Serialize;

use super::{Result, VoiceError, array, product, sum};

pub const DURATION_ROOT_TYPE_ID: [u8; 16] = [
    0xa0, 0xeb, 0x6b, 0x03, 0xbb, 0xf1, 0x14, 0xa8, 0xbd, 0xcb, 0x9b, 0xe3, 0x75, 0x87, 0xce, 0xe8,
];

#[derive(Clone, Copy, Debug, Serialize)]
pub struct DurationConfiguration {
    pub language_embedding_width: u16,
    pub phone_embedding_width: u16,
    pub position_embedding_width: u16,
    pub duration_encoding_width: u16,
    pub pitch_encoding_width: u16,
    /// Additional configuration fields without assigned numerical roles.
    pub unassigned_words: [u16; 3],
}

impl DurationConfiguration {
    /// Parses exactly eight little-endian u16 configuration fields.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 32 || array::<16>(bytes, 0)? != DURATION_ROOT_TYPE_ID {
            return Err(VoiceError::new(
                "Invalid duration-root type or configuration size",
            ));
        }
        let mut fields = [0_u16; 8];
        for (index, value) in fields.iter_mut().enumerate() {
            *value = u16::from_le_bytes(array(bytes, 16 + index * 2)?);
        }
        if fields[..6].contains(&0) || fields[6..].iter().any(|value| *value < 2) {
            return Err(VoiceError::new(
                "Duration-root dimensions fail constructor constraints",
            ));
        }
        Ok(Self {
            language_embedding_width: fields[0],
            phone_embedding_width: fields[1],
            position_embedding_width: fields[2],
            duration_encoding_width: fields[3],
            pitch_encoding_width: fields[4],
            unassigned_words: [fields[5], fields[6], fields[7]],
        })
    }

    /// Width of the concatenated blocks before the first projection.
    /// The constant width comes from root child 1's vector.
    pub fn feature_width(self, constant_width: usize) -> Result<usize> {
        let widths = [
            self.phone_embedding_width as usize,
            self.language_embedding_width as usize,
            product(self.position_embedding_width as usize, 2)?,
            self.duration_encoding_width as usize,
            self.pitch_encoding_width as usize,
            constant_width,
        ];
        widths.into_iter().try_fold(0, sum)
    }

    pub fn validate_input_projection(
        self,
        rows: usize,
        columns: usize,
        constant_width: usize,
    ) -> Result<()> {
        let expected = self.feature_width(constant_width)?;
        if rows == 0 || columns != expected {
            return Err(VoiceError::new(format!(
                "Duration input projection requires {expected} columns, got {rows}x{columns}"
            )));
        }
        Ok(())
    }
}
