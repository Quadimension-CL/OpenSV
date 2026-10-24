use super::{Result, VoiceError};

pub(super) fn encode(descriptors: &[f32; 16]) -> Result<[f32; 73]> {
    let mut row = [0.0; 73];
    for (output, index) in row[..8].iter_mut().zip([1, 2, 3, 4, 7, 11, 12, 15]) {
        let value = descriptors[index];
        *output = if value.is_finite() { value } else { 0.0 };
    }
    for (output, index) in row[8..24].chunks_exact_mut(4).zip([5, 6, 13, 14]) {
        let value = descriptors[index];
        if !value.is_finite() {
            continue;
        }
        let half = value * 0.5;
        let phases = [
            half + 0.5,
            (half + 1.0_f32 / 3.0) + 0.5,
            (half + 2.0_f32 / 3.0) + 0.5,
            (half + 1.0) + 0.5,
        ];
        for (out, phase) in output.iter_mut().zip(phases) {
            *out = (phase * std::f32::consts::PI).cos();
        }
        let mut squares = 0.0_f32;
        for value in output.iter() {
            squares += value * value;
        }
        let scale = 1.0 / squares.sqrt();
        for value in output {
            *value *= scale;
        }
    }
    for (index, offset, width) in [(0, 24, 10), (8, 34, 11), (9, 45, 14), (10, 59, 14)] {
        let value = descriptors[index];
        let value = if value.is_nan() { 0.0 } else { value };
        if !value.is_finite() || value.fract() != 0.0 || value < 0.0 || value >= width as f32 {
            return Err(VoiceError::new("Invalid categorical phonetic descriptor"));
        }
        row[offset + value as usize] = 1.0;
    }
    Ok(row)
}
