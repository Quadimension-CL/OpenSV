use crate::synthesis::audio::AudioError;
use std::sync::{Arc, RwLock};

pub(super) const POSITION_BITS: u32 = 44;
pub(super) const POSITION_MASK: u64 = (1 << POSITION_BITS) - 1;

#[derive(Clone, Default)]
pub(crate) struct LoopControl(Arc<RwLock<Option<(f64, f64)>>>);

#[derive(Clone, Copy)]
pub(super) struct FrameLoop {
    pub start: u64,
    pub end: u64,
}

impl LoopControl {
    pub fn set_range(&self, range: Option<(f64, f64)>) -> Result<(), AudioError> {
        if let Some((start, end)) = range
            && (!start.is_finite()
                || !end.is_finite()
                || start < 0.0
                || end <= start
                || end * 384_000.0 > POSITION_MASK as f64)
        {
            return Err(AudioError::InvalidControl(
                "Loop range must be finite, nonnegative and within playback capacity".into(),
            ));
        }
        *self
            .0
            .write()
            .map_err(|_| AudioError::InvalidControl("Loop control lock poisoned".into()))? = range;
        Ok(())
    }

    pub(super) fn frames(&self, sample_rate: u32) -> Option<FrameLoop> {
        self.0
            .read()
            .ok()
            .and_then(|range| range.map(|range| FrameLoop::new(range, sample_rate)))
    }

    /// A concurrent edit retains the previous buffer's bounds. The audio
    /// callback never waits for the UI to release its control lock.
    pub(super) fn update_frames(&self, sample_rate: u32, range: &mut Option<FrameLoop>) {
        if let Ok(current) = self.0.try_read() {
            *range = current.map(|range| FrameLoop::new(range, sample_rate));
        }
    }
}

impl FrameLoop {
    fn new((start, end): (f64, f64), sample_rate: u32) -> Self {
        let start = ((start * f64::from(sample_rate)).ceil() as u64).min(POSITION_MASK - 1);
        let end = (end * f64::from(sample_rate)).ceil() as u64;
        Self {
            start,
            end: end.max(start + 1),
        }
    }

    pub fn wrap(self, cursor: u64) -> u64 {
        if cursor < self.start || cursor >= self.end {
            self.start
        } else {
            cursor
        }
    }
}
