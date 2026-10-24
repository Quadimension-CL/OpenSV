use super::super::Contract;

use crate::synthesis::{
    inference::Sequence,
    stages::f0::{frontend::FrontendNote, statistics::SamplerStatistics},
    voice::{Result, VoiceError},
};

pub(super) struct Controls {
    pub slope: Sequence,
    pub slope_weight: Sequence,
    pub offset: Sequence,
    pub offset_weight: Sequence,
}

pub(super) fn prepare(
    notes: &[FrontendNote],
    note_frames: &[usize],
    predicted: &SamplerStatistics,
    contract: Contract,
) -> Result<Option<Controls>> {
    if notes.len() != note_frames.len()
        || predicted.slope.len() != predicted.offset.len()
        || note_frames.iter().sum::<usize>() != predicted.slope.len()
    {
        return Err(VoiceError::new("DDS pitch control grids differ"));
    }
    if !notes
        .iter()
        .any(|note| note.performance.r_intonation.is_some() || note.performance.r_tone.is_some())
    {
        return Ok(None);
    }
    let mut slopes = Vec::with_capacity(notes.len());
    let mut slope_weights = Vec::with_capacity(notes.len());
    let mut offsets = Vec::with_capacity(notes.len());
    let mut offset_weights = Vec::with_capacity(notes.len());
    let mut begin = 0;
    for (note, &frames) in notes.iter().zip(note_frames) {
        let prediction = if frames == 0 {
            (0.0, 0.0)
        } else {
            (predicted.slope[begin], predicted.offset[begin])
        };
        let (slope, slope_weight) = weight(note.performance.r_intonation, prediction.0, 10.0)?;
        let (offset, offset_weight) = weight(note.performance.r_tone, prediction.1, 5.0)?;
        slopes.push(slope);
        slope_weights.push(slope_weight);
        offsets.push(offset);
        offset_weights.push(offset_weight);
        begin += frames;
    }
    Ok(Some(Controls {
        slope: spread(&slopes, note_frames, begin, contract)?,
        slope_weight: spread(&slope_weights, note_frames, begin, contract)?,
        offset: spread(&offsets, note_frames, begin, contract)?,
        offset_weight: spread(&offset_weights, note_frames, begin, contract)?,
    }))
}

fn weight(value: Option<f32>, predicted: f32, scale: f32) -> Result<(f32, f32)> {
    match value {
        Some(value) if value.is_finite() => {
            Ok((value, ((value - predicted).abs() * scale).tanh() * 1.5))
        }
        Some(_) => Err(VoiceError::new("DDS pitch control must be finite")),
        None => Ok((0.0, 0.0)),
    }
}

fn spread(
    values: &[f32],
    note_frames: &[usize],
    total: usize,
    contract: Contract,
) -> Result<Sequence> {
    let mut output = Sequence::zeros(total, 1)?;
    let mut begin = 0_i64;
    for (&value, &frames) in values.iter().zip(note_frames) {
        let frames = frames as i64;
        for relative in -5..frames + 5 {
            let frame = begin + relative;
            if frame < 0 || frame >= total as i64 {
                continue;
            }
            let left = (-(relative as f64)).exp();
            let right = (-((frames - relative) as f64)).exp();

            let envelope = match contract {
                Contract::Compatible => (1.0 / (left + 1.0)) * (1.0 / (right + 1.0)),
                Contract::Native { .. } => 1.0 / ((left + 1.0) * (right + 1.0)),
            } as f32;
            output.values[frame as usize] += envelope * value;
        }
        begin += frames;
    }
    Ok(output)
}
