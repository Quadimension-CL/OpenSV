use std::{ops::Range, time::Instant};

/// A newly published interval's transient waveform-edge highlight.
#[derive(Clone, Debug)]
pub struct VisualFlash {
    pub range: Range<i64>,
    pub started: Instant,
}

impl VisualFlash {
    pub fn intensity(&self, now: Instant) -> f32 {
        let remaining = 1.0 - now.saturating_duration_since(self.started).as_secs_f32() / 0.9;
        remaining.max(0.0).powi(2)
    }
}
