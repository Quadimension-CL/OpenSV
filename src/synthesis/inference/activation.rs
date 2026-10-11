//! Rational sigmoid and SiLU activation kernels.
//!
//! The scalar path uses division; the four-lane SSE path refines a reciprocal
//! estimate. Their rounding can differ, as can implementations that use FMA.

use super::InferenceError;

pub const SIGMOID_TYPE_ID: [u8; 16] = [
    0x03, 0xaa, 0x2a, 0x4f, 0x90, 0xf5, 0xbc, 0x15, 0xf1, 0x04, 0xcc, 0xca, 0x8e, 0x04, 0x6d, 0x77,
];
pub const SILU_TYPE_ID: [u8; 16] = [
    0x2a, 0x79, 0xa4, 0xa9, 0x12, 0xc7, 0x78, 0x99, 0xed, 0x8d, 0x45, 0xdd, 0xc5, 0x59, 0xe1, 0xf8,
];

// Keep the rounded float32 coefficients exact; substituting integer or f64
// coefficients changes intermediate rounding.
const NUMERATOR: [f32; 11] = [
    f32::from_bits(0x4304_0000),
    f32::from_bits(0x4606_1000),
    f32::from_bits(0x48af_f500),
    f32::from_bits(0x4b24_f5b0),
    f32::from_bits(0x4d66_f190),
    f32::from_bits(0x4f75_60a9),
    f32::from_bits(0x5145_2dac),
    f32::from_bits(0x52ea_263d),
    f32::from_bits(0x5443_1fdd),
    f32::from_bits(0x554c_e175),
    f32::from_bits(0x55cc_e175),
];
const DENOMINATOR: [f32; 6] = [
    f32::from_bits(0x4384_0000),
    f32::from_bits(0x492f_f500),
    f32::from_bits(0x4de6_f190),
    f32::from_bits(0x51c5_2dac),
    f32::from_bits(0x54c3_1fdd),
    f32::from_bits(0x564c_e175),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationOperator {
    Sigmoid,
    Silu,
}

impl ActivationOperator {
    pub fn from_type_id(type_id: [u8; 16]) -> Result<Self, InferenceError> {
        match type_id {
            SIGMOID_TYPE_ID => Ok(Self::Sigmoid),
            SILU_TYPE_ID => Ok(Self::Silu),
            _ => Err(InferenceError::UnsupportedModule(type_id)),
        }
    }

    /// Scalar remainder formula, independent of the slice length.
    /// It intentionally differs from the four-lane SSE path at some roundings.
    pub fn execute_reference(
        self,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<(), InferenceError> {
        check_lengths(input, output)?;
        self.scalar_remainder(input, output);
        Ok(())
    }

    /// Non-overlapping baseline kernel: four-element SSE batches,
    /// followed by scalar division for the remaining zero to three elements.
    /// SSE is mandatory on x86_64; the caller's floating-point environment is
    /// preserved, so this assumes the normal round-to-nearest environment.
    #[cfg(target_arch = "x86_64")]
    pub fn execute_baseline(self, input: &[f32], output: &mut [f32]) -> Result<(), InferenceError> {
        check_lengths(input, output)?;
        let vector_len = input.len() / 4 * 4;
        for (source, target) in input[..vector_len]
            .as_chunks::<4>()
            .0
            .iter()
            .zip(output[..vector_len].as_chunks_mut::<4>().0.iter_mut())
        {
            // SAFETY: x86_64 has SSE; both chunks contain exactly four f32
            // elements. Unaligned loads/stores are used and the Rust borrows
            // ensure input/output do not overlap.
            unsafe { self.sse_four(source.as_ptr(), target.as_mut_ptr()) };
        }
        self.scalar_remainder(&input[vector_len..], &mut output[vector_len..]);
        Ok(())
    }

    fn scalar_remainder(self, input: &[f32], output: &mut [f32]) {
        for (&value, target) in input.iter().zip(output.iter_mut()) {
            let sigmoid = sigmoid_reference(value);
            *target = match self {
                Self::Sigmoid => sigmoid,
                Self::Silu => sigmoid * value,
            };
        }
    }

    #[cfg(target_arch = "x86_64")]
    unsafe fn sse_four(self, input: *const f32, output: *mut f32) {
        use std::arch::x86_64::{
            _mm_add_ps, _mm_loadu_ps, _mm_max_ps, _mm_min_ps, _mm_mul_ps, _mm_rcp_ps, _mm_set1_ps,
            _mm_setzero_ps, _mm_storeu_ps, _mm_sub_ps,
        };

        // SAFETY: the enclosing function's caller guarantees four readable /
        // writable elements, and every intrinsic here requires only SSE.
        unsafe {
            let x = _mm_loadu_ps(input);
            let z = _mm_mul_ps(x, x);
            let mut numerator = _mm_add_ps(x, _mm_set1_ps(NUMERATOR[0]));
            for coefficient in &NUMERATOR[1..] {
                numerator = _mm_add_ps(_mm_mul_ps(numerator, x), _mm_set1_ps(*coefficient));
            }
            let mut denominator = _mm_mul_ps(z, _mm_set1_ps(DENOMINATOR[0]));
            denominator = _mm_add_ps(denominator, _mm_set1_ps(DENOMINATOR[1]));
            for coefficient in &DENOMINATOR[2..] {
                denominator = _mm_add_ps(_mm_mul_ps(denominator, z), _mm_set1_ps(*coefficient));
            }
            // Keep these operations separate: division or FMA changes the
            // result and the behaviour on overflow/underflow.
            let reciprocal = _mm_rcp_ps(denominator);
            let estimate = _mm_mul_ps(numerator, reciprocal);
            let residual = _mm_sub_ps(numerator, _mm_mul_ps(denominator, estimate));
            let refined = _mm_add_ps(_mm_mul_ps(residual, reciprocal), estimate);
            let sigmoid = _mm_min_ps(_mm_max_ps(refined, _mm_setzero_ps()), _mm_set1_ps(1.0));
            let result = match self {
                Self::Sigmoid => sigmoid,
                Self::Silu => _mm_mul_ps(sigmoid, x),
            };
            _mm_storeu_ps(output, result);
        }
    }
}

fn check_lengths(input: &[f32], output: &[f32]) -> Result<(), InferenceError> {
    if input.len() != output.len() {
        return Err(InferenceError::LengthMismatch {
            input: input.len(),
            output: output.len(),
        });
    }
    Ok(())
}

fn sigmoid_reference(x: f32) -> f32 {
    let mut numerator = x + NUMERATOR[0];
    for coefficient in &NUMERATOR[1..] {
        numerator = numerator * x + coefficient;
    }
    let z = x * x;
    let mut denominator = z * DENOMINATOR[0] + DENOMINATOR[1];
    for coefficient in &DENOMINATOR[2..] {
        denominator = denominator * z + coefficient;
    }
    let ratio = numerator / denominator;
    // MAXSS(value, +0) then MINSS(value, +1): a NaN ratio becomes positive
    // zero. f32::clamp would preserve NaN and is therefore incorrect here.
    let nonnegative = if ratio > 0.0 { ratio } else { 0.0 };
    if nonnegative < 1.0 { nonnegative } else { 1.0 }
}
