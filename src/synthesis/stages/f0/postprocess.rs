//! F0 control blending and per-note output. These operations do not generate
//! a fundamental frequency.

use std::fmt;

/// Frame duration: 240 samples at 48 kHz.
pub const FRAME_SECONDS: f64 = 240.0 / 48_000.0;
/// Result context on each side of a note, not an edit invalidation radius.
pub const NOTE_CONTEXT_FRAMES: usize = 100;

// Left/right float32 weights differ slightly; evaluating a symmetric
// cosine would lose those roundings.
const LEFT_WEIGHTS: [u32; 15] = [
    0x3f7d_8a60,
    0x3f76_41b0,
    0x3f6a_6d99,
    0x3f5a_827a,
    0x3f47_1cee,
    0x3f30_fbc6,
    0x3f18_f8b8,
    0x3f00_0000,
    0x3ece_0e91,
    0x3e9e_0876,
    0x3e63_8c4e,
    0x3e15_f61a,
    0x3dac_933c,
    0x3d1b_e510,
    0x3c1d_6840,
];
const RIGHT_WEIGHTS: [u32; 15] = [
    0x3f7d_8a5f,
    0x3f76_41af,
    0x3f6a_6d98,
    0x3f5a_8278,
    0x3f47_1ceb,
    0x3f30_fbc3,
    0x3f18_f8b9,
    0x3f00_0000,
    0x3ece_0e8e,
    0x3e9e_0872,
    0x3e63_8c46,
    0x3e15_f612,
    0x3dac_933c,
    0x3d1b_e508,
    0x3c1d_6820,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PostprocessError {
    LengthMismatch { generated: usize, controls: usize },
    NonFiniteGenerated { frame: usize },
    InfiniteControl { frame: usize },
    InvalidNote { note: usize },
    EmptyGenerated,
    FrameCountOverflow,
    AllocationFailed,
}

impl fmt::Display for PostprocessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthMismatch {
                generated,
                controls,
            } => write!(
                f,
                "F0 control length {controls} differs from generated length {generated}"
            ),
            Self::NonFiniteGenerated { frame } => {
                write!(f, "Non-finite generated pitch at frame {frame}")
            }
            Self::InfiniteControl { frame } => write!(f, "Infinite F0 control at frame {frame}"),
            Self::InvalidNote { note } => write!(f, "Invalid F0 duration or pitch at note {note}"),
            Self::EmptyGenerated => f.write_str("A nonzero note requires generated F0 frames"),
            Self::FrameCountOverflow => f.write_str("F0 frame count exceeds supported indexing"),
            Self::AllocationFailed => f.write_str("Could not allocate F0 output frames"),
        }
    }
}

impl std::error::Error for PostprocessError {}

/// Blends absolute pitch controls onto actual model output, in the same pitch
/// unit as that output. NaN means an absent control, not an unvoiced frame.
/// The caller supplies an already aligned control frame for every model frame.
/// All argument validation happens before `generated` is modified.
pub fn mix_controls(generated: &mut [f32], controls: &[f32]) -> Result<(), PostprocessError> {
    if generated.len() != controls.len() {
        return Err(PostprocessError::LengthMismatch {
            generated: generated.len(),
            controls: controls.len(),
        });
    }
    validate_generated(generated)?;
    if let Some(frame) = controls.iter().position(|value| value.is_infinite()) {
        return Err(PostprocessError::InfiniteControl { frame });
    }
    let Some(first) = controls.iter().position(|value| !value.is_nan()) else {
        return Ok(());
    };
    let mut weights = allocated_frames(generated.len(), 0.0)?;
    let mut residuals = allocated_frames(generated.len(), f32::NAN)?;
    for (frame, &control) in controls.iter().enumerate() {
        if control.is_nan() {
            continue;
        }
        residuals[frame] = control - generated[frame];
        weights[frame] += 1.0;
        for (offset, &bits) in RIGHT_WEIGHTS.iter().enumerate() {
            if let Some(weight) = weights.get_mut(frame + offset + 1) {
                *weight += f32::from_bits(bits);
            }
        }
        for (offset, &bits) in LEFT_WEIGHTS.iter().enumerate() {
            if let Some(target) = frame.checked_sub(offset + 1) {
                weights[target] += f32::from_bits(bits);
            }
        }
    }

    let initial = residuals[first];
    residuals[..first].fill(initial);
    let mut previous = first;
    for frame in first + 1..residuals.len() {
        if controls[frame].is_nan() {
            continue;
        }
        let start = residuals[previous];
        let difference = residuals[frame] - start;
        let span = (frame - previous) as f32;
        for offset in 1..frame - previous {
            residuals[previous + offset] = (offset as f32 / span) * difference + start;
        }
        previous = frame;
    }
    let final_value = residuals[previous];
    residuals[previous + 1..].fill(final_value);
    for ((output, &weight), &residual) in generated.iter_mut().zip(&weights).zip(&residuals) {
        *output += weight.min(1.0) * residual;
    }
    Ok(())
}

