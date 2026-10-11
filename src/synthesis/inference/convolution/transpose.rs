//! Transposed temporal convolution with ordered tap overlap-add.

use super::{Convolution, ConvolutionError, MatrixError};

pub const TRANSPOSED_CONVOLUTION_TYPE_ID: [u8; 16] = [
    0xbd, 0x96, 0x2b, 0x2e, 0x4a, 0x96, 0x75, 0x93, 0xb0, 0xfd, 0xdc, 0x63, 0x89, 0x97, 0x19, 0x1c,
];

/// Transposed convolution shares the same tap layout and five-word config as
/// ordinary convolution, but crops output boundaries instead of padding input.
#[derive(Clone)]
pub struct TransposedConvolution {
    inner: Convolution,
}

impl TransposedConvolution {
    pub fn from_convolution(inner: Convolution) -> Self {
        Self { inner }
    }

    pub fn input_channels(&self) -> usize {
        self.inner.input_channels()
    }

    pub fn output_channels(&self) -> usize {
        self.inner.output_channels()
    }

    pub fn output_frames(&self, frames: usize) -> Result<usize, ConvolutionError> {
        Ok(self
            .uncropped_frames(frames)?
            .saturating_sub(self.inner.config.padding * 2))
    }

    fn uncropped_frames(&self, frames: usize) -> Result<usize, ConvolutionError> {
        if frames == 0 {
            return Ok(0);
        }
        (frames - 1)
            .checked_mul(self.inner.config.stride)
            .and_then(|frames| frames.checked_add(self.inner.config.receptive_frames().ok()?))
            .ok_or(MatrixError::InvalidShape.into())
    }

    pub fn stream(&self) -> TransposedConvolutionStream {
        TransposedConvolutionStream {
            operator: self.clone(),
            pending: Vec::new(),
            origin: 0,
            input_frames: 0,
            finished: false,
        }
    }

    pub fn execute(&self, input: &[f32]) -> Result<Vec<f32>, ConvolutionError> {
        self.stream().process(input, true)
    }
}

/// Retains only overlap output rows, including the right crop until finalization.
#[derive(Clone)]
pub struct TransposedConvolutionStream {
    operator: TransposedConvolution,
    pending: Vec<f32>,
    origin: usize,
    input_frames: usize,
    finished: bool,
}

impl TransposedConvolutionStream {
    pub fn reset(&mut self) {
        self.pending.clear();
        self.origin = 0;
        self.input_frames = 0;
        self.finished = false;
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn pending_frames(&self) -> usize {
        self.pending.len() / self.operator.output_channels()
    }

    pub fn process(
        &mut self,
        input: &[f32],
        final_chunk: bool,
    ) -> Result<Vec<f32>, ConvolutionError> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("op.transposed_convolution");
        if self.finished {
            return Err(ConvolutionError::Finished);
        }
        let op = &self.operator.inner;
        let frames = op.input_frames(input)?;
        let count = self
            .input_frames
            .checked_add(frames)
            .ok_or(MatrixError::InvalidShape)?;
        let extent = self.operator.uncropped_frames(count)?;
        let output_channels = op.output_channels();
        let length = extent
            .checked_sub(self.origin)
            .and_then(|frames| frames.checked_mul(output_channels))
            .ok_or(MatrixError::InvalidShape)?;
        let stable = if final_chunk {
            extent
        } else {
            count
                .checked_mul(op.config.stride)
                .ok_or(MatrixError::InvalidShape)?
        };
        let previous_length = self.pending.len();
        self.pending.resize(length, 0.0);
        // Unlike ordinary convolution, the transpose operation initializes
        // each new output row with bias BEFORE adding matrix contributions.
        if let Some(bias) = &op.bias {
            for row in self.pending[previous_length..].chunks_exact_mut(output_channels) {
                row.copy_from_slice(bias);
            }
        }
        {
            #[cfg(feature = "synth-profile")]
            let _profile = crate::synthesis::profiling::span("kernel.transposed_convolution");
            for (frame, source) in input.chunks_exact(op.input_channels()).enumerate() {
                for tap in 0..op.config.kernel {
                    let target_frame =
                        (self.input_frames + frame) * op.config.stride + tap * op.config.dilation;
                    let offset = (target_frame - self.origin) * output_channels;
                    let target = &mut self.pending[offset..offset + output_channels];
                    op.weights.accumulate(tap, source, target);
                }
            }
        }
        self.input_frames = count;
        let end = stable.saturating_sub(op.config.padding);
        let start = self.origin.max(op.config.padding);
        let output = if end > start {
            self.pending
                [(start - self.origin) * output_channels..(end - self.origin) * output_channels]
                .to_vec()
        } else {
            Vec::new()
        };
        if final_chunk {
            self.pending.clear();
            self.finished = true;
        } else {
            let consumed = end.max(op.config.padding.min(stable)).max(self.origin);
            self.pending
                .drain(..(consumed - self.origin) * output_channels);
            self.origin = consumed;
        }
        Ok(output)
    }
}
