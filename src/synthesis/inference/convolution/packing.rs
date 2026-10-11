//! Output-channel blocks shared by ordinary and transposed temporal convolution.

use std::sync::Arc;

use super::{DenseMatrix, MatrixError};

pub(super) const OUTPUT_BLOCK: usize = 8;

/// [output block][tap][input channel][8 output lanes]. The final output block
/// is zero-padded; those lanes are never exposed as real output channels.
#[derive(Clone)]
pub(super) struct PackedWeights {
    input_channels: usize,
    output_channels: usize,
    block_length: usize,
    values: Arc<[f32]>,
}

impl PackedWeights {
    /// The caller has validated that every tap has the same nonempty shape.
    pub(super) fn new(taps: &[DenseMatrix]) -> Result<Self, MatrixError> {
        let input_channels = taps[0].columns();
        let output_channels = taps[0].rows();
        let block_length = taps
            .len()
            .checked_mul(input_channels)
            .and_then(|length| length.checked_mul(OUTPUT_BLOCK))
            .ok_or(MatrixError::InvalidShape)?;
        let length = output_channels
            .div_ceil(OUTPUT_BLOCK)
            .checked_mul(block_length)
            .ok_or(MatrixError::InvalidShape)?;
        let mut values = vec![0.0; length];
        for (block, target) in values.chunks_exact_mut(block_length).enumerate() {
            let first_output = block * OUTPUT_BLOCK;
            let channels = (output_channels - first_output).min(OUTPUT_BLOCK);
            for (tap, matrix) in taps.iter().enumerate() {
                let offset = tap * input_channels * OUTPUT_BLOCK;
                for lane in 0..channels {
                    for (input, &value) in matrix.row(first_output + lane).iter().enumerate() {
                        target[offset + input * OUTPUT_BLOCK + lane] = value;
                    }
                }
            }
        }
        Ok(Self {
            input_channels,
            output_channels,
            block_length,
            values: values.into(),
        })
    }

    pub(super) fn input_channels(&self) -> usize {
        self.input_channels
    }

    pub(super) fn output_channels(&self) -> usize {
        self.output_channels
    }

    pub(super) fn blocks(&self) -> std::slice::ChunksExact<'_, f32> {
        self.values.chunks_exact(self.block_length)
    }
}
