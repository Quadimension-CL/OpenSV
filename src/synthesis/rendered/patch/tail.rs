use super::{
    MAX_FADE_SAMPLES, Result, TailSplice, VoiceError, absolute_difference, interpolate,
    validate_block_size, validate_components,
};

/// Finish a replacement using anchor search, phase matching, smooth
/// time warp and new-to-old fade. `old` starts at the cache block after any
/// pending partial block has been completed; `fresh` must have that exact same
/// origin and already include the head seam's carried displacement.
///
/// `old` contains complete cache blocks. `minimum_hz` comes from phase records
/// and is zero when no such records are available. It is independent of the
/// vocoder's per-frame metadata and must not be replaced with predicted F0.
/// Only the returned prefix needs to replace old PCM; the rest remains intact.
pub fn splice_tail(
    old: [&[f32]; 2],
    fresh: [&[f32]; 2],
    sample_rate: u32,
    block_samples: usize,
    minimum_hz: f32,
) -> Result<TailSplice> {
    validate_block_size(block_samples)?;
    if sample_rate == 0 || !minimum_hz.is_finite() || minimum_hz < 0.0 {
        return Err(VoiceError::new(
            "PCM tail seam has invalid frequency metadata",
        ));
    }
    validate_components(old)?;
    validate_components(fresh)?;
    if old[0].is_empty() || !old[0].len().is_multiple_of(block_samples) {
        return Err(VoiceError::new(
            "PCM tail seam requires complete old cache blocks",
        ));
    }
    let count = old[0].len().min(fresh[0].len());
    if count > i32::MAX as usize || count / 2 <= 10 {
        return Err(VoiceError::new(
            "PCM tail seam has insufficient comparison context",
        ));
    }
    let fade = (count / 2).min(block_samples.min(MAX_FADE_SAMPLES));
    let raw_period = ((sample_rate as f32 / minimum_hz.max(10.0)) * 0.6) as usize;
    let period = if raw_period == 0 {
        fade / 2
    } else {
        raw_period
    };
    let anchor = choose_anchor(old[0], fresh[0], count, fade, period);
    let left = fade.min(anchor - fade);
    let right = fade.min(count - fade - anchor);
    let (left, right) = if raw_period > 0 {
        (left.min(raw_period), right.min(raw_period))
    } else {
        (left, right)
    };
    let shift = choose_shift(old[0], fresh[0], anchor, fade, left, right);
    let boundary = (anchor as i64 + i64::from(shift)) as usize;
    let end = boundary + fade;
    let published = end.div_ceil(block_samples) * block_samples;
    if boundary <= 10 || published > old[0].len() {
        return Err(VoiceError::new("PCM tail seam exceeds its cache context"));
    }
    let span = boundary as f64 - 10.0;
    let cubic = ((2.0 * f64::from(shift) / span) / span) / span;
    let square = (3.0 * f64::from(shift) / span) / span;
    let mut samples: [Vec<f32>; 2] =
        std::array::from_fn(|channel| old[channel][..published].to_vec());
    for channel in 0..2 {
        let mut warped = Vec::with_capacity(end);
        for index in 0..end {
            let value = if index < 10 {
                fresh[channel][index]
            } else if index >= boundary {
                fresh[channel][(index as i64 - i64::from(shift)) as usize]
            } else {
                let time = index as f64 - 10.0;
                let squared = f64::from((time * time) as f32);
                let position = (cubic * time * squared - squared * square + index as f64) as f32;
                interpolate::sample(fresh[channel], position)?
            };
            warped.push(value);
        }
        samples[channel][..boundary].copy_from_slice(&warped[..boundary]);
        let step = 1.0_f32 / fade as f32;
        let mut old_gain = 0.0_f32;
        let mut new_gain = 1.0_f32;
        for index in boundary..end {
            samples[channel][index] = old[channel][index] * old_gain + warped[index] * new_gain;
            old_gain += step;
            new_gain -= step;
        }
    }
    Ok(TailSplice {
        samples,
        anchor,
        shift_samples: shift,
        fade_samples: fade,
    })
}

fn choose_anchor(old: &[f32], fresh: &[f32], count: usize, fade: usize, period: usize) -> usize {
    let mut anchor = count / 2;
    let Some(limit) = count.checked_sub(fade + period.saturating_mul(2)) else {
        return anchor;
    };
    if anchor >= limit {
        return anchor;
    }
    let mut prefix = vec![0.0_f32; count];
    let mut best = 1e9_f32;
    for phase in 0..10 {
        let offset = ((phase as f64 / 10.0) * period as f64) as usize;
        let mut sum = 0.0_f32;
        for index in 0..count - offset {
            sum += (old[index] - fresh[index + offset]).abs();
            prefix[index] = sum;
        }
        for candidate in count / 2..limit {
            let distance = (prefix[candidate + fade] - prefix[candidate]) / fade as f32;
            let score = distance - 0.00001_f32 * (candidate as f32 / count as f32);
            if score < best {
                best = score;
                anchor = candidate;
            }
        }
    }
    anchor
}

fn choose_shift(
    old: &[f32],
    fresh: &[f32],
    anchor: usize,
    fade: usize,
    left: usize,
    right: usize,
) -> i32 {
    // The extra rightmost zero is intentional: the inclusive vector receives
    // scores over a half-open displacement range before the search selects
    // strict interior local minima. An ordinary global argmin is different.
    let mut scores = vec![0.0_f32; left + right + 1];
    for candidate in -(left as i32)..right as i32 {
        let old_start = (anchor as i64 + i64::from(candidate)) as usize;
        let distance = absolute_difference(
            &fresh[anchor..anchor + fade],
            &old[old_start..old_start + fade],
        ) / fade as f32;
        scores[(candidate + left as i32) as usize] = (f64::from(distance)
            - 0.025 * (-0.00001 * f64::from(candidate) * f64::from(candidate)).exp())
            as f32;
    }
    let mut shift = 0;
    let mut best = 1e9_f32;
    for index in 1..scores.len().saturating_sub(1) {
        if scores[index] < scores[index - 1]
            && scores[index] < scores[index + 1]
            && scores[index] < best
        {
            best = scores[index];
            shift = index as i32 - left as i32;
        }
    }
    shift
}
