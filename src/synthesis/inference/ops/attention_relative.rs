//! Multi-head attention core and learned relative positions.
//!
//! This scalar implementation adds relative-key scores after QK scaling.
//! Changing the accumulation order can change floating-point results.

use std::{fmt, sync::Arc};

use crate::synthesis::inference::matrix::DenseMatrix;

pub const RELATIVE_ATTENTION_TYPE_ID: [u8; 16] = [
    0x33, 0x10, 0xce, 0x95, 0x60, 0x7a, 0x3a, 0x07, 0x63, 0xef, 0xcf, 0xe7, 0x62, 0x7e, 0x01, 0x50,
];

#[derive(Clone, Copy, Debug, Default)]
pub struct AttentionOptions<'a> {
    /// None or zero disables the local window entirely.
    pub window: Option<usize>,
    /// Added to the query index before applying the window and relative index.
    pub position_offset: i32,
    /// One float per key frame; values greater than 0.5 mark padding.
    pub key_padding_mask: Option<&'a [f32]>,
}

#[derive(Clone)]
pub struct RelativeAttention {
    positions: Option<RelativePositions>,
}

#[derive(Clone)]
struct RelativePositions {
    key_channels: usize,
    value_channels: usize,
    entries: usize,
    key: Arc<[f32]>,
    value: Arc<[f32]>,
}

#[derive(Clone, Debug)]
pub enum AttentionError {
    RelativeShape,
    InvalidDimensions,
    InvalidLength(&'static str),
    NonFiniteInput,
    Softmax(String),
}

impl fmt::Display for AttentionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RelativeShape => f.write_str(
                "Relative attention requires two float matrices with matching odd position counts",
            ),
            Self::InvalidDimensions => {
                f.write_str("Attention dimensions are empty, unsupported, or overflow")
            }
            Self::InvalidLength(name) => write!(f, "Invalid attention {name} length"),
            Self::NonFiniteInput => f.write_str("Attention input contains a non-finite value"),
            Self::Softmax(error) => write!(f, "Attention softmax: {error}"),
        }
    }
}

impl std::error::Error for AttentionError {}

impl RelativeAttention {
    /// Matrix rows are channels and columns are relative positions. Preparation
    /// transposes both matrices once into contiguous position vectors.
    pub fn new(
        key: Option<&DenseMatrix>,
        value: Option<&DenseMatrix>,
    ) -> Result<Self, AttentionError> {
        let positions = match (key, value) {
            (None, None) => None,
            (Some(key), Some(value)) => {
                if key.columns() != value.columns()
                    || key.columns().is_multiple_of(2)
                    || key.quantization().is_some()
                    || value.quantization().is_some()
                {
                    return Err(AttentionError::RelativeShape);
                }
                Some(RelativePositions {
                    key_channels: key.rows(),
                    value_channels: value.rows(),
                    entries: key.columns(),
                    key: transpose(key),
                    value: transpose(value),
                })
            }
            _ => return Err(AttentionError::RelativeShape),
        };
        Ok(Self { positions })
    }

