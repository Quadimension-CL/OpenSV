//! Dense affine reference: y = W x, followed by an optional bias addition.

mod batch;

use std::sync::Arc;

use super::matrix::{DenseMatrix, MatrixError, check_length};

pub const AFFINE_TYPE_ID: [u8; 16] = [
    0xff, 0xe7, 0x93, 0xdc, 0x27, 0x62, 0x4c, 0x3d, 0x31, 0xda, 0xe4, 0x51, 0x2f, 0x20, 0xf1, 0xc1,
];

/// CPU reference for the plain affine module.
///
/// Prepared weights and bias are shared by clones. The reference follows the
/// four-input accumulation grouping without fused multiply
/// adds; this is not a claim of bit identity with every SIMD or batch backend.
#[derive(Clone)]
pub struct Affine {
    matrix: DenseMatrix,
    bias: Option<Arc<[f32]>>,
}

impl Affine {
    pub fn new(matrix: DenseMatrix, bias: Option<Arc<[f32]>>) -> Result<Self, MatrixError> {
        if let Some(bias) = &bias {
            check_length("affine bias", matrix.rows(), bias.len())?;
        }
        Ok(Self { matrix, bias })
    }

    pub fn input_channels(&self) -> usize {
        self.matrix.columns()
    }

    pub fn output_channels(&self) -> usize {
        self.matrix.rows()
    }

    pub fn matrix(&self) -> &DenseMatrix {
        &self.matrix
    }

    pub fn bias(&self) -> Option<&[f32]> {
        self.bias.as_deref()
    }

    /// Accumulating dense operation. Existing output is
    /// the initial accumulator; the optional bias is added after the dot product.
    pub fn execute_accumulate(&self, input: &[f32], output: &mut [f32]) -> Result<(), MatrixError> {
        check_length("affine input", self.input_channels(), input.len())?;
        check_length("affine output", self.output_channels(), output.len())?;
        for (channel, target) in output.iter_mut().enumerate() {
            let mut value = baseline_dot_from(self.matrix.row(channel), input, *target);
            if let Some(bias) = &self.bias {
                value += bias[channel];
            }
            *target = value;
        }
        Ok(())
    }

    pub fn execute(&self, input: &[f32], output: &mut [f32]) -> Result<(), MatrixError> {
        self.execute_batch(input, output, 1)
    }

    /// Input and output are contiguous frames, each with its channel dimension.
    /// Both complete shapes are validated before writing any output.
    pub fn execute_batch(
        &self,
        input: &[f32],
        output: &mut [f32],
        frames: usize,
    ) -> Result<(), MatrixError> {
        // Single-frame calls form the inner recurrent/sample loops. Timing each
        // matrix there would distort their enclosing GRU/sampling measurement.
        #[cfg(feature = "synth-profile")]
        let _profile = (frames > 1).then(|| crate::synthesis::profiling::span("op.affine.batch"));
        let inputs = frames
            .checked_mul(self.input_channels())
            .ok_or(MatrixError::InvalidShape)?;
        let outputs = frames
            .checked_mul(self.output_channels())
            .ok_or(MatrixError::InvalidShape)?;
        check_length("affine input", inputs, input.len())?;
        check_length("affine output", outputs, output.len())?;
        self.execute_frames(input, output, frames)
    }
}

fn baseline_dot(weights: &[f32], input: &[f32]) -> f32 {
    baseline_dot_from(weights, input, 0.0)
}

fn baseline_dot_from(weights: &[f32], input: &[f32], mut sum: f32) -> f32 {
    let mut chunks = weights
        .as_chunks::<4>()
        .0
        .iter()
        .zip(input.as_chunks::<4>().0.iter());
    for (weights, input) in &mut chunks {
        let p0 = weights[0] * input[0];
        let p1 = weights[1] * input[1];
        let p2 = weights[2] * input[2];
        let p3 = weights[3] * input[3];
        // Preserve this four-input grouping to keep floating-point rounding stable.
        sum = p3 + ((p2 + p1) + (p0 + sum));
    }
    let tail = weights.len() / 4 * 4;
    for (&weight, &value) in weights[tail..].iter().zip(&input[tail..]) {
        sum += weight * value;
    }
    sum
}
