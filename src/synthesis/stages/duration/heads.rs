use crate::synthesis::{
    inference::ops::softmax,
    voice::{Result, VoiceError},
};

pub(super) fn maximum_index(values: &[f32], bins: usize, frames: usize) -> Result<Vec<f64>> {
    validate(values, bins, frames)?;
    Ok(values
        .chunks_exact(bins)
        .map(|frame| {
            let mut best = 0;

            for index in 1..bins {
                if frame[index] > frame[best] {
                    best = index;
                }
            }
            best as f64 / (bins - 1) as f64
        })
        .collect())
}

pub(super) fn expected_index(values: &[f32], bins: usize, frames: usize) -> Result<Vec<f32>> {
    validate(values, bins, frames)?;
    let mut output = Vec::with_capacity(frames);
    for frame in values.chunks_exact(bins) {

        let mut weights = frame.to_vec();
        softmax::execute(&mut weights).map_err(|error| VoiceError::new(error.to_string()))?;
        for (index, weight) in weights.iter_mut().enumerate() {
            *weight *= index as f32;
        }
        output.push(sum(&weights) / (bins - 1) as f32);
    }
    Ok(output)
}

fn sum(values: &[f32]) -> f32 {
    let mut lanes = [0.0_f32; 8];
    let (blocks, remainder) = values.as_chunks::<8>();
    for block in blocks {
        for (sum, value) in lanes.iter_mut().zip(block) {
            *sum += value;
        }
    }
    let merged = std::array::from_fn::<_, 4, _>(|index| lanes[index + 4] + lanes[index]);
    let mut total = (merged[3] + merged[1]) + (merged[2] + merged[0]);
    for value in remainder {
        total += value;
    }
    total
}

fn validate(values: &[f32], bins: usize, frames: usize) -> Result<()> {
    if bins < 2
        || frames.checked_mul(bins) != Some(values.len())
        || values.iter().any(|value| !value.is_finite())
    {
        return Err(VoiceError::new(
            "Duration output head has an invalid shape or nonfinite values",
        ));
    }
    Ok(())
}
