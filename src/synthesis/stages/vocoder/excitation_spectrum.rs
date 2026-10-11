//! Shape-dependent complex excitation spectrum.
//!
//! This is the source excitation supplied to the learned vocoder, not PCM.

use crate::synthesis::voice::{Result, VoiceError};

#[path = "excitation_spectrum_data.rs"]
mod data;

const PERIOD: f32 = 0.005;

/// Evaluate the amplitude/phase envelope at the supplied frequencies.
///
/// The caller passes 100 Hz reference and a `100, 200, ...` frequency table.
/// Amplitudes are divided by the magnitude evaluated at 200 Hz; phases retain
/// their absolute values. Phase alignment and harmonic-count gain follow in
/// the streaming excitation generator.
pub fn spectrum(
    shape: f32,
    reference_hz: f32,
    frequencies: &[f32],
    amplitudes: &mut [f32],
    phases: &mut [f32],
) -> Result<()> {
    if frequencies.len() != amplitudes.len() || frequencies.len() != phases.len() {
        return Err(VoiceError::new("Excitation spectrum buffer lengths differ"));
    }
    if !shape.is_finite()
        || !reference_hz.is_finite()
        || reference_hz <= 0.0
        || frequencies.iter().any(|value| !value.is_finite())
    {
        return Err(VoiceError::new(
            "Excitation spectrum parameters are not finite",
        ));
    }
    let pulse = Pulse::new(shape, reference_hz);
    let reference = pulse.at(200.0).magnitude();
    if !reference.is_finite() || reference <= 0.0 {
        return Err(VoiceError::new("Excitation reference magnitude is invalid"));
    }
    for ((&frequency, amplitude), phase) in frequencies.iter().zip(amplitudes).zip(phases) {
        let value = pulse.at(frequency);
        *amplitude = value.magnitude() / reference;
        *phase = value.im.atan2(value.re);
        if !amplitude.is_finite() || !phase.is_finite() {
            return Err(VoiceError::new(
                "Excitation spectrum produced a non-finite value",
            ));
        }
    }
    Ok(())
}

struct Pulse {
    te: f32,
    ta: f32,
    alpha: f32,
    epsilon: f32,
    frequency_scale: f32,
    wp: f32,
    sin_te_wp: f32,
    wp_cos_te_wp: f32,
    e0: f32,
    ta_epsilon: f32,
    return_coefficient: f32,
}

impl Pulse {
    fn new(shape: f32, reference_hz: f32) -> Self {
        let coefficients = interpolate(shape);
        let te = coefficients[0] * PERIOD;
        let tp = coefficients[1] * PERIOD;
        let ta = coefficients[2] * PERIOD;
        let epsilon = coefficients[3].exp();
        let alpha = coefficients[4].exp();
        let wp = std::f32::consts::PI / tp;
        let sin_te_wp = (te * wp).sin();
        let ta_epsilon = ta * epsilon;
        Self {
            te,
            ta,
            alpha,
            epsilon,
            frequency_scale: reference_hz * PERIOD,
            wp,
            sin_te_wp,
            wp_cos_te_wp: (te * wp).cos() * wp,
            e0: -1.0 / ((te * alpha).exp() * sin_te_wp),
            ta_epsilon,
            // This expression alone is evaluated in double precision after
            // the float32 Ta * epsilon product.
            return_coefficient: ((1.0 - f64::from(ta_epsilon)) * f64::from(epsilon)) as f32,
        }
    }

    fn at(&self, frequency: f32) -> Complex {
        let omega = (frequency.max(0.1) * std::f32::consts::TAU) / self.frequency_scale;
        let z = Complex::new(self.alpha, -omega);
        let square = z.mul(z);
        let open_scale =
            Complex::new(self.e0, 0.0).div(Complex::new(square.re - self.wp * self.wp, square.im));
        let open_shape = Complex::new(
            z.re * self.sin_te_wp - self.wp_cos_te_wp,
            z.im * self.sin_te_wp,
        );
        let open_exp = Complex::polar((self.te * self.alpha).exp(), self.te * -omega);
        let open_product = open_exp.mul(open_shape);
        let open = open_scale.mul(Complex::new(self.wp + open_product.re, open_product.im));

        let return_exp = Complex::polar(1.0, self.te * -omega);
        let return_denominator =
            Complex::new(0.0, self.ta_epsilon * omega).mul(Complex::new(self.epsilon, omega));
        let return_scale = return_exp.div(return_denominator);
        let return_shape = if self.return_coefficient < 1.0 {
            Complex::new(0.0, omega * (-self.epsilon * self.ta))
        } else {
            let angle = -omega * (PERIOD - self.te);
            let (sin, cos) = angle.sin_cos();
            Complex::new(
                self.return_coefficient - cos * self.return_coefficient,
                sin * -self.return_coefficient - self.ta_epsilon * omega,
            )
        };
        let returning = return_scale.mul(return_shape);
        Complex::new(open.re + returning.re, open.im + returning.im)
    }
}

fn interpolate(shape: f32) -> [f32; 5] {
    let table = &data::SHAPE_TABLE;
    let upper = table.partition_point(|row| f32::from_bits(row[0]) <= shape);
    let index = upper.saturating_sub(1);
    let lower = table[index].map(f32::from_bits);
    if upper == 0 || upper == table.len() {
        return lower[1..].try_into().expect("five shape coefficients");
    }
    let next = table[upper].map(f32::from_bits);
    let weight = (shape - lower[0]) / (next[0] - lower[0]);
    std::array::from_fn(|index| weight * (next[index + 1] - lower[index + 1]) + lower[index + 1])
}

#[derive(Clone, Copy)]
struct Complex {
    re: f32,
    im: f32,
}

impl Complex {
    fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    fn polar(magnitude: f32, phase: f32) -> Self {
        let (sin, cos) = phase.sin_cos();
        Self::new(magnitude * cos, magnitude * sin)
    }

    fn mul(self, other: Self) -> Self {
        Self::new(
            self.re * other.re - self.im * other.im,
            self.im * other.re + self.re * other.im,
        )
    }

    fn div(self, other: Self) -> Self {
        // Complex division rescales the denominator by its
        // binary exponent, then uses float32 products/division and scalbn.
        let maximum = other.re.abs().max(other.im.abs());
        let exponent = binary_exponent(maximum);
        let scale = 2.0_f64.powi(-exponent);
        let real = (f64::from(other.re) * scale) as f32;
        let imaginary = (f64::from(other.im) * scale) as f32;
        let norm = real * real + imaginary * imaginary;
        Self::new(
            (f64::from((self.re * real + self.im * imaginary) / norm) * scale) as f32,
            (f64::from((self.im * real - self.re * imaginary) / norm) * scale) as f32,
        )
    }

    fn magnitude(self) -> f32 {
        self.re.hypot(self.im)
    }
}

fn binary_exponent(value: f32) -> i32 {
    let bits = value.to_bits() & 0x7fff_ffff;
    let exponent = (bits >> 23) as i32;
    if exponent == 0 {
        if bits == 0 {
            0
        } else {
            31 - bits.leading_zeros() as i32 - 149
        }
    } else if exponent == 255 {
        0
    } else {
        exponent - 127
    }
}
