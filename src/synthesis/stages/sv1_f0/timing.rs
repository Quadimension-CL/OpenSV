use crate::synthesis::{
    stages::f0::frontend::FrontendNote,
    voice::{Result, VoiceError},
};

pub(super) fn duration_seconds(note: &FrontendNote) -> Result<f64> {
    if note.parts == 0
        || note.part_index >= note.parts
        || !note.duration_seconds.is_finite()
        || note.duration_seconds < 0.0
    {
        return Err(VoiceError::new("Invalid native DDS note duration"));
    }
    let mut duration = note.duration_seconds / f64::from(note.parts);
    for &extra in &note.appended_durations {
        if !extra.is_finite() || extra < 0.0 {
            return Err(VoiceError::new("Invalid native DDS linked note duration"));
        }
        duration += extra;
    }
    if !duration.is_finite() {
        return Err(VoiceError::new("Native DDS note duration overflow"));
    }
    Ok(duration)
}

pub(super) fn note_frame_counts(notes: &[FrontendNote], step: f32) -> Result<Vec<usize>> {
    let mut elapsed = 0.0_f32;
    let inverse = 1.0_f32 / step;
    let inverse_double = 1.0 / f64::from(step);
    notes
        .iter()
        .map(|note| {

            let left = frame(f64::from((elapsed * inverse).round()))?;
            let end = f64::from(elapsed) + duration_seconds(note)?;
            let right = frame((end * inverse_double).round())?;
            elapsed = end as f32;
            right
                .checked_sub(left)
                .ok_or_else(|| VoiceError::new("Native DDS note clock moved backwards"))
        })
        .collect()
}

pub(super) fn phone_frame_counts(seconds: &[f64], step: f32) -> Result<Vec<usize>> {
    let mut elapsed = 0.0_f32;
    seconds
        .iter()
        .map(|&duration| {
            if !duration.is_finite() || duration < 0.0 || duration > f64::from(f32::MAX) {
                return Err(VoiceError::new("Invalid native DDS phone duration"));
            }
            let left = frame(f64::from((elapsed / step).round()))?;
            elapsed += duration as f32;
            let right = frame(f64::from((elapsed / step).round()))?;
            right
                .checked_sub(left)
                .ok_or_else(|| VoiceError::new("Native DDS phone clock moved backwards"))
        })
        .collect()
}

fn frame(value: f64) -> Result<usize> {
    if !value.is_finite() || value < 0.0 || value >= f64::from(i32::MAX) {
        return Err(VoiceError::new(
            "Native DDS frame count exceeds its native range",
        ));
    }
    Ok(value as usize)
}
