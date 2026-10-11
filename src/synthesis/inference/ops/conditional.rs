//! Conditional residual/skip stack configuration.
//!
//! The three channel counts configure parallel branches. Execution lives in
//! runtime/composites/conditional; the child lists are not sequential containers.

pub const CONDITIONAL_TYPE_ID: [u8; 16] = [
    0x86, 0xa9, 0xbc, 0x79, 0xa0, 0x11, 0xcf, 0x4c, 0xe7, 0xaf, 0xa9, 0xf0, 0xec, 0xda, 0xc1, 0xd1,
];

#[derive(Clone, Copy, Debug)]
pub struct ConditionalConfig {
    pub input_channels: usize,
    pub output_channels: usize,
    pub condition_channels: usize,
}

impl ConditionalConfig {
    pub fn parse(payload: &[u8]) -> Result<Self, String> {
        if payload.len() != 12 {
            return Err("Conditional stack requires three u32 channel counts".into());
        }
        let words: Vec<_> = payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| u32::from_le_bytes(*bytes))
            .collect();
        if words.iter().any(|&value| value > i32::MAX as u32) || words[0] == 0 || words[1] == 0 {
            return Err("Conditional stack has invalid channel counts".into());
        }
        Ok(Self {
            input_channels: words[0] as usize,
            output_channels: words[1] as usize,
            condition_channels: words[2] as usize,
        })
    }
}
