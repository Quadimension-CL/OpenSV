use super::{RATE, Result, VoiceError};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Clock {
    pub origin: i64,
    pub end: i64,
}

impl Clock {
    pub fn new(seconds: f64, count: usize) -> Result<Self> {
        let position = seconds * RATE;
        if !position.is_finite()
            || (position - position.round()).abs() > 0.0001
            || position <= i64::MIN as f64
            || position >= i64::MAX as f64
            || count == 0
        {
            return Err(VoiceError::new("Invalid progressive PCM sample clock"));
        }
        let origin = position.round() as i64;
        let end = i64::try_from(count)
            .ok()
            .and_then(|count| origin.checked_add(count))
            .ok_or_else(|| VoiceError::new("Progressive PCM sample clock overflow"))?;
        Ok(Self { origin, end })
    }

    pub fn range(self, range: Range<i64>) -> Result<Range<usize>> {
        if range.start < self.origin || range.end > self.end || range.start >= range.end {
            return Err(VoiceError::new(
                "Progressive PCM range exceeds its sample clock",
            ));
        }
        Ok((range.start - self.origin) as usize..(range.end - self.origin) as usize)
    }
}
