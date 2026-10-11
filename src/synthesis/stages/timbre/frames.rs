//! Current-owner frame conditioning, projected only as the stream consumes it.
use std::{ops::Range, sync::OnceLock};

use crate::synthesis::{
    inference::Sequence,
    stages::f0::features::ExpandedFeatures,
    voice::{Result, VoiceError},
};

use super::TimbreNetwork;

const BLOCK_FRAMES: usize = 256;

/// Frame conditions shared by all cursors of one immutable owner revision.
/// The blocks contain independent projection results, never recurrent state or
/// a previous edit's synthesis result. Dropping the prepared owner releases all.
pub struct PreparedTimbreFrames {
    expanded: ExpandedFeatures,
    channels: usize,
    pub(super) note_frames: Vec<usize>,
    blocks: Vec<OnceLock<Sequence>>,
}

impl PreparedTimbreFrames {
    pub(super) fn new(
        expanded: ExpandedFeatures,
        channels: usize,
        note_frames: Vec<usize>,
    ) -> Self {
        let blocks = (0..expanded.frames.div_ceil(BLOCK_FRAMES))
            .map(|_| OnceLock::new())
            .collect();
        Self {
            expanded,
            channels,
            note_frames,
            blocks,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.expanded.frames
    }

    pub(super) fn rows(
        &self,
        network: &TimbreNetwork<'_>,
        range: Range<usize>,
    ) -> Result<Sequence> {
        if range.start > range.end || range.end > self.len() {
            return Err(VoiceError::new("Timbre feature interval is out of bounds"));
        }
        let mut values = Vec::with_capacity(range.len() * self.channels);
        let mut position = range.start;
        while position < range.end {
            let index = position / BLOCK_FRAMES;
            let block_start = index * BLOCK_FRAMES;
            let block_end = (block_start + BLOCK_FRAMES).min(self.len());
            let slot = &self.blocks[index];
            if slot.get().is_none() {
                crate::synthesis::execution::checkpoint()?;
                // project_range reads the real convolution support from the
                // complete owner's expansion before cropping these rows.
                let projected = network
                    .feature_network
                    .project_range(&self.expanded, block_start..block_end)?;
                if projected
                    .convolutional
                    .iter()
                    .any(|value| !value.is_finite())
                {
                    return Err(VoiceError::new("Invalid timbre projected feature value"));
                }
                let block =
                    Sequence::new(projected.frames, self.channels, projected.convolutional)?;
                // Concurrent readers may compute the same immutable block.
                // Publication keeps only the first successful result.
                let _ = slot.set(block);
            }
            let block = slot.get().expect("projected block was published above");
            let end = range.end.min(block_end);
            values.extend_from_slice(
                &block.values
                    [(position - block_start) * self.channels..(end - block_start) * self.channels],
            );
            position = end;
        }
        Sequence::new(range.len(), self.channels, values)
    }
}
