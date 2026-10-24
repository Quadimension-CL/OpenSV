use crate::synthesis::voice::{Result, VoiceError};

#[derive(Clone, Copy, Debug)]
pub struct SyllableRange {
    pub start: usize,
    pub end: usize,

    pub baseline: f32,
}

#[derive(Clone, Debug)]
pub struct SamplerStatistics {
    pub slope: Vec<f32>,
    pub offset: Vec<f32>,
}

impl SamplerStatistics {

    pub fn estimate(
        pitch: &[f32],
        vowel_mask: &[bool],
        syllables: &[SyllableRange],
    ) -> Result<Self> {
        if pitch.len() != vowel_mask.len() {
            return Err(VoiceError::new("F0 pitch and vowel mask lengths differ"));
        }
        if let Some(frame) = pitch.iter().position(|value| !value.is_finite()) {
            return Err(VoiceError::new(format!(
                "F0 statistics received non-finite pitch at frame {frame}"
            )));
        }
        let mut previous_end = 0;
        for (index, syllable) in syllables.iter().enumerate() {
            if syllable.start < previous_end
                || syllable.start > syllable.end
                || syllable.end > pitch.len()
                || !syllable.baseline.is_finite()
            {
                return Err(VoiceError::new(format!(
                    "Invalid F0 statistics bounds or baseline at syllable {index}"
                )));
            }
            previous_end = syllable.end;
        }

        let mut result = Self {
            slope: vec![0.0; pitch.len()],
            offset: vec![0.0; pitch.len()],
        };
        let mut differences = Vec::new();
        let mut levels = Vec::new();
        for syllable in syllables {
            differences.clear();
            levels.clear();
            for frame in syllable.start..syllable.end {
                if vowel_mask[frame] {

                    differences.push(if frame == 0 {
                        0.0
                    } else {
                        pitch[frame] - pitch[frame - 1]
                    });
                    levels.push(pitch[frame]);
                }
            }
            if levels.is_empty() {
                continue;
            }
            let vowel_frames = levels.len() as f32;

            let slope = vowel_frames * (percentile(&mut differences, 0.5) * 0.05);
            let offset = percentile(&mut levels, 0.8) - syllable.baseline;
            result.slope[syllable.start..syllable.end].fill(slope);
            result.offset[syllable.start..syllable.end].fill(offset);
        }
        Ok(result)
    }
}

fn percentile(values: &mut [f32], quantile: f32) -> f32 {
    let index = (((values.len() - 1) as f32 * quantile) as usize).min(values.len() - 1);
    *values.select_nth_unstable_by(index, f32::total_cmp).1
}

pub fn control_weight(delta: f32, scale: f32) -> f32 {
    if scale <= 0.0 {
        return delta;
    }
    let scaled = delta * scale;
    let magnitude = scaled.abs();
    let square = scaled * scaled;
    let denominator = ((((square * 264.0 + 720_720.0) * square + 484_323_840.0) * square
        + 105_859_350_000.0)
        * square
        + 6_704_425_600_000.0)
        * square
        + 56_317_176_000_000.0;
    let numerator = ((((((((((magnitude + 132.0) * magnitude + 8_580.0) * magnitude
        + 360_360.0)
        * magnitude
        + 10_810_800.0)
        * magnitude
        + 242_161_920.0)
        * magnitude
        + 4_116_752_600.0)
        * magnitude
        + 52_929_675_000.0)
        * magnitude
        + 502_831_940_000.0)
        * magnitude
        + 3_352_212_800_000.0)
        * magnitude
        + 14_079_294_000_000.0)
        * magnitude
        + 28_158_588_000_000.0;
    let probability = ((1.0 / denominator) * numerator).clamp(0.0, 1.0);
    probability + probability - 1.0
}
