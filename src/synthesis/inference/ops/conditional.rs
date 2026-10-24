pub const CONDITIONAL_TYPE_ID: [u8; 16] = [
    0x86, 0xa9, 0xbc, 0x79, 0xa0, 0x11, 0xcf, 0x4c, 0xe7, 0xaf, 0xa9, 0xf0, 0xec, 0xda, 0xc1, 0xd1,
];

#[derive(Clone, Copy, Debug)]
pub struct ConditionalConfig {
    pub input_channels: usize,
    pub output_channels: usize,
    pub condition_channels: usize,

    pub prunable_layers: usize,
}

impl ConditionalConfig {
    pub fn parse(payload: &[u8]) -> Result<Self, String> {
        if !matches!(payload.len(), 12 | 16) {
            return Err(
                "Conditional stack requires three channel counts and an optional pruning count"
                    .into(),
            );
        }
        let words: Vec<_> = payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| u32::from_le_bytes(*bytes))
            .collect();
        if words[..3].iter().any(|&value| value > i32::MAX as u32)
            || words[0] == 0
            || words[1] == 0
            || words.get(3).is_some_and(|&count| count >= 64)
        {
            return Err("Conditional stack has invalid channel counts".into());
        }
        Ok(Self {
            input_channels: words[0] as usize,
            output_channels: words[1] as usize,
            condition_channels: words[2] as usize,
            prunable_layers: words.get(3).copied().unwrap_or(0) as usize,
        })
    }
}
