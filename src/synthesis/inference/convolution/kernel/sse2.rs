#[cfg(not(target_arch = "x86_64"))]
compile_error!("The packed synthesis convolution backend requires x86-64 SSE2");

use std::arch::x86_64::{
    __m128, _mm_add_ps, _mm_loadu_ps, _mm_mul_ps, _mm_set1_ps, _mm_setzero_ps, _mm_storeu_ps,
};

#[derive(Clone, Copy)]
pub(super) struct Output {
    low: __m128,
    high: __m128,
}

impl Output {
    #[inline(always)]
    pub(super) fn zero() -> Self {

        unsafe {
            Self {
                low: _mm_setzero_ps(),
                high: _mm_setzero_ps(),
            }
        }
    }

    #[inline(always)]
    pub(super) fn load(values: &[f32]) -> Self {
        debug_assert!(values.len() <= 8);
        let mut padded = [0.0; 8];
        let source = if values.len() == 8 {
            values
        } else {
            padded[..values.len()].copy_from_slice(values);
            &padded
        };

        unsafe {
            Self {
                low: _mm_loadu_ps(source.as_ptr()),
                high: _mm_loadu_ps(source.as_ptr().add(4)),
            }
        }
    }

    #[inline(always)]
    pub(super) fn store(self, values: &mut [f32]) {
        debug_assert!(values.len() <= 8);
        let mut padded = [0.0; 8];
        let target = if values.len() == 8 {
            &mut *values
        } else {
            &mut padded
        };

        unsafe {
            _mm_storeu_ps(target.as_mut_ptr(), self.low);
            _mm_storeu_ps(target.as_mut_ptr().add(4), self.high);
        }
        if values.len() != 8 {
            values.copy_from_slice(&padded[..values.len()]);
        }
    }

    #[inline(always)]
    pub(super) fn add(&mut self, value: Self) {

        unsafe {
            self.low = _mm_add_ps(self.low, value.low);
            self.high = _mm_add_ps(self.high, value.high);
        }
    }
}

#[inline(always)]
pub(super) fn accumulate_one(weights: &[f32], source: &[f32], sum: &mut Output) {
    debug_assert_eq!(weights.len() / 8, source.len());
    for (weights, &sample) in weights.as_chunks::<8>().0.iter().zip(source) {

        unsafe {
            let low = _mm_loadu_ps(weights.as_ptr());
            let high = _mm_loadu_ps(weights.as_ptr().add(4));
            let input = _mm_set1_ps(sample);
            sum.low = _mm_add_ps(sum.low, _mm_mul_ps(low, input));
            sum.high = _mm_add_ps(sum.high, _mm_mul_ps(high, input));
        }
    }
}

#[inline(always)]
pub(super) fn accumulate_four(weights: &[f32], source: [&[f32]; 4], sums: &mut [Output; 4]) {
    debug_assert!(source.iter().all(|row| row.len() == weights.len() / 8));
    let rows = weights
        .as_chunks::<8>()
        .0
        .iter()
        .zip(source[0])
        .zip(source[1])
        .zip(source[2])
        .zip(source[3]);
    for ((((weights, &a), &b), &c), &d) in rows {

        unsafe {
            let low = _mm_loadu_ps(weights.as_ptr());
            let high = _mm_loadu_ps(weights.as_ptr().add(4));
            let a = _mm_set1_ps(a);
            sums[0].low = _mm_add_ps(sums[0].low, _mm_mul_ps(low, a));
            sums[0].high = _mm_add_ps(sums[0].high, _mm_mul_ps(high, a));
            let b = _mm_set1_ps(b);
            sums[1].low = _mm_add_ps(sums[1].low, _mm_mul_ps(low, b));
            sums[1].high = _mm_add_ps(sums[1].high, _mm_mul_ps(high, b));
            let c = _mm_set1_ps(c);
            sums[2].low = _mm_add_ps(sums[2].low, _mm_mul_ps(low, c));
            sums[2].high = _mm_add_ps(sums[2].high, _mm_mul_ps(high, c));
            let d = _mm_set1_ps(d);
            sums[3].low = _mm_add_ps(sums[3].low, _mm_mul_ps(low, d));
            sums[3].high = _mm_add_ps(sums[3].high, _mm_mul_ps(high, d));
        }
    }
}
