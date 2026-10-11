//! Parameterized CPU activations.

use crate::synthesis::inference::matrix::{MatrixError, check_length};

pub const LEAKY_RELU_TYPE_ID: [u8; 16] = [
    0x0f, 0x6d, 0x94, 0x93, 0xcd, 0xc6, 0x8f, 0x96, 0x35, 0x3c, 0xd3, 0x39, 0xb8, 0x19, 0xdb, 0x8c,
];

/// Leaky ReLU with a finite, nonnegative f32 slope for nonpositive inputs.
#[derive(Clone, Copy, Debug)]
pub struct LeakyRelu {
    negative_slope: f32,
}

impl LeakyRelu {
    pub fn from_payload(payload: &[u8]) -> Result<Self, MatrixError> {
        check_length("leaky ReLU configuration", 4, payload.len())?;
        Self::new(f32::from_le_bytes(payload.try_into().unwrap()))
    }

    pub fn new(negative_slope: f32) -> Result<Self, MatrixError> {
        // Accept finite positive values, subnormals, and ±0.
        if !negative_slope.is_finite() || negative_slope < 0.0 {
            return Err(MatrixError::InvalidScale);
        }
        Ok(Self { negative_slope })
    }

    pub fn execute(&self, input: &[f32], output: &mut [f32]) -> Result<(), MatrixError> {
        check_length("leaky ReLU output", input.len(), output.len())?;
        for (&value, target) in input.iter().zip(output.iter_mut()) {
            // The comparison is strictly 0 < x. In particular, ±0 and
            // NaN select the parameter, then still perform multiplication.
            let multiplier = if value > 0.0 {
                1.0
            } else {
                self.negative_slope
            };
            *target = multiplier * value;
        }
        Ok(())
    }
}
