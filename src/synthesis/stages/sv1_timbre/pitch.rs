use super::Contract;
use crate::synthesis::{
    stages::{f0::PitchResult, timbre::records::FrameControl},
    voice::{Result, VoiceError},
};

pub(super) struct PitchFrames {
    pub raw_log_hz: Vec<f32>,
    pub conditioning_log_hz: Vec<f32>,
}

/// Interpolate physical pitch in Hz. Acoustic pitch conditioning is separate
/// from the original pitch later supplied to the vocoder.
pub(super) fn sample(
    pitch: &PitchResult,
    frames: usize,
    frame_seconds: f32,
    contract: Contract,
    controls: Option<&[FrameControl]>,
) -> Result<PitchFrames> {
    if pitch.pitch.is_empty()
        || pitch.pitch.iter().any(|value| !value.is_finite())
        || !pitch.frame_seconds.is_finite()
        || pitch.frame_seconds <= 0.0
    {
        return Err(VoiceError::new("Invalid DDS acoustic pitch timeline"));
    }
    validate_controls(controls, frames)?;
    let raw_hz = match contract {
        Contract::Compatible => {
            // The compatibility path consumes MIDI rows directly, truncating
            // or holding the final sample to match the acoustic phone timeline.
            (0..frames)
                .map(|frame| {
                    let midi = pitch.pitch[frame.min(pitch.pitch.len() - 1)];
                    f64::from(((midi - 69.0) / 12.0).exp2() * 440.0)
                })
                .collect::<Vec<_>>()
        }
        Contract::Native => native(pitch, frames, f64::from(frame_seconds)),
    };
    let raw_log_hz: Vec<_> = raw_hz.iter().map(|&hz| (hz as f32).ln()).collect();
    if raw_log_hz.iter().any(|value| !value.is_finite()) {
        return Err(VoiceError::new(
            "DDS acoustic pitch produced invalid log-Hz",
        ));
    }
    let conditioning_log_hz = if let Some(controls) = controls {
        raw_hz
            .iter()
            .zip(&raw_log_hz)
            .zip(controls)
            .map(|((&hz, &log_hz), control)| {
                let shifted_log = match contract {
                    Contract::Compatible => log_hz + control.pitch_ratio.ln(),
                    Contract::Native => ((hz * f64::from(control.pitch_ratio)) as f32).ln(),
                };
                if !shifted_log.is_finite() {
                    return Err(VoiceError::new(
                        "DDS acoustic pitch ratio produced invalid conditioning pitch",
                    ));
                }
                Ok(shifted_log)
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        raw_log_hz.clone()
    };
    Ok(PitchFrames {
        raw_log_hz,
        conditioning_log_hz,
    })
}

fn validate_controls(controls: Option<&[FrameControl]>, frames: usize) -> Result<()> {
    let Some(controls) = controls else {
        return Ok(());
    };
    if controls.len() != frames {
        return Err(VoiceError::new(
            "DDS frame controls do not match the acoustic timeline",
        ));
    }
    for control in controls {
        if !control.pitch_ratio.is_finite()
            || control.pitch_ratio <= 0.0
            || control.scalars.iter().any(|value| !value.is_finite())
        {
            return Err(VoiceError::new(
                "DDS frame controls require a finite positive pitch ratio and finite scalars",
            ));
        }
        if control.scalars.iter().any(|&value| value != 0.0) {
            return Err(VoiceError::new(
                "DDS voices do not support acoustic frame scalar controls",
            ));
        }
    }
    Ok(())
}

fn native(pitch: &PitchResult, frames: usize, frame_seconds: f64) -> Vec<f64> {
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
            // Retain the f64 interpolation for frame conditioning. Both pitch
            // paths round to f32 before their logarithms.
            hz[left] * (1.0 - weight) + hz[right] * weight
        })
        .collect()
}
