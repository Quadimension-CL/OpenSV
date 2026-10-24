use super::{InferenceError, UnaryOperator};

#[inline]
fn reciprocal(value: f32) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {

        unsafe {
            std::arch::x86_64::_mm_cvtss_f32(std::arch::x86_64::_mm_rcp_ss(
                std::arch::x86_64::_mm_set_ss(value),
            ))
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        1.0 / value
    }
}

#[inline]
fn polynomial(value: f32, tanh: bool) -> f32 {
    let [a, b, c] = if tanh {
        [0x3f7b_fbbd, 0x3f19_8f1d, 0x3e06_0807]
    } else {
        [0x3efb_fc65, 0x3e19_4af5, 0x3c06_0568]
    };
    let absolute = value.abs();
    let square = value * value;
    f32::from_bits(c)
        .mul_add(square, f32::from_bits(b))
        .mul_add(absolute, f32::from_bits(a))
        .mul_add(absolute, 1.0)
}

pub(crate) fn tanh(input: &[f32], output: &mut [f32]) {
    for (&value, target) in input.iter().zip(output) {
        let p = polynomial(value, true);
        let signed = if value >= 0.0 { p } else { reciprocal(p) };
        let square = signed * signed;
        *target = (square - 1.0) / (square + 1.0);
    }
}

pub(crate) fn sigmoid(input: &[f32], output: &mut [f32]) {
    for (&value, target) in input.iter().zip(output) {
        let p = polynomial(value, false);
        let signed = if value < 0.0 { p } else { reciprocal(p) };
        *target = reciprocal(signed * signed + 1.0);
    }
}

pub(crate) fn execute(
    operator: UnaryOperator,
    input: &[f32],
    output: &mut [f32],
) -> Result<(), InferenceError> {
    if input.len() != output.len() {
        return Err(InferenceError::LengthMismatch {
            input: input.len(),
            output: output.len(),
        });
    }
    match operator {
        UnaryOperator::Tanh => tanh(input, output),
        UnaryOperator::Sigmoid => sigmoid(input, output),
        UnaryOperator::Silu => {
            sigmoid(input, output);
            for (&value, target) in input.iter().zip(output) {
                *target *= value;
            }
        }
        other => return other.execute(input, output),
    }
    Ok(())
}
