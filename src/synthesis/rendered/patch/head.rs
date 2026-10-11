use super::{
    HeadSplice, MAX_FADE_SAMPLES, Result, VoiceError, absolute_difference, validate_block_size,
    validate_components,
};

/// Join the replacement to the preceding old block. Each fresh component starts
/// at that preceding block, not the first changed block, and includes
/// at least two blocks of new PCM. At project block zero the caller bypasses
/// this function because there is no old preceding block.
///
/// Positive alignment shifts consume lookahead and shorten the valid
/// result by that many samples. No padding or invented tail samples are added.
pub fn splice_head(
    old_previous: [&[f32]; 2],
    fresh: [&[f32]; 2],
    block_samples: usize,
) -> Result<HeadSplice> {
    validate_block_size(block_samples)?;
    validate_components(old_previous)?;
    validate_components(fresh)?;
    if old_previous[0].len() != block_samples || fresh[0].len() < block_samples * 2 {
        return Err(VoiceError::new(
            "PCM head seam requires one old block and at least two new blocks",
        ));
    }
    let fade = block_samples.min(MAX_FADE_SAMPLES);
    let radius = ((block_samples / 2).min(fade * 2) / 2) as i32;
    let mut shift = 0_i32;
    let mut best = 1e9_f32;
    let seam_start = block_samples - fade;
    if seam_start < radius as usize {
        return Err(VoiceError::new(
            "PCM head block cannot hold its alignment context",
        ));
    }
    for candidate in -radius..radius {
        let offset = (seam_start as i64 + i64::from(candidate)) as usize;
        let distance = absolute_difference(
            &old_previous[0][seam_start..],
            &fresh[0][offset..offset + fade],
        ) / fade as f32;
        let score = (f64::from(distance)
            - 0.1 * (-0.00001 * f64::from(candidate) * f64::from(candidate)).exp())
            as f32;
        if score < best {
            best = score;
            shift = candidate;
        }
    }
    let mixed = fade.min((block_samples as i64 + i64::from(shift)) as usize);
    let first_new = block_samples - mixed;
    let valid_len = fresh[0].len() - shift.max(0) as usize;
    let source_start = (first_new as i64 + i64::from(shift)) as usize;
    let samples = std::array::from_fn(|channel| {
        let mut samples = Vec::with_capacity(valid_len);
        samples.extend_from_slice(&old_previous[channel][..first_new]);
        samples
            .extend_from_slice(&fresh[channel][source_start..source_start + valid_len - first_new]);
        let step = 1.0_f32 / mixed as f32;
        let mut old_gain = 1.0_f32;
        let mut new_gain = 0.0_f32;
        for index in first_new..block_samples {
            samples[index] = old_previous[channel][index] * old_gain + samples[index] * new_gain;
            old_gain -= step;
            new_gain += step;
        }
        samples
    });
    Ok(HeadSplice {
        samples,
        shift_samples: shift,
    })
}
