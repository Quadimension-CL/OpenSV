//! Stateful row queue for temporal convolution.

use super::{Convolution, ConvolutionError, MatrixError};

/// Stream state is independent of immutable weights and can be cloned for an
/// incremental rendering checkpoint. Only unconsumed input rows are retained.
#[derive(Clone)]
pub struct ConvolutionStream {
    operator: Convolution,
    pending: Vec<f32>,
    finished: bool,
}

impl ConvolutionStream {
    pub(super) fn new(operator: Convolution) -> Result<Self, ConvolutionError> {
        let length = operator
            .config
            .padding
            .checked_mul(operator.input_channels())
            .ok_or(MatrixError::InvalidShape)?;
        Ok(Self {
            operator,
            pending: vec![0.0; length],
            finished: false,
        })
    }

    pub fn reset(&mut self) {
        self.pending.clear();
        self.pending.resize(
            self.operator.config.padding * self.operator.input_channels(),
            0.0,
        );
        self.finished = false;
    }

    pub fn pending_frames(&self) -> usize {
        self.pending.len() / self.operator.input_channels()
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Supply chronological input once; `final_chunk` makes the right padding
    /// available. A finalized stream must be reset before reuse.
    pub fn process(
        &mut self,
        input: &[f32],
        final_chunk: bool,
    ) -> Result<Vec<f32>, ConvolutionError> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("op.convolution");
        if self.finished {
            return Err(ConvolutionError::Finished);
        }
        let input_frames = self.operator.input_frames(input)?;
        let available = self
            .pending_frames()
            .checked_add(input_frames)
            .and_then(|frames| {
                frames.checked_add(if final_chunk {
                    self.operator.config.padding
                } else {
                    0
                })
            })
            .ok_or(MatrixError::InvalidShape)?;
        let frames = self.operator.config.available_outputs(available)?;
        frames
            .checked_mul(self.operator.output_channels())
            .ok_or(MatrixError::InvalidShape)?;
        self.pending
            .len()
            .checked_add(input.len())
            .ok_or(MatrixError::InvalidShape)?;
        self.pending.extend_from_slice(input);
        if frames == 0 {
            // A zero-output call leaves the final flag unset.
            return Ok(Vec::new());
        }
        let output = self.operator.execute_pending(&self.pending, frames)?;
        if final_chunk {
            self.pending.clear();
            self.finished = true;
        } else {
            let consumed = frames * self.operator.config.stride * self.operator.input_channels();
            self.pending.drain(..consumed);
        }
        Ok(output)
    }
}
