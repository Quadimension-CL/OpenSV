use std::{ops::Range, sync::OnceLock};

use crate::synthesis::{
    inference::Sequence,
    stages::f0::features::ExpandedFeatures,
    voice::{Result, VoiceError},
};

use super::TimbreNetwork;

const BLOCK_FRAMES: usize = 256;

struct FrameBlock {
    convolutional: Sequence,
    pointwise: Option<Vec<f32>>,
}

impl FrameBlock {
    fn slice_values(&self, rows: Range<usize>, pointwise: bool) -> Result<&[f32]> {
        if rows.start > rows.end || rows.end > self.convolutional.frames {
            return Err(VoiceError::new("Feature block row interval is out of bounds"));
        }
        let values = if pointwise {
            self.pointwise.as_deref().ok_or_else(|| {
                VoiceError::new("16-child mean requires the distinct pointwise feature branch")
            })?
        } else {
            &self.convolutional.values
        };
        if values.len() != self.convolutional.values.len() {
            return Err(VoiceError::new("Feature branches have different row geometry"));
        }
        let width = self.convolutional.channels;
        let selected = &values[rows.start * width..rows.end * width];
        if pointwise && selected.iter().any(|value| !value.is_finite()) {
            return Err(VoiceError::new("Invalid pointwise timbre feature value"));
        }
        Ok(selected)
    }
}

pub struct PreparedTimbreFrames {
    expanded: ExpandedFeatures,
    channels: usize,
    pub(super) note_frames: Vec<usize>,
    blocks: Vec<OnceLock<FrameBlock>>,
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
        self.branch_rows(network, range, false)
    }

    pub(super) fn pointwise_rows(
        &self,
        network: &TimbreNetwork<'_>,
        range: Range<usize>,
    ) -> Result<Sequence> {
        self.branch_rows(network, range, true)
    }

    fn branch_rows(
        &self,
        network: &TimbreNetwork<'_>,
        range: Range<usize>,
        pointwise: bool,
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

                let block = FrameBlock {
                    convolutional: Sequence::new(projected.frames, self.channels, projected.convolutional)?,
                    pointwise: projected.pointwise,
                };

                let _ = slot.set(block);
            }
            let block = slot.get().expect("projected block was published above");
            let end = range.end.min(block_end);
            values.extend_from_slice(block.slice_values(position - block_start..end - block_start, pointwise)?);
            position = end;
        }
        Sequence::new(range.len(), self.channels, values)
    }
}

#[cfg(test)]
mod pointwise_feature_tests {
    use super::*;

    #[test]
    fn distinct_cached_branches_preserve_cross_block_row_order() {
        let blocks: Vec<_> = (0..2).map(|block| {
            let start = block * BLOCK_FRAMES;
            FrameBlock {
                convolutional: Sequence::new(BLOCK_FRAMES, 1, (start..start + BLOCK_FRAMES).map(|i| i as f32).collect()).unwrap(),
                pointwise: Some((start..start + BLOCK_FRAMES).map(|i| 1000.0 + i as f32).collect()),
            }
        }).collect();
        for (pointwise, expected) in [(false, vec![255.0, 256.0, 257.0]), (true, vec![1255.0, 1256.0, 1257.0])] {
            let mut rows = blocks[0].slice_values(255..256, pointwise).unwrap().to_vec();
            rows.extend_from_slice(blocks[1].slice_values(0..2, pointwise).unwrap());
            assert_eq!(rows, expected);
        }
        assert_eq!(blocks[0].slice_values(0..1, false).unwrap(), &[0.0]);
    }

    #[test]
    fn absent_or_invalid_pointwise_is_not_a_convolutional_fallback() {
        let mut block = FrameBlock { convolutional: Sequence::new(1, 1, vec![9.0]).unwrap(), pointwise: None };
        assert!(block.slice_values(0..1, true).is_err());
        assert_eq!(block.slice_values(0..1, false).unwrap(), &[9.0]);
        block.pointwise = Some(vec![f32::NAN]);
        assert!(block.slice_values(0..1, true).is_err());
        assert_eq!(block.slice_values(0..1, false).unwrap(), &[9.0]);
    }

    #[test]
    fn partial_final_block_has_exact_bounds_and_distinct_storage() {
        let block = FrameBlock {
            convolutional: Sequence::new(244, 1, (256..500).map(|i| i as f32).collect()).unwrap(),
            pointwise: Some((256..500).map(|i| 1000.0 + i as f32).collect()),
        };
        assert_eq!(block.slice_values(243..244, false).unwrap(), &[499.0]);
        assert_eq!(block.slice_values(243..244, true).unwrap(), &[1499.0]);
        assert!(block.slice_values(243..245, true).is_err());
        assert_ne!(block.convolutional.values.as_ptr(), block.pointwise.as_ref().unwrap().as_ptr());
    }
}
