//! Shared complex arithmetic for excitation chirps and spectral filtering.

pub(super) type Complex = [f32; 2];

pub(super) fn multiply([ar, ai]: Complex, [br, bi]: Complex) -> Complex {
    [ar * br - ai * bi, ar * bi + ai * br]
}

/// Unnormalized radix-2 FFT on a power-of-two complex buffer.
///
/// `roots[j] = exp(-i * TAU * j / count)` for `0 <= j < count/2`.
/// The forward transform uses a negative exponent; inverse uses a positive
/// exponent. Callers apply their own `1/count` normalization after inversion.
pub(super) fn fft(values: &mut [Complex], roots: &[Complex], inverse: bool) {
    let count = values.len();
    debug_assert!(count.is_power_of_two());
    debug_assert!(roots.len() >= count / 2);
    if count <= 1 {
        return;
    }
    let bits = count.trailing_zeros();
    for index in 0..count {
        let reversed = index.reverse_bits() >> (usize::BITS - bits);
        if index < reversed {
            values.swap(index, reversed);
        }
    }
    let mut width = 2;
    loop {
        let half = width / 2;
        let stride = count / width;
        for chunk in values.chunks_exact_mut(width) {
            let (left, right) = chunk.split_at_mut(half);
            for (index, (a, b)) in left.iter_mut().zip(right).enumerate() {
                let [real, imag] = roots[index * stride];
                let root = [real, if inverse { -imag } else { imag }];
                let product = multiply(*b, root);
                let previous = *a;
                *a = [previous[0] + product[0], previous[1] + product[1]];
                *b = [previous[0] - product[0], previous[1] - product[1]];
            }
        }
        if width == count {
            break;
        }
        width *= 2;
    }
}
