mod conversion;

use super::AudioError;
use std::sync::{Arc, RwLock, RwLockReadGuard};

pub const LIVE_SAMPLE_RATE: u32 = 48_000;
pub const LIVE_BLOCK_FRAMES: usize = 24_000;
pub const LIVE_CHANNELS: usize = 2;

#[derive(Debug)]
pub(crate) enum LiveBlock {
    Pending,
    Silence,
    Pcm(Arc<[f32]>),
}

#[derive(Debug)]
pub struct LiveAudio {
    total_frames: usize,
    blocks: Vec<RwLock<LiveBlock>>,
}

impl LiveAudio {
    pub fn new(total_frames: usize) -> Self {
        let count = total_frames.div_ceil(LIVE_BLOCK_FRAMES);
        Self {
            total_frames,
            blocks: (0..count)
                .map(|_| RwLock::new(LiveBlock::Pending))
                .collect(),
        }
    }

    pub fn total_frames(&self) -> usize {
        self.total_frames
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    pub fn block_frames(&self, index: usize) -> Option<usize> {
        self.blocks.get(index).map(|_| {
            self.total_frames
                .saturating_sub(index * LIVE_BLOCK_FRAMES)
                .min(LIVE_BLOCK_FRAMES)
        })
    }

    pub fn publish(&self, index: usize, samples: Arc<[f32]>) -> Result<(), AudioError> {
        let frames = self.block_frames(index).ok_or_else(|| {
            AudioError::InvalidPcm(format!("Live PCM block {index} is outside the timeline"))
        })?;
        if samples.len() != frames * LIVE_CHANNELS {
            return Err(AudioError::InvalidPcm(format!(
                "Live PCM block {index} requires {} stereo samples, got {}",
                frames * LIVE_CHANNELS,
                samples.len()
            )));
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(AudioError::InvalidPcm(format!(
                "Live PCM block {index} contains a nonfinite sample"
            )));
        }
        self.set(index, LiveBlock::Pcm(samples))
    }

    pub fn silence(&self, index: usize) -> Result<(), AudioError> {
        self.set(index, LiveBlock::Silence)
    }

    pub(crate) fn block(&self, index: usize) -> Option<RwLockReadGuard<'_, LiveBlock>> {
        self.blocks.get(index)?.try_read().ok()
    }

    fn set(&self, index: usize, block: LiveBlock) -> Result<(), AudioError> {
        let slot = self.blocks.get(index).ok_or_else(|| {
            AudioError::InvalidPcm(format!("Live PCM block {index} is outside the timeline"))
        })?;
        let previous = {
            let mut guard = slot.write().map_err(|_| {
                AudioError::InvalidPcm(format!(
                    "Live PCM block {index} publication lock is poisoned"
                ))
            })?;
            std::mem::replace(&mut *guard, block)
        };

        drop(previous);
        Ok(())
    }
}
