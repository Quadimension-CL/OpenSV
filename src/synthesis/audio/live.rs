//! Sparse, immutable PCM blocks for one live playback revision.

mod conversion;

use super::AudioError;
use std::sync::{Arc, RwLock, RwLockReadGuard};

pub const LIVE_SAMPLE_RATE: u32 = 48_000;
pub const LIVE_BLOCK_FRAMES: usize = 24_000;
pub const LIVE_CHANNELS: usize = 2;

/// Pending synthesis is distinct from a known silent part of the timeline.
#[derive(Debug)]
pub(crate) enum LiveBlock {
    Pending,
    Silence,
    Pcm(Arc<[f32]>),
}

/// One revision of the complete project timeline in 48 kHz stereo.
///
/// Publishers may fill blocks in any order, then replace complete blocks when
/// seam correction finishes. Buffers themselves stay immutable. Callback reads
/// borrow the block under a nonblocking read guard; replacement and release of
/// old PCM happen on the publishing worker after those reads finish. The
/// playback session retains this source until replacement excludes its active
/// callbacks. A new project revision gets a separate source.
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

    /// All blocks are half a second, except the final block if it is shorter.
    pub fn block_frames(&self, index: usize) -> Option<usize> {
        self.blocks.get(index).map(|_| {
            self.total_frames
                .saturating_sub(index * LIVE_BLOCK_FRAMES)
                .min(LIVE_BLOCK_FRAMES)
        })
    }

    /// Publish immutable interleaved stereo PCM. The buffer must have exactly
    /// the block's frame count, including the shorter final block. Call only
    /// from a worker: replacing a block may wait for an active callback read.
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

    /// Mark a block as known silence without allocating a sample buffer.
    pub fn silence(&self, index: usize) -> Result<(), AudioError> {
        self.set(index, LiveBlock::Silence)
    }

    /// Nonblocking callback access. `None` means a contended/poisoned block or
    /// an index beyond the source. Unpublished blocks contain `Pending`.
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
        // Reclaim the previous allocation outside the short publication lock,
        // on this worker, never on a callback that only borrowed its contents.
        drop(previous);
        Ok(())
    }
}
