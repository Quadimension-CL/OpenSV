//! CPU attention normalization.
//!
//! Uses a log normalizer and a second exponential pass; replacing
//! that pass with division by the exponential sum changes its f32 results.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoftmaxError {
    TooManyValues,
    NonFiniteInput { position: usize },
}

impl fmt::Display for SoftmaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyValues => f.write_str("Attention normalization length exceeds u32"),
            Self::NonFiniteInput { position } => {
                write!(f, "Attention score {position} is not finite")
            }
        }
    }
}

impl std::error::Error for SoftmaxError {}

/// Normalizes one attention row in place. Scores already include attention
/// scaling, relative-position terms, and the finite masking value.
/// Validation finishes before modifying the buffer.
pub fn execute(values: &mut [f32]) -> Result<(), SoftmaxError> {
    let count = u32::try_from(values.len()).map_err(|_| SoftmaxError::TooManyValues)?;
    if let Some(position) = values.iter().position(|value| !value.is_finite()) {
        return Err(SoftmaxError::NonFiniteInput { position });
    }
    match values {
        [] => return Ok(()),
        [value] => {
            *value = 1.0;
            return Ok(());
        }
        _ => {}
    }

    let maximum = values.iter().copied().fold(values[0], f32::max);
    let mut lanes = [0.0_f32; 8];
    let (blocks, remainder) = values.as_chunks::<8>();
    for block in blocks {
        for (sum, &value) in lanes.iter_mut().zip(block) {
            *sum += (value - maximum).exp();
        }
    }
    // Merge the two groups of four accumulation lanes, then reduce
    // (lane 3 + lane 1) + (lane 2 + lane 0), before the scalar remainder.
    let merged = std::array::from_fn::<_, 4, _>(|lane| lanes[lane + 4] + lanes[lane]);
    let mut sum = (merged[3] + merged[1]) + (merged[2] + merged[0]);
    for &value in remainder {
        sum += (value - maximum).exp();
    }
    if sum == 0.0 {
        values.fill(1.0 / count as f32);
        return Ok(());
    }

    let normalizer = maximum + sum.ln();
    for value in values {
        *value = (*value - normalizer).exp();
    }
    Ok(())
}
