use super::{RenderedPhoneme, RenderedPhonemeTrack};
use crate::synthesis::{
    score::{FRAME_SECONDS, PreparedPhrase},
    stages::duration::DurationResult,
    voice::{Result, VoiceError},
};
use std::collections::BTreeSet;

impl RenderedPhonemeTrack {
    pub fn from_phrase(phrase: &PreparedPhrase, duration: &DurationResult) -> Result<Self> {
        let count = duration.durations.len();
        let prediction = &duration.prediction;
        if !phrase.start_seconds.is_finite() || duration.default_durations.len() != count {
            return Err(VoiceError::new(
                "Rendered phoneme timings differ from their source phrase",
            ));
        }
        let has_impulse_heads = !prediction.impulse_values.is_empty();
        let impulse_count = if has_impulse_heads { count } else { 0 };
        if [
            prediction.impulse_values.len(),
            prediction.impulse_positions.len(),
            prediction.default_impulse_values.len(),
            prediction.default_impulse_positions.len(),
        ]
        .iter()
        .any(|&length| length != impulse_count)
        {
            return Err(VoiceError::new(
                "Rendered phoneme impulse arrays have different lengths",
            ));
        }

        let mut phones = Vec::with_capacity(count);
        let mut frames = 0_usize;
        let mut default_frames = 0_usize;
        let mut sources = BTreeSet::new();
        for (index, (actual, default)) in duration
            .durations
            .iter()
            .zip(&duration.default_durations)
            .enumerate()
        {
            if actual.record_index != default.record_index
                || actual.phone_index != default.phone_index
            {
                return Err(VoiceError::new(
                    "Rendered phoneme defaults refer to another source phone",
                ));
            }
            let source = phrase
                .records
                .get(actual.record_index)
                .and_then(|record| record.phones.get(actual.phone_index))
                .ok_or_else(|| VoiceError::new("Rendered phoneme source is outside its phrase"))?;
            let begin_seconds = seconds_at(phrase.start_seconds, frames)?;
            let default_begin_seconds = seconds_at(phrase.start_seconds, default_frames)?;
            frames = frames
                .checked_add(actual.frames)
                .ok_or_else(|| VoiceError::new("Rendered phoneme frame count overflows"))?;
            default_frames = default_frames
                .checked_add(default.frames)
                .ok_or_else(|| VoiceError::new("Rendered phoneme default frame count overflows"))?;
            let end_seconds = seconds_at(phrase.start_seconds, frames)?;
            let default_end_seconds = seconds_at(phrase.start_seconds, default_frames)?;
            let Some(identity) = &source.source else {
                continue;
            };
            if !sources.insert((identity.note_id, identity.phone_index)) {
                return Err(VoiceError::new(
                    "Rendered phoneme source identity is duplicated",
                ));
            }
            let (activity, position, default_activity, default_position) = if has_impulse_heads {
                let values = [
                    prediction.impulse_values[index],
                    f64::from(prediction.impulse_positions[index]),
                    prediction.default_impulse_values[index],
                    f64::from(prediction.default_impulse_positions[index]),
                ];
                if values.iter().any(|value| !value.is_finite()) {
                    return Err(VoiceError::new(
                        "Rendered phoneme impulse value is non-finite",
                    ));
                }
                (
                    Some(values[0]),
                    Some(values[1]),
                    Some(values[2]),
                    Some(values[3]),
                )
            } else {
                (None, None, None, None)
            };
            phones.push(RenderedPhoneme {
                source: *identity,
                token: source.token.clone(),
                language: source.language.clone(),
                begin_seconds,
                end_seconds,
                default_begin_seconds,
                default_end_seconds,
                activity,
                position,
                strength: duration
                    .supports_strength
                    .then_some(source.attributes.strength.unwrap_or(0.0)),
                default_activity,
                default_position,
                default_strength: duration.supports_strength.then_some(0.0),
                supports_timing: duration.supports_timing,
                supports_impulse: has_impulse_heads
                    && (default_activity.is_some_and(|value| value > 0.0)
                        || source.attributes.activity.is_some()),
            });
        }
        Ok(Self {
            phones: phones.into(),
        })
    }
}

fn seconds_at(origin: f64, frames: usize) -> Result<f64> {
    let seconds = origin + frames as f64 * FRAME_SECONDS;
    if !seconds.is_finite() {
        return Err(VoiceError::new("Rendered phoneme time exceeds its clock"));
    }
    Ok(seconds)
}
