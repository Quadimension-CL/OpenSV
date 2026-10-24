use crate::synthesis::voice::{Result, VoiceError};

pub fn pitch_embedding(normalized_pitch: &[f32], channels: usize) -> Result<Vec<f32>> {
    if channels == 0 || normalized_pitch.iter().any(|x| !x.is_finite()) {
        return Err(VoiceError::new(
            "Timbre pitch embedding requires finite pitch and nonzero width",
        ));
    }
    let length = normalized_pitch
        .len()
        .checked_mul(channels)
        .ok_or_else(|| VoiceError::new("Timbre pitch embedding size overflows"))?;
    let mut output = Vec::with_capacity(length);
    for &pitch in normalized_pitch {
        for channel in 0..channels {
            let phase = (pitch - (channel as f32 - 1.0) / channels as f32) * std::f32::consts::TAU;
            output.push(phase.cos() * 0.5 + 0.5);
        }
    }
    Ok(output)
}

pub fn assemble_normalized_acoustics(spectral: &[f32], auxiliary: &[f32]) -> Result<Vec<f32>> {
    if !spectral.len().is_multiple_of(64) || auxiliary.len() != spectral.len() / 64 * 7 {
        return Err(VoiceError::new(
            "Timbre heads require matching 64- and 7-channel frames",
        ));
    }
    let frames = spectral.len() / 64;
    let mut output = Vec::with_capacity(frames * 71);
    for (spectrum, extra) in spectral
        .as_chunks::<64>()
        .0
        .iter()
        .zip(auxiliary.as_chunks::<7>().0)
    {
        output.extend_from_slice(&extra[..2]);
        output.extend_from_slice(spectrum);
        output.extend_from_slice(&extra[2..]);
    }
    Ok(output)
}

pub fn insert_vocoder_pitch(acoustics: &[f32], pitch_hz: &[f32]) -> Result<Vec<f32>> {
    if !acoustics.len().is_multiple_of(71) || acoustics.len() / 71 != pitch_hz.len() {
        return Err(VoiceError::new(
            "Vocoder pitch length differs from acoustic frame count",
        ));
    }
    if pitch_hz.iter().any(|x| !x.is_finite() || *x <= 0.0)
        || acoustics.iter().any(|x| !x.is_finite())
    {
        return Err(VoiceError::new(
            "Vocoder conditions contain invalid numerical values",
        ));
    }
    let mut output = Vec::with_capacity(pitch_hz.len() * 72);
    for (frame, &pitch) in acoustics.as_chunks::<71>().0.iter().zip(pitch_hz) {
        output.push(frame[0]);
        output.push(pitch);
        output.extend_from_slice(&frame[1..]);
    }
    Ok(output)
}
