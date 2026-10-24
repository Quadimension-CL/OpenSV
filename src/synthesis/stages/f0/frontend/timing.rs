use crate::synthesis::voice::{Result, VoiceError};

use super::FrontendNote;

const ROUNDING_BIAS: f64 = f64::from_bits(0x3fdf_ffff_ffff_ffff);

fn rounded_frames(value: f64) -> Result<usize> {
    if !value.is_finite() || value < 0.0 {
        return Err(VoiceError::new("Invalid note frame count"));
    }
    let frames = (value + ROUNDING_BIAS).trunc();
    if frames >= u32::MAX as f64 {
        return Err(VoiceError::new(
            "Note frame count exceeds the model's u32 range",
        ));
    }
    Ok(frames as usize)
}

pub(crate) fn duration_seconds(note: &FrontendNote) -> Result<f64> {
    if note.parts == 0
        || note.part_index >= note.parts
        || !note.duration_seconds.is_finite()
        || note.duration_seconds < 0.0
        || !note.frame_seconds.is_finite()
        || note.frame_seconds <= 0.0
    {
        return Err(VoiceError::new("Invalid note conditioning timing"));
    }
    let parts = f64::from(note.parts);
    let left = rounded_frames(
        note.duration_seconds * (f64::from(note.part_index) / parts) / note.frame_seconds,
    )?;
    let right = rounded_frames(
        note.duration_seconds * (f64::from(note.part_index + 1) / parts) / note.frame_seconds,
    )?;
    let mut duration = (right - left) as f64 * note.frame_seconds;
    for &extra in &note.appended_durations {
        if !extra.is_finite() || extra < 0.0 {
            return Err(VoiceError::new("Invalid associated note duration"));
        }
        duration += extra;
    }
    if !duration.is_finite() {
        return Err(VoiceError::new("Note conditioning duration overflow"));
    }
    Ok(duration)
}

pub fn frame_count(note: &FrontendNote) -> Result<usize> {
    rounded_frames(duration_seconds(note)? / note.frame_seconds)
}
