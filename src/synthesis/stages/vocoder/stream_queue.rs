//! Frame-preserving joins between branches with different streaming latency.

use std::collections::VecDeque;

use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

pub(crate) struct FrameQueue {
    channels: usize,
    values: VecDeque<f32>,
}

impl FrameQueue {
    pub(crate) fn new(channels: usize) -> Self {
        Self {
            channels,
            values: VecDeque::new(),
        }
    }

    pub(crate) fn frames(&self) -> usize {
        self.values.len() / self.channels
    }

    pub(crate) fn push(&mut self, input: Sequence) -> Result<()> {
        input.validate()?;
        if input.channels != self.channels {
            return Err(VoiceError::new("Vocoder queue channel count changed"));
        }
        self.values.extend(input.values);
        Ok(())
    }

    pub(crate) fn take(&mut self, frames: usize) -> Result<Sequence> {
        if frames > self.frames() {
            return Err(VoiceError::new("Vocoder queue has insufficient frames"));
        }
        Sequence::new(
            frames,
            self.channels,
            self.values.drain(..frames * self.channels).collect(),
        )
    }
}

/// Only explicit stream finalization requires both upstream tails to agree.
pub(crate) fn paired(
    first: &mut FrameQueue,
    second: &mut FrameQueue,
    end: bool,
) -> Result<(Sequence, Sequence)> {
    if end && first.frames() != second.frames() {
        return Err(VoiceError::new(
            "Vocoder branches ended with unaligned frames",
        ));
    }
    let frames = first.frames().min(second.frames());
    Ok((first.take(frames)?, second.take(frames)?))
}
