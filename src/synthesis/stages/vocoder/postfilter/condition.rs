//! Centered, edge-replicated condition interpolation.
//!
//! Instead of repeating every input frame, the operator places each frame
//! at the center of its upsampled interval and interpolates between centers.
//! Only the last input frame is retained between calls.

use crate::synthesis::{
    execution::checkpoint,
    inference::Sequence,
    voice::{Result, VoiceError},
};

#[derive(Clone, Debug)]
pub struct ConditionUpsampler {
    factor: usize,
    previous: Vec<f32>,
    finished: bool,
}

impl ConditionUpsampler {
    /// Mo Xu uses `frame_samples / bands = 240 / 8 = 30`.
    pub fn new(factor: usize) -> Result<Self> {
        if factor == 0 || factor > i32::MAX as usize {
            return Err(VoiceError::new("Invalid condition upsampling factor"));
        }
        Ok(Self {
            factor,
            previous: Vec::new(),
            finished: false,
        })
    }

    pub fn reset(&mut self) {
        self.previous.clear();
        self.finished = false;
    }

    /// Returns only newly available frames. Before the final call, the stream
    /// withholds `ceil(factor / 2)` frames, waiting for the next input center.
    /// A final empty call is supported; a complete N-frame stream yields
    /// exactly `N * factor` output frames without adding a temporal offset.
    pub fn process(&mut self, input: &Sequence, flush: bool) -> Result<Sequence> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.postfilter.condition.process");
        input.validate()?;
        if self.finished {
            return Err(VoiceError::new("Condition upsampler is already finished"));
        }
        if !self.previous.is_empty() && self.previous.len() != input.channels {
            return Err(VoiceError::new("Condition channel count changed midstream"));
        }
        if input.values.iter().any(|value| !value.is_finite()) {
            return Err(VoiceError::new("Non-finite vocoder condition"));
        }
        let primed = !self.previous.is_empty();
        let has_input = primed || input.frames != 0;
        let frames = if has_input {
            let intervals = input.frames - usize::from(!primed);
            intervals
                .checked_mul(self.factor)
                .and_then(|frames| frames.checked_add(if primed { 0 } else { self.factor / 2 }))
                .and_then(|frames| {
                    frames.checked_add(if flush { self.factor.div_ceil(2) } else { 0 })
                })
                .ok_or_else(|| VoiceError::new("Upsampled condition length overflows"))?
        } else {
            0
        };
        let length = frames
            .checked_mul(input.channels)
            .ok_or_else(|| VoiceError::new("Upsampled condition size overflows"))?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(length)
            .map_err(|_| VoiceError::new("Could not allocate upsampled conditions"))?;

        let center_offset = if self.factor.is_multiple_of(2) {
            0.5
        } else {
            0.0
        };
        for current in input.values.chunks_exact(input.channels) {
            checkpoint()?;
            if self.previous.is_empty() {
                for _ in 0..self.factor / 2 {
                    values.extend_from_slice(current);
                }
                self.previous.extend_from_slice(current);
            } else {
                for step in 0..self.factor {
                    let alpha = (step as f32 + center_offset) / self.factor as f32;
                    values.extend(
                        self.previous
                            .iter()
                            .zip(current)
                            .map(|(&previous, &next)| (next - previous) * alpha + previous),
                    );
                }
                self.previous.copy_from_slice(current);
            }
        }
        if flush && has_input {
            for _ in 0..self.factor.div_ceil(2) {
                values.extend_from_slice(&self.previous);
            }
        }
        self.finished = flush;
        Sequence::new(frames, input.channels, values)
    }
}