    /// Inputs and output are frame-major with contiguous heads within a frame.
    /// Key and value frame counts must match; an optional padding mask has one
    /// float per key frame. Appended extra keys are handled by the calling stage.
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        query: &[f32],
        key: &[f32],
        value: &[f32],
        heads: usize,
        key_channels: usize,
        value_channels: usize,
        options: AttentionOptions<'_>,
    ) -> Result<Vec<f32>, AttentionError> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("kernel.relative_attention");
        let query_width = checked_width(heads, key_channels)?;
        let output_width = checked_width(heads, value_channels)?;
        if !query.len().is_multiple_of(query_width) {
            return Err(AttentionError::InvalidLength("query"));
        }
        if !key.len().is_multiple_of(query_width) {
            return Err(AttentionError::InvalidLength("key"));
        }
        let query_frames = query.len() / query_width;
        let key_frames = key.len() / query_width;
        if query_frames != 0 && key_frames == 0 {
            return Err(AttentionError::InvalidLength(
                "empty keys for nonempty queries",
            ));
        }
        if let Some(mask) = options.key_padding_mask
            && mask.len() != key_frames
        {
            return Err(AttentionError::InvalidLength("padding mask"));
        }
        if key_frames.checked_mul(output_width) != Some(value.len()) {
            return Err(AttentionError::InvalidLength("value"));
        }
        if query_frames > i32::MAX as usize || key_frames > i32::MAX as usize {
            return Err(AttentionError::InvalidDimensions);
        }
        if let Some(positions) = &self.positions
            && (positions.key_channels != key_channels
                || positions.value_channels != value_channels)
        {
            return Err(AttentionError::RelativeShape);
        }
        if query.iter().chain(key).chain(value).any(|v| !v.is_finite()) {
            return Err(AttentionError::NonFiniteInput);
        }
        let output_len = query_frames
            .checked_mul(output_width)
            .ok_or(AttentionError::InvalidDimensions)?;
        let mut output = vec![0.0; output_len];
        let mut scores = vec![0.0; key_frames];
        let scale = 1.0 / (key_channels as f32).sqrt();
        for query_index in 0..query_frames {
            for head in 0..heads {
                let query_start = query_index * query_width + head * key_channels;
                let query = &query[query_start..query_start + key_channels];
                for (key_index, score) in scores.iter_mut().enumerate() {
                    let key_start = key_index * query_width + head * key_channels;
                    *score = dot(query, &key[key_start..key_start + key_channels]) * scale;
                    if options
                        .key_padding_mask
                        .is_some_and(|mask| mask[key_index] > 0.5)
                    {
                        *score = f32::from_bits(0xd015_02f9);
                    }
                    let distance =
                        query_index as i64 + i64::from(options.position_offset) - key_index as i64;
                    if let Some(window) = options.window.filter(|&window| window != 0)
                        && distance.unsigned_abs() > window as u64
                    {
                        *score = f32::from_bits(0xd015_02f9);
                    }
                    if let Some(positions) = &self.positions {
                        let position = positions.index(distance) * key_channels;
                        // The relative dot product is not multiplied by scale.
                        *score +=
                            relative_dot(query, &positions.key[position..position + key_channels]);
                    }
                }
                super::softmax::execute(&mut scores)
                    .map_err(|error| AttentionError::Softmax(error.to_string()))?;
                let output_start = query_index * output_width + head * value_channels;
                let frame = &mut output[output_start..output_start + value_channels];
                for (key_index, &probability) in scores.iter().enumerate() {
                    let value_start = key_index * output_width + head * value_channels;
                    for (channel, target) in frame.iter_mut().enumerate() {
                        *target += probability * value[value_start + channel];
                    }
                }
                // Keep the second accumulation separate to preserve rounding:
                // first A*V finishes, then relative value terms are added.
                if let Some(positions) = &self.positions {
                    for (key_index, &probability) in scores.iter().enumerate() {
                        let distance = query_index as i64 + i64::from(options.position_offset)
                            - key_index as i64;
                        let position = positions.index(distance) * value_channels;
                        for (channel, target) in frame.iter_mut().enumerate() {
                            *target += probability * positions.value[position + channel];
                        }
                    }
                }
            }
        }
        Ok(output)
    }
}

impl RelativePositions {
    fn index(&self, distance: i64) -> usize {
        (distance + (self.entries / 2) as i64).clamp(0, (self.entries - 1) as i64) as usize
    }
}

fn checked_width(heads: usize, channels: usize) -> Result<usize, AttentionError> {
    heads
        .checked_mul(channels)
        .filter(|&n| n != 0)
        .ok_or(AttentionError::InvalidDimensions)
}

fn transpose(matrix: &DenseMatrix) -> Arc<[f32]> {
    let mut output = vec![0.0; matrix.rows() * matrix.columns()];
    for channel in 0..matrix.rows() {
        for (position, &value) in matrix.row(channel).iter().enumerate() {
            output[position * matrix.rows() + channel] = value;
        }
    }
    output.into()
}

fn dot(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right)
        .fold(0.0, |sum, (&a, &b)| sum + a * b)
}

/// Accumulate eight lanes, reduce them in a fixed pairwise order, then add the
/// scalar remainder. Preserve this order for consistent floating-point results.
fn relative_dot(left: &[f32], right: &[f32]) -> f32 {
    let mut lanes = [0.0; 8];
    let blocks = left.len() / 8 * 8;
    for start in (0..blocks).step_by(8) {
        for lane in 0..8 {
            lanes[lane] += left[start + lane] * right[start + lane];
        }
    }
    let mut sum = ((lanes[7] + lanes[3]) + (lanes[5] + lanes[1]))
        + ((lanes[6] + lanes[2]) + (lanes[4] + lanes[0]));
    for index in blocks..left.len() {
        sum += left[index] * right[index];
    }
    sum
}
