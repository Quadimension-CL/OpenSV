use super::{DurationPrediction, DurationRecord, PhoneDuration, frontend::PhoneAllocations};
use crate::synthesis::voice::{Result, VoiceError};

pub(super) fn allocate_boundaries(
    records: &[DurationRecord],
    mut durations: Vec<PhoneDuration>,
    step: f64,
    phrase_start_seconds: f64,
) -> Result<PhoneAllocations> {
    if !phrase_start_seconds.is_finite() {
        return Err(VoiceError::new("Invalid phoneme timeline origin"));
    }
    let phones: Vec<_> = records.iter().flat_map(|record| &record.phones).collect();
    if phones.len() != durations.len() {
        return Err(VoiceError::new(
            "Phoneme control count differs from its duration allocation",
        ));
    }
    let mut default_durations = durations.clone();
    let mut offsets = Vec::with_capacity(phones.len());
    let mut predicted_start = 0.0;
    for (phone, duration) in phones.iter().zip(&durations) {
        phone.attributes.validate().map_err(VoiceError::new)?;
        let offset = if let Some(left) = phone.attributes.left_offset {
            let source_onset = phone
                .source_note_onset_seconds
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    VoiceError::new("A source-note phoneme boundary requires a finite source onset")
                })?;
            (source_onset - phrase_start_seconds) + left - predicted_start
        } else {
            0.0
        };
        if !offset.is_finite() {
            return Err(VoiceError::new("Non-finite phoneme onset offset"));
        }
        offsets.push(offset);
        predicted_start += duration.duration_seconds;
    }
    apply_frames(&mut default_durations, step, None)?;
    apply_frames(&mut durations, step, Some(&offsets))?;
    Ok(PhoneAllocations {
        default_durations,
        durations,
    })
}

fn apply_frames(durations: &mut [PhoneDuration], step: f64, offsets: Option<&[f64]>) -> Result<()> {
    let mut seconds = 0.0;
    let mut previous = 0_usize;
    for (index, phone) in durations.iter_mut().enumerate() {
        seconds += phone.duration_seconds;
        let offset = offsets
            .and_then(|values| values.get(index + 1))
            .copied()
            .unwrap_or(0.0);
        let boundary = super::frontend::rounded_frames((seconds + offset) / step);
        if !boundary.is_finite() || boundary > i32::MAX as f64 {
            return Err(VoiceError::new(
                "Phoneme boundary exceeds supported frame range",
            ));
        }
        let minimum = if index == 0 { 0 } else { previous + 1 };
        let end = (boundary.max(0.0) as usize).max(minimum);
        phone.frames = end - previous;
        previous = end;
    }
    Ok(())
}

pub(super) fn apply_impulses(
    records: &[DurationRecord],
    prediction: &mut DurationPrediction,
) -> Result<()> {
    let supports_impulses = !prediction.impulse_values.is_empty();
    for (index, phone) in records.iter().flat_map(|record| &record.phones).enumerate() {
        phone.attributes.validate().map_err(VoiceError::new)?;
        if phone.attributes.activity.is_some() || phone.attributes.position.is_some() {
            if !supports_impulses {
                return Err(VoiceError::new(
                    "This voice does not support phoneme activity or position controls",
                ));
            }
            if let Some(value) = phone.attributes.activity {
                *prediction
                    .impulse_values
                    .get_mut(index)
                    .ok_or_else(|| VoiceError::new("Phoneme activity prediction is missing"))? =
                    value;
            }
            if let Some(value) = phone.attributes.position {
                *prediction
                    .impulse_positions
                    .get_mut(index)
                    .ok_or_else(|| VoiceError::new("Phoneme position prediction is missing"))? =
                    value as f32;
            }
        }
    }
    Ok(())
}
