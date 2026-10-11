//! Pitch-control sampling shared by full, progressive and local synthesis.

use crate::synthesis::{
    score::PreparedPhrase,
    stages::f0::{PitchResult, frontend::frame_count},
    voice::{Result, VoiceError},
};

pub(super) fn sv2_controls(phrase: &PreparedPhrase) -> Result<Option<Vec<f32>>> {
    let Some(controls) = &phrase.pitch_controls else {
        return Ok(None);
    };
    let count = phrase.notes.iter().try_fold(0_usize, |count, note| {
        count
            .checked_add(frame_count(note)?)
            .ok_or_else(|| VoiceError::new("Pitch control frame count overflow"))
    })?;
    controls
        .sample(
            phrase.start_seconds,
            crate::synthesis::score::FRAME_SECONDS,
            count,
        )
        .map(Some)
}

pub(super) fn expressiveness(
    phrase: &PreparedPhrase,
    step: f64,
    count: usize,
) -> Result<Option<Vec<f32>>> {
    phrase
        .performance_controls
        .as_ref()
        .map(|controls| controls.sample(phrase.start_seconds, step, count))
        .transpose()
}

/// The DDS adapter consumes absolute curves after its native F0 prediction.
/// It overwrites only specified frames; SV2's neural conditioning and 15-frame
/// control blending are deliberately not substituted for this contract.
pub(super) fn apply_dds_curves(phrase: &PreparedPhrase, pitch: &mut PitchResult) -> Result<()> {
    let Some(controls) = &phrase.pitch_controls else {
        return Ok(());
    };
    let controls = controls.sample(phrase.start_seconds, pitch.frame_seconds, pitch.pitch.len())?;
    for (value, control) in pitch.pitch.iter_mut().zip(controls) {
        if control.is_finite() {
            *value = control;
        }
    }
    Ok(())
}
