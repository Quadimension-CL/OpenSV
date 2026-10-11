use super::{Result, VoiceError};

/// The tail warp's radius-three polynomial. Its powers are f32 but
/// polynomial coefficients and combinations are f64, followed by f32 weights.
fn weight(distance: f32) -> f32 {
    let r = distance.abs();
    if r >= 3.0 {
        return 0.0;
    }
    let r2 = r * r;
    let r3 = r * r2;
    let r4 = r * r3;
    let r5 = r * r4;
    let r = f64::from(r);
    let r2 = f64::from(r2);
    let r3 = f64::from(r3);
    let r4 = f64::from(r4);
    let r5 = f64::from(r5);
    if r < 1.0 {
        (1.0 - (25.0 / 12.0) * r2 + (5.0 / 12.0) * r3 + (13.0 / 12.0) * r4 - (5.0 / 12.0) * r5)
            as f32
    } else if r < 2.0 {
        (1.0 + (5.0 / 12.0) * r - (35.0 / 8.0) * r2 + (35.0 / 8.0) * r3 - (13.0 / 8.0) * r4
            + (5.0 / 24.0) * r5) as f32
    } else {
        (3.0 - (29.0 / 4.0) * r + (155.0 / 24.0) * r2 - (65.0 / 24.0) * r3 + (13.0 / 24.0) * r4
            - (1.0 / 24.0) * r5) as f32
    }
}

pub(super) fn sample(input: &[f32], position: f32) -> Result<f32> {
    let center = position as i64;
    if !position.is_finite() || center < 3 || center + 4 >= input.len() as i64 {
        return Err(VoiceError::new("PCM tail warp lacks interpolation context"));
    }
    let fraction = center as f32 - position;
    let mut output = 0.0_f32;
    for offset in -3..=4 {
        output += input[(center + i64::from(offset)) as usize] * weight(fraction + offset as f32);
    }
    Ok(output)
}
