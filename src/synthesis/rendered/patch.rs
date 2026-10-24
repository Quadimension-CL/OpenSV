mod head;
mod interpolate;
mod tail;

use crate::synthesis::voice::{Result, VoiceError};

pub use head::splice_head;
pub use tail::splice_tail;

pub const PREVIEW_SAMPLE_RATE: u32 = 48_000;
pub const PREVIEW_BLOCK_SAMPLES: usize = 24_000;
const MAX_FADE_SAMPLES: usize = 1_024;

#[derive(Debug)]
pub struct HeadSplice {

    pub samples: [Vec<f32>; 2],

    pub shift_samples: i32,
}

#[derive(Debug)]
pub struct TailSplice {

    pub samples: [Vec<f32>; 2],

    pub anchor: usize,

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
