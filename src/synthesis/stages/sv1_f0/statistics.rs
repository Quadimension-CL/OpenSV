//! Default sampler diagnostics from vowel-restricted syllable statistics.

use super::Contract;

use crate::synthesis::{
    stages::f0::{frontend::FrontendNote, statistics::SamplerStatistics},
    voice::{Result, VoiceError},
};

pub(super) fn estimate(
    pitch: &[f32],
    vowel_mask: &[bool],
    notes: &[FrontendNote],
    note_frames: &[usize],
    transpose: f32,
    contract: Contract,
) -> Result<SamplerStatistics> {
    let mut result = SamplerStatistics {
        slope: vec![0.0; pitch.len()],
        offset: vec![0.0; pitch.len()],
    };
    let mut begin = 0;
    for (note, &frames) in notes.iter().zip(note_frames) {
        let end = begin + frames;
        if end > pitch.len() {
            return Err(VoiceError::new(
                "Compatible F0 statistics exceed its pitch grid",
            ));
        }
        let mut levels = Vec::new();
        let mut differences = Vec::new();
        for frame in begin..end {
            if vowel_mask.get(frame).copied().unwrap_or(false) {
                levels.push(pitch[frame]);
                differences.push(if frame > 0 {
                    pitch[frame] - pitch[frame - 1]
                } else {
                    0.0
                });
            }
        }
        if !levels.is_empty() {
            let median = f64::from(percentile(&mut differences, 0.5));
            let count = levels.len() as f64;
            let slope = match contract {
                Contract::Compatible => count * (median * 0.05),
                Contract::Native { .. } => (count * 0.05) * median,
            } as f32;
            let offset =
                percentile(&mut levels, 0.8) - (note.pitch + note.pitch_offset + transpose);
            result.slope[begin..end].fill(slope);
            result.offset[begin..end].fill(offset);
        }
        begin = end;
    }
    Ok(result)
}

fn percentile(values: &mut [f32], quantile: f32) -> f32 {
    let index = ((values.len() - 1) as f32 * quantile) as usize;
    *values.select_nth_unstable_by(index, f32::total_cmp).1
}
