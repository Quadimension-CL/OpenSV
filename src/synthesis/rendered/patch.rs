//! Phase-aligned seams for replacing published PCM blocks.
//!
//! Callers provide old and newly rendered samples on the same absolute clock;
//! this module does not choose model context or invalidation ranges. Matching
//! uses the first internal vocoder component and applies the same displacement
//! and time warp to both components, before they are mixed to playback mono.

mod head;
mod interpolate;
mod tail;

use crate::synthesis::voice::{Result, VoiceError};

pub use head::splice_head;
pub use tail::splice_tail;

/// Live preview's sample rate and publication block size.
pub const PREVIEW_SAMPLE_RATE: u32 = 48_000;
pub const PREVIEW_BLOCK_SAMPLES: usize = 24_000;
const MAX_FADE_SAMPLES: usize = 1_024;

#[derive(Debug)]
pub struct HeadSplice {
    /// Starts at the same absolute sample as `fresh`, including the old block
    /// preceding the replacement. Samples before its fade retain old PCM.
    pub samples: [Vec<f32>; 2],
    /// `output[t]` reads `fresh[t + shift_samples]` after the head fade.
    pub shift_samples: i32,
}

#[derive(Debug)]
pub struct TailSplice {
    /// Starts at the shared old/new origin and covers complete output blocks.
    /// The last block's samples beyond the fade retain the old PCM.
    pub samples: [Vec<f32>; 2],
    /// The selected comparison anchor in the unwarped new PCM.
    pub anchor: usize,
    /// The final fade starts at `anchor + shift_samples`.
    pub shift_samples: i32,
    pub fade_samples: usize,
}

impl TailSplice {
    pub fn fade_start(&self) -> usize {
        (self.anchor as i64 + i64::from(self.shift_samples)) as usize
    }

    pub fn fade_end(&self) -> usize {
        self.fade_start() + self.fade_samples
    }
}

fn validate_block_size(block_samples: usize) -> Result<()> {
    if block_samples == 0 || block_samples > i32::MAX as usize / 2 {
        return Err(VoiceError::new("PCM seam has an invalid block size"));
    }
    Ok(())
}

fn validate_components(samples: [&[f32]; 2]) -> Result<()> {
    if samples[0].len() != samples[1].len() {
        return Err(VoiceError::new(
            "PCM seam components have different lengths",
        ));
    }
    if samples
        .iter()
        .flat_map(|channel| channel.iter())
        .any(|sample| !sample.is_finite())
    {
        return Err(VoiceError::new("PCM seam input contains nonfinite samples"));
    }
    Ok(())
}

fn absolute_difference(left: &[f32], right: &[f32]) -> f32 {
    let mut sum = 0.0_f32;
    for (left, right) in left.iter().zip(right) {
        sum += (left - right).abs();
    }
    sum
}