/// Duration and note pitch from the prepared, sequential synthesis records.
/// Include any rest records that advance the synthesis cursor. Editor note
/// onsets alone are insufficient to construct these records.
#[derive(Clone, Copy, Debug)]
pub struct NotePitchSpan {
    pub duration_seconds: f64,
    /// Same pitch unit as the generated sequence; no Hz conversion is implied.
    pub pitch: f32,
}

#[derive(Clone, Debug)]
pub struct NotePitchCurve {
    /// Index of the first sample in the phrase's 5 ms time grid. May be negative.
    pub first_frame: i64,
    pub body_frames: usize,
    /// Relative to `NotePitchSpan::pitch`, with 100 frames of context per side.
    pub pitch_offsets: Vec<f32>,
}

/// Splits actual generated output by individually rounded note durations.
/// Out-of-range context uses the global sequence's nearest endpoint. The
/// zero-frame branch leaves its 200 context samples at zero.
pub fn split_note_curves(
    generated: &[f32],
    notes: &[NotePitchSpan],
) -> Result<Vec<NotePitchCurve>, PostprocessError> {
    validate_generated(generated)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(notes.len())
        .map_err(|_| PostprocessError::AllocationFailed)?;
    let mut cursor = 0_i64;
    for (note, span) in notes.iter().enumerate() {
        if !span.duration_seconds.is_finite()
            || span.duration_seconds < 0.0
            || !span.pitch.is_finite()
        {
            return Err(PostprocessError::InvalidNote { note });
        }
        let rounded = (span.duration_seconds / FRAME_SECONDS).round();
        if rounded >= i32::MAX as f64 {
            return Err(PostprocessError::FrameCountOverflow);
        }
        let body_frames = rounded as usize;
        if body_frames > 0 && generated.is_empty() {
            return Err(PostprocessError::EmptyGenerated);
        }
        let length = body_frames
            .checked_add(2 * NOTE_CONTEXT_FRAMES)
            .ok_or(PostprocessError::FrameCountOverflow)?;
        let first_frame = cursor - NOTE_CONTEXT_FRAMES as i64;
        let mut pitch_offsets = allocated_frames(length, 0.0)?;
        if body_frames > 0 {
            for (offset, value) in pitch_offsets.iter_mut().enumerate() {
                let frame = (first_frame + offset as i64).clamp(0, generated.len() as i64 - 1);
                *value = generated[frame as usize] - span.pitch;
            }
        }
        cursor = cursor
            .checked_add(body_frames as i64)
            .filter(|&frames| frames < i32::MAX as i64)
            .ok_or(PostprocessError::FrameCountOverflow)?;
        result.push(NotePitchCurve {
            first_frame,
            body_frames,
            pitch_offsets,
        });
    }
    Ok(result)
}

fn validate_generated(generated: &[f32]) -> Result<(), PostprocessError> {
    if let Some(frame) = generated.iter().position(|value| !value.is_finite()) {
        return Err(PostprocessError::NonFiniteGenerated { frame });
    }
    Ok(())
}

fn allocated_frames(length: usize, value: f32) -> Result<Vec<f32>, PostprocessError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(length)
        .map_err(|_| PostprocessError::AllocationFailed)?;
    result.resize(length, value);
    Ok(result)
}
