use super::Contract;
use crate::synthesis::{
    stages::f0::PitchResult,
    voice::{Result, VoiceError},
};

/// Preserve physical pitch until the acoustic output inserts it into vocoder
/// channel 1. Interpolate in Hz rather than MIDI values.
pub(super) fn sample_log_hz(
    pitch: &PitchResult,
    frames: usize,
    frame_seconds: f32,
    contract: Contract,
) -> Result<Vec<f32>> {
    if pitch.pitch.is_empty()
        || pitch.pitch.iter().any(|value| !value.is_finite())
        || !pitch.frame_seconds.is_finite()
        || pitch.frame_seconds <= 0.0
    {
        return Err(VoiceError::new("Invalid DDS acoustic pitch timeline"));
    }
    let output = match contract {
        Contract::Compatible => {
            // The compatibility path consumes MIDI rows directly, truncating
            // or holding the final sample to match the acoustic phone timeline.
            (0..frames)
                .map(|frame| {
                    let midi = pitch.pitch[frame.min(pitch.pitch.len() - 1)];
                    (((midi - 69.0) / 12.0).exp2() * 440.0).ln()
                })
                .collect::<Vec<_>>()
        }
        Contract::Native => native(pitch, frames, f64::from(frame_seconds)),
    };
    if output.iter().any(|value| !value.is_finite()) {
        return Err(VoiceError::new(
            "DDS acoustic pitch produced invalid log-Hz",
        ));
    }
    Ok(output)
}

fn native(pitch: &PitchResult, frames: usize, frame_seconds: f64) -> Vec<f32> {
    let hz: Vec<_> = pitch
        .pitch
        .iter()
        .map(|&midi| 440.0 * ((f64::from(midi) - 69.0) / 12.0).exp2())
        .collect();
    let mut right = 0;
    (0..frames)
        .map(|frame| {
            // Promote the frontend f32 hop without recomputing it in f64.
            // Its exact multiples preserve the compensated accumulator's timestamps.
            let time = frame as f64 * frame_seconds;
            while right < hz.len() && right as f64 * pitch.frame_seconds < time {
                right += 1;
            }
            let (left, right) = if right == hz.len() {
                (hz.len() - 1, hz.len() - 1)
            } else {
                (right.saturating_sub(1), right)
            };
            let left_time = left as f64 * pitch.frame_seconds;
            let span = (right - left) as f64 * pitch.frame_seconds;
            let mut weight = ((time - left_time) / (span + 0.00001)).clamp(0.0, 1.0);
            if span > 0.05 {
                let exponential = (20.0 * weight - 10.0).exp();
                weight = exponential / (1.0 + exponential);
            }
            // Blend Hz values in f64, then round to f32 before the logarithm.
            // Do not introduce an intermediate conversion back to MIDI.
            ((hz[left] * (1.0 - weight) + hz[right] * weight) as f32).ln()
        })
        .collect()
}
