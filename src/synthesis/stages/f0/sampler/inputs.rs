use crate::synthesis::voice::{Result, VoiceError};

use super::super::statistics::SyllableRange;

/// Frame-aligned controls for both prediction passes.
pub struct SamplerControls<'a> {
    /// Appended to the positional features without a projection.
    pub noise: &'a [f32],
    pub expressiveness: &'a [f32],
    pub strength: &'a [f32],
    /// NaN means unspecified and is measured from the preliminary pitch.
    pub slope: &'a [f32],
    /// NaN means unspecified and is measured from the preliminary pitch.
    pub offset: &'a [f32],
    /// Normalized absolute pitch control and its blend weight.
    pub absolute_control: &'a [f32],
    pub control_weight: &'a [f32],
    /// Three ordered position channels, each expanded to sin/cos features.
    pub positions: [&'a [f32]; 3],
    /// Multiplies the projected voice/style condition.
    pub style_weight: &'a [f32],
    /// Additional scalar condition in the 13-child layout.
    pub accent: &'a [f32],
}

pub struct SamplerInputs<'a> {
    pub frames: usize,
    pub frame_features: usize,
    /// Frame-major context features.
    pub frame_condition: &'a [f32],
    /// Frame-major context input to the scalar source projection.
    pub source_condition: &'a [f32],
    /// Frame-major style features.
    pub style_condition: &'a [f32],
    pub controls: SamplerControls<'a>,
    pub vowel_mask: &'a [bool],
    pub syllables: &'a [SyllableRange],
}

impl SamplerInputs<'_> {
    pub(super) fn validate(&self) -> Result<()> {
        let expected = self
            .frames
            .checked_mul(self.frame_features)
            .ok_or_else(|| VoiceError::new("F0 frame feature size overflow"))?;
        if self.frame_features == 0 || self.vowel_mask.len() != self.frames {
            return Err(VoiceError::new("Invalid F0 frame dimensions or vowel mask"));
        }
        check_output(self.frame_condition, expected, "F0 frame condition")?;
        check_frames(self.source_condition, self.frames, "F0 source condition")?;
        check_frames(self.style_condition, self.frames, "F0 style condition")?;
        for (name, values) in [
            ("noise", self.controls.noise),
            ("expressiveness", self.controls.expressiveness),
            ("strength", self.controls.strength),
            ("absolute control", self.controls.absolute_control),
            ("control weight", self.controls.control_weight),
            ("position 1", self.controls.positions[0]),
            ("position 2", self.controls.positions[1]),
            ("position 3", self.controls.positions[2]),
            ("style weight", self.controls.style_weight),
            ("accent", self.controls.accent),
        ] {
            check_output(values, self.frames, name)?;
        }
        for values in [self.controls.slope, self.controls.offset] {
            if values.len() != self.frames || values.iter().any(|value| value.is_infinite()) {
                return Err(VoiceError::new("Invalid F0 slope/offset controls"));
            }
        }
        Ok(())
    }

    pub(super) fn positional(&self, width: usize, scale: f32) -> Result<Vec<f32>> {
        let stride = width
            .checked_mul(3)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| VoiceError::new("F0 positional feature width overflow"))?;
        let size = self
            .frames
            .checked_mul(stride)
            .ok_or_else(|| VoiceError::new("F0 positional feature size overflow"))?;
        let mut result = vec![0.0; size];
        // Compute f32 frequencies with exponent 2*i/width.
        let frequencies: Vec<_> = (0..width / 2)
            .map(|index| 10000.0_f32.powf((index * 2) as f32 / width as f32))
            .collect();
        for (frame, row) in result.chunks_exact_mut(stride).enumerate() {
            for (position, values) in self.controls.positions.iter().enumerate() {
                let value = values[frame] * scale;
                for (index, divisor) in frequencies.iter().enumerate() {
                    let (sin, cos) = (value / divisor).sin_cos();
                    row[position * width + index] = sin;
                    row[position * width + width / 2 + index] = cos;
                }
            }
            row[stride - 1] = self.controls.noise[frame];
        }
        Ok(result)
    }
}

pub(super) fn check_output(values: &[f32], expected: usize, label: &str) -> Result<()> {
    if values.len() != expected || values.iter().any(|value| !value.is_finite()) {
        return Err(VoiceError::new(format!(
            "Invalid {label} shape or non-finite value"
        )));
    }
    Ok(())
}

pub(super) fn check_frames(values: &[f32], frames: usize, label: &str) -> Result<()> {
    if (frames == 0 && !values.is_empty())
        || (frames != 0 && (values.is_empty() || !values.len().is_multiple_of(frames)))
        || values.iter().any(|value| !value.is_finite())
    {
        return Err(VoiceError::new(format!("Invalid {label} frame layout")));
    }
    Ok(())
}

pub(super) fn concatenate(left: &[f32], right: &[f32], frames: usize) -> Result<Vec<f32>> {
    check_frames(left, frames, "F0 concatenation left input")?;
    check_frames(right, frames, "F0 concatenation right input")?;
    if frames == 0 {
        return Ok(Vec::new());
    }
    let size = left
        .len()
        .checked_add(right.len())
        .ok_or_else(|| VoiceError::new("F0 concatenation size overflow"))?;
    let mut result = Vec::with_capacity(size);
    for (left, right) in left
        .chunks_exact(left.len() / frames)
        .zip(right.chunks_exact(right.len() / frames))
    {
        result.extend_from_slice(left);
        result.extend_from_slice(right);
    }
    Ok(result)
}
