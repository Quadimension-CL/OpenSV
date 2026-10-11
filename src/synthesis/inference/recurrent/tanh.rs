//! Elementwise tanh for the GRU candidate activation.
//!
//! Uses the platform f32 operation rather than a rational approximation.
//! Rounding is not guaranteed to be bit-identical across platform libm implementations.

pub(super) fn apply(input: &[f32], output: &mut [f32]) {
    assert_eq!(input.len(), output.len());
    for (&value, target) in input.iter().zip(output.iter_mut()) {
        *target = value.tanh();
    }
}
