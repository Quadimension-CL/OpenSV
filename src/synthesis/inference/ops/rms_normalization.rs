use crate::synthesis::inference::matrix::{MatrixError, check_length};
use std::sync::Arc;

pub const RMS_NORM_TYPE_ID: [u8; 16] = [
    0x81, 0x22, 0x33, 0xb4, 0xd8, 0xf7, 0x67, 0x9d, 0x9b, 0xc8, 0x6c, 0xaf, 0xa2, 0x22, 0x0e, 0x25,
];

#[derive(Clone)]
pub struct RmsNorm {
    channels: usize,
    epsilon: f32,
    gain: Arc<[f32]>,
}
impl RmsNorm {
    pub fn new(payload: &[u8], gain: Arc<[f32]>) -> Result<Self, MatrixError> {
        check_length("RMS normalization configuration", 8, payload.len())?;

        if payload[4] != 1 || payload[5] != 1 {
            return Err(MatrixError::InvalidShape);
        }
        let channels = u16::from_le_bytes([payload[6], payload[7]]) as usize;
        let epsilon = f32::from_le_bytes(payload[..4].try_into().unwrap());
        if channels == 0 || !epsilon.is_finite() || epsilon < 0.0 {
            return Err(MatrixError::InvalidShape);
        }
        check_length("RMS normalization gain", channels, gain.len())?;
        if gain.iter().any(|v| !v.is_finite()) {
            return Err(MatrixError::InvalidScale);
        }
        Ok(Self {
            channels,
            epsilon,
            gain,
        })
    }
    pub fn channels(&self) -> usize {
        self.channels
    }
    pub fn execute_batch(
        &self,
        input: &[f32],
        output: &mut [f32],
        frames: usize,
    ) -> Result<(), MatrixError> {
        let length = frames
            .checked_mul(self.channels)
            .ok_or(MatrixError::InvalidShape)?;
        check_length("RMS normalization input", length, input.len())?;
        check_length("RMS normalization output", length, output.len())?;
        for (input, output) in input
            .chunks_exact(self.channels)
            .zip(output.chunks_exact_mut(self.channels))
        {
            let mut squares = 0.0_f32;
            for value in input {
                squares += value * value;
            }
            let scale = 1.0 / (squares / self.channels as f32 + self.epsilon).sqrt();
            for ((&value, &gain), out) in input.iter().zip(self.gain.iter()).zip(output) {
                *out = (value * scale) * gain;
            }
        }
        Ok(())
    }
}
