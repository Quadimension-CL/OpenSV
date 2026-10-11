use std::{ops::Range, sync::Arc};

use super::super::{PreparedTimbreFrames, TimbreNetwork};
use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

pub(super) enum Features {
    Dense(Sequence),
    Prepared {
        source: Arc<PreparedTimbreFrames>,
        first_frame: usize,
    },
}

impl Features {
    pub fn frames(&self) -> usize {
        match self {
            Self::Dense(rows) => rows.frames,
            Self::Prepared {
                source,
                first_frame,
            } => source.len() - first_frame,
        }
    }

    pub fn slice_frames(
        &self,
        network: &TimbreNetwork<'_>,
        range: Range<usize>,
    ) -> Result<Sequence> {
        if range.start > range.end || range.end > self.frames() {
            return Err(VoiceError::new(
                "Timbre stream feature interval is out of bounds",
            ));
        }
        match self {
            Self::Dense(rows) => rows.slice_frames(range),
            Self::Prepared {
                source,
                first_frame,
            } => source.rows(network, first_frame + range.start..first_frame + range.end),
        }
    }
}
