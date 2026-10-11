use super::{Clock, PcmComponents, Result, VoiceError};
use std::ops::Range;

/// A bounded rolling buffer. Already published samples are discarded rather
/// than cloning a growing full render on each half-second publication.
#[derive(Default)]
pub(super) struct Buffer {
    origin: i64,
    offset: usize,
    samples: [Vec<f32>; 2],
    received_end: Option<i64>,
}

impl Buffer {
    pub fn append(&mut self, pcm: &PcmComponents) -> Result<()> {
        if pcm.primary.is_empty() {
            if !pcm.secondary.is_empty() || !pcm.start_seconds.is_finite() {
                return Err(VoiceError::new("Invalid empty progressive vocoder chunk"));
            }
            // The network can consume frames without releasing PCM yet. An
            // empty return supplies no sample-clock evidence or ready audio.
            return Ok(());
        }
        let clock = Clock::new(pcm.start_seconds, pcm.primary.len())?;
        if pcm.primary.len() != pcm.secondary.len()
            || pcm
                .primary
                .iter()
                .chain(pcm.secondary.iter())
                .any(|v| !v.is_finite())
            || self.received_end.is_some_and(|end| end != clock.origin)
        {
            return Err(VoiceError::new(
                "Progressive vocoder chunks are not finite and contiguous",
            ));
        }
        if self.received_end.is_none() {
            self.origin = clock.origin;
        }
        self.samples[0].extend_from_slice(&pcm.primary);
        self.samples[1].extend_from_slice(&pcm.secondary);
        self.received_end = Some(clock.end);
        Ok(())
    }

    pub fn received_end(&self) -> Option<i64> {
        self.received_end
    }

    pub fn end(&self) -> i64 {
        self.origin + self.samples[0].len() as i64
    }

    pub fn slices(&self, range: Range<i64>) -> Option<[&[f32]; 2]> {
        if range.start < self.origin + self.offset as i64
            || range.end > self.end()
            || range.start >= range.end
        {
            return None;
        }
        let range = (range.start - self.origin) as usize..(range.end - self.origin) as usize;
        Some([&self.samples[0][range.clone()], &self.samples[1][range]])
    }

    pub fn discard_before(&mut self, sample: i64) {
        self.offset =
            (sample - self.origin).clamp(self.offset as i64, self.samples[0].len() as i64) as usize;
        if self.offset >= 48_000 {
            for channel in &mut self.samples {
                channel.drain(..self.offset);
            }
            self.origin += self.offset as i64;
            self.offset = 0;
        }
    }

    /// This may only be called after observing the actual final model sample.
    pub fn natural_padding(&mut self, final_sample: i64, required_end: i64) -> Result<()> {
        if self.received_end != Some(final_sample) {
            return Err(VoiceError::new(
                "Silent context requires the verified natural model end",
            ));
        }
        if required_end > self.end() {
            let length = usize::try_from(required_end - self.origin)
                .map_err(|_| VoiceError::new("Natural PCM padding size overflow"))?;
            for channel in &mut self.samples {
                channel.resize(length, 0.0);
            }
        }
        Ok(())
    }
}
