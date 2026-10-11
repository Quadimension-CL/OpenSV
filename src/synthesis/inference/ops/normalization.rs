//! Channel affine and layer normalization CPU operations.

use std::sync::Arc;

use crate::synthesis::inference::matrix::{MatrixError, check_length};

pub const LAYER_NORM_TYPE_ID: [u8; 16] = [
    0x9d, 0x54, 0x76, 0x11, 0x23, 0x40, 0x2e, 0xb8, 0xc4, 0xaf, 0x7f, 0x5e, 0xf3, 0xe2, 0xeb, 0xb6,
];
pub const CHANNEL_AFFINE_TYPE_ID: [u8; 16] = [
    0xcb, 0xc1, 0x1d, 0x2c, 0x8b, 0x91, 0xf3, 0x57, 0xca, 0x73, 0x2c, 0xdb, 0x0f, 0x6c, 0xbf, 0x51,
];

/// Configuration stores one normalized dimension as a u16.
#[derive(Clone, Copy, Debug)]
pub struct LayerNormConfig {
    pub channels: usize,
    pub epsilon: f32,
    pub affine: bool,
}

impl LayerNormConfig {
    pub fn parse(payload: &[u8]) -> Result<Self, MatrixError> {
        check_length("layer normalization configuration", 8, payload.len())?;
        if payload[5] != 1 {
            return Err(MatrixError::InvalidShape);
        }
        let config = Self {
            channels: usize::from(u16::from_le_bytes([payload[6], payload[7]])),
            epsilon: f32::from_le_bytes(payload[..4].try_into().unwrap()),
            affine: payload[4] != 0,
        };
        validate_dimensions(config.channels, config.epsilon)?;
        Ok(config)
    }
}

#[derive(Clone)]
pub struct LayerNorm {
    channels: usize,
    epsilon: f32,
    gain: Option<Arc<[f32]>>,
    bias: Option<Arc<[f32]>>,
}

impl LayerNorm {
    pub fn new(
        channels: usize,
        epsilon: f32,
        gain: Option<Arc<[f32]>>,
        bias: Option<Arc<[f32]>>,
    ) -> Result<Self, MatrixError> {
        validate_dimensions(channels, epsilon)?;
        if let Some(values) = &gain {
            check_length("layer normalization gain", channels, values.len())?;
        }
        if let Some(values) = &bias {
            check_length("layer normalization bias", channels, values.len())?;
        }
        Ok(Self {
            channels,
            epsilon,
            gain,
            bias,
        })
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Computes variance from the sum and squared sum rather than a second
    /// centered pass. Input and output use separate buffers.
    pub fn execute_batch(
        &self,
        input: &[f32],
        output: &mut [f32],
        frames: usize,
    ) -> Result<(), MatrixError> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("op.layer_norm");
        validate_buffers(self.channels, frames, input.len(), output.len())?;
        let reciprocal_channels = 1.0 / self.channels as f32;
        for (input, output) in input
            .chunks_exact(self.channels)
            .zip(output.chunks_exact_mut(self.channels))
        {
            let (sum, squares) = baseline_moments(input);
            let mean = sum * reciprocal_channels;
            // Preserve this order: scale squares, add epsilon, subtract mean squared, sqrt.
            let std = ((squares * reciprocal_channels + self.epsilon) - mean * mean).sqrt();
            let reciprocal_std = 1.0 / std;
            let vector_end = self.channels & !3;
            for (channel, (&value, target)) in input.iter().zip(output.iter_mut()).enumerate() {
                let centered = value - mean;
                let gain = self.gain.as_ref().map(|values| values[channel]);
                let bias = self.bias.as_ref().map(|values| values[channel]);
                let mut normalized = if channel < vector_end {
                    // Multiply gain by reciprocal_std before applying the centered value.
                    gain.map_or(centered * reciprocal_std, |g| {
                        (g * reciprocal_std) * centered
                    })
                } else {
                    let scaled = gain.map_or(centered, |g| centered * g);
                    if channel == vector_end
                        && self.channels & 1 != 0
                        && (gain.is_some() || bias.is_some())
                    {
                        scaled / std
                    } else {
                        scaled * reciprocal_std
                    }
                };
                if let Some(bias) = bias {
                    normalized += bias;
                }
                *target = normalized;
            }
        }
        Ok(())
    }
}

/// Applies y = x * gain + bias without computing statistics.
/// The two parameter vectors are stored in gain/bias order.
#[derive(Clone)]
pub struct ChannelAffine {
    gain: Arc<[f32]>,
    bias: Arc<[f32]>,
}

impl ChannelAffine {
    pub fn new(gain: Arc<[f32]>, bias: Arc<[f32]>) -> Result<Self, MatrixError> {
        if gain.is_empty() {
            return Err(MatrixError::InvalidShape);
        }
        check_length("channel affine bias", gain.len(), bias.len())?;
        Ok(Self { gain, bias })
    }

    pub fn channels(&self) -> usize {
        self.gain.len()
    }

    pub fn execute_batch(
        &self,
        input: &[f32],
        output: &mut [f32],
        frames: usize,
    ) -> Result<(), MatrixError> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("op.channel_affine");
        validate_buffers(self.channels(), frames, input.len(), output.len())?;
        for (input, output) in input
            .chunks_exact(self.channels())
            .zip(output.chunks_exact_mut(self.channels()))
        {
            for (channel, (&value, target)) in input.iter().zip(output.iter_mut()).enumerate() {
                *target = value * self.gain[channel] + self.bias[channel];
            }
        }
        Ok(())
    }
}

fn validate_dimensions(channels: usize, epsilon: f32) -> Result<(), MatrixError> {
    if channels == 0 {
        return Err(MatrixError::InvalidShape);
    }
    if !epsilon.is_finite() || epsilon < 0.0 {
        return Err(MatrixError::InvalidScale);
    }
    Ok(())
}

fn validate_buffers(
    channels: usize,
    frames: usize,
    input: usize,
    output: usize,
) -> Result<(), MatrixError> {
    let length = channels
        .checked_mul(frames)
        .ok_or(MatrixError::InvalidShape)?;
    check_length("channel operator input", length, input)?;
    check_length("channel operator output", length, output)
}

fn baseline_moments(input: &[f32]) -> (f32, f32) {
    let mut sums = [0.0_f32; 8];
    let mut squares = [0.0_f32; 8];
    let (chunks, remainder) = input.as_chunks::<8>();
    for chunk in chunks {
        for lane in 0..8 {
            sums[lane] += chunk[lane];
            squares[lane] += chunk[lane] * chunk[lane];
        }
    }
    let reduce = |lanes: [f32; 8]| {
        let a = lanes[4] + lanes[0];
        let b = lanes[5] + lanes[1];
        let c = lanes[6] + lanes[2];
        let d = lanes[7] + lanes[3];
        (d + b) + (c + a)
    };
    let mut sum = reduce(sums);
    let mut square_sum = reduce(squares);
    for &value in remainder {
        sum += value;
        square_sum += value * value;
    }
    (sum, square_sum)
}
