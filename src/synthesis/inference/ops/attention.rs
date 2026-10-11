//! Projected multi-head attention.
//!
//! Children are ordered as Q, K, V affine projections, optional extra K/V
//! vectors, output projection, and attention core. This implementation supports
//! four projections and rejects extra bias/zero tokens.

use std::fmt;

use crate::synthesis::inference::affine::Affine;

use super::attention_relative::{AttentionOptions, RelativeAttention};

pub const MULTI_HEAD_ATTENTION_TYPE_ID: [u8; 16] = [
    0x6e, 0xf1, 0xa8, 0x15, 0xba, 0x55, 0xa5, 0x27, 0x62, 0x77, 0x3c, 0xaf, 0x41, 0x1a, 0x0a, 0x42,
];

/// Configuration encoded as eight little-endian u32 values.
#[derive(Clone, Copy, Debug)]
pub struct AttentionConfig {
    pub flags: u32,
    pub query_channels: usize,
    pub key_channels: usize,
    pub value_channels: usize,
    pub output_channels: usize,
    pub key_head_channels: usize,
    pub value_head_channels: usize,
    pub heads: usize,
}

#[derive(Clone, Debug)]
pub enum MultiHeadAttentionError {
    Configuration(&'static str),
    UnsupportedFlags(u32),
    ProjectionShape(&'static str),
    InputShape(&'static str),
    Operator(String),
}

impl fmt::Display for MultiHeadAttentionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => write!(f, "Attention configuration: {message}"),
            Self::UnsupportedFlags(flags) => write!(
                f,
                "Attention flags {flags:#x} require an unimplemented projection/token variant"
            ),
            Self::ProjectionShape(name) => write!(
                f,
                "Attention {name} projection dimensions differ from the configuration"
            ),
            Self::InputShape(name) => write!(f, "Attention {name} input has an invalid shape"),
            Self::Operator(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for MultiHeadAttentionError {}

impl AttentionConfig {
    pub fn from_payload(payload: &[u8]) -> Result<Self, MultiHeadAttentionError> {
        if payload.len() != 32 {
            return Err(MultiHeadAttentionError::Configuration("expected 32 bytes"));
        }
        let mut words = [0_u32; 8];
        for (word, bytes) in words.iter_mut().zip(payload.as_chunks::<4>().0.iter()) {
            *word = u32::from_le_bytes(*bytes);
        }
        let config = Self {
            flags: words[0],
            query_channels: words[1] as usize,
            key_channels: words[2] as usize,
            value_channels: words[3] as usize,
            output_channels: words[4] as usize,
            key_head_channels: words[5] as usize,
            value_head_channels: words[6] as usize,
            heads: words[7] as usize,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(self) -> Result<(), MultiHeadAttentionError> {
        // Bits 3, 4, 5 and 6 enable Q/K/V/output projections. Bit 2 is
        // preserved in the configuration but does not affect execution.
        if !matches!(self.flags, 0x78 | 0x7c) {
            return Err(MultiHeadAttentionError::UnsupportedFlags(self.flags));
        }
        if [
            self.query_channels,
            self.key_channels,
            self.value_channels,
            self.output_channels,
            self.key_head_channels,
            self.value_head_channels,
            self.heads,
        ]
        .contains(&0)
        {
            return Err(MultiHeadAttentionError::Configuration("zero dimension"));
        }
        if self.heads.checked_mul(self.key_head_channels).is_none()
            || self.heads.checked_mul(self.value_head_channels).is_none()
        {
            return Err(MultiHeadAttentionError::Configuration("dimension overflow"));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct MultiHeadAttention {
    config: AttentionConfig,
    query: Affine,
    key: Affine,
    value: Affine,
    output: Affine,
    attention: RelativeAttention,
}

impl MultiHeadAttention {
    pub fn new(
        config: AttentionConfig,
        query: Affine,
        key: Affine,
        value: Affine,
        output: Affine,
        attention: RelativeAttention,
    ) -> Result<Self, MultiHeadAttentionError> {
        config.validate()?;
        let keys = config.heads * config.key_head_channels;
        let values = config.heads * config.value_head_channels;
        for (name, projection, inputs, outputs) in [
            ("query", &query, config.query_channels, keys),
            ("key", &key, config.key_channels, keys),
            ("value", &value, config.value_channels, values),
            ("output", &output, values, config.output_channels),
        ] {
            if projection.input_channels() != inputs || projection.output_channels() != outputs {
                return Err(MultiHeadAttentionError::ProjectionShape(name));
            }
        }
        Ok(Self {
            config,
            query,
            key,
            value,
            output,
            attention,
        })
    }

    pub fn config(&self) -> AttentionConfig {
        self.config
    }

    /// Contiguous frames containing all heads. Queries may have a different
    /// frame count from keys/values (the streaming wrapper uses this).
    pub fn execute(
        &self,
        queries: &[f32],
        keys: &[f32],
        values: &[f32],
        options: AttentionOptions<'_>,
    ) -> Result<Vec<f32>, MultiHeadAttentionError> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("op.attention");
        let query_frames = frames(queries, self.config.query_channels, "query")?;
        let key_frames = frames(keys, self.config.key_channels, "key")?;
        if frames(values, self.config.value_channels, "value")? != key_frames {
            return Err(MultiHeadAttentionError::InputShape(
                "key/value frame counts",
            ));
        }
        let queries = project(&self.query, queries, query_frames)?;
        let keys = project(&self.key, keys, key_frames)?;
        let values = project(&self.value, values, key_frames)?;
        let context = self
            .attention
            .execute(
                &queries,
                &keys,
                &values,
                self.config.heads,
                self.config.key_head_channels,
                self.config.value_head_channels,
                options,
            )
            .map_err(|error| MultiHeadAttentionError::Operator(error.to_string()))?;
        project(&self.output, &context, query_frames)
    }
}

fn frames(
    input: &[f32],
    channels: usize,
    name: &'static str,
) -> Result<usize, MultiHeadAttentionError> {
    if !input.len().is_multiple_of(channels) {
        return Err(MultiHeadAttentionError::InputShape(name));
    }
    Ok(input.len() / channels)
}

fn project(
    operator: &Affine,
    input: &[f32],
    frames: usize,
) -> Result<Vec<f32>, MultiHeadAttentionError> {
    let size = frames.checked_mul(operator.output_channels()).ok_or(
        MultiHeadAttentionError::Configuration("output size overflow"),
    )?;
    let mut output = vec![0.0; size];
    operator
        .execute_batch(input, &mut output, frames)
        .map_err(|error| MultiHeadAttentionError::Operator(error.to_string()))?;
    Ok(output)
}
