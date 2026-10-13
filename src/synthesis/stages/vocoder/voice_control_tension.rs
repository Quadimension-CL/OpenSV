//! Normalized glottal-source spectrum adjustment for vocal tension.

use crate::synthesis::voice::{Result, VoiceError};

pub(super) fn log_ratio(
    f0_hz: f32,
    old_shape: f32,
    new_shape: f32,
    nyquist_hz: f32,
    frequencies: &[f32],
) -> Result<Vec<f32>> {
    if [f0_hz, old_shape, new_shape, nyquist_hz]
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
        || frequencies
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err(VoiceError::new("Invalid tension spectrum parameters"));
    }
    let count = (nyquist_hz / f0_hz) as usize;
    if (old_shape - new_shape).abs() <= 0.01 || count < 2 {
        return Ok(vec![0.0; frequencies.len()]);
    }
    let old = Pulse::new(old_shape, f0_hz)?;
    let new = Pulse::new(new_shape, f0_hz)?;
    let old_reference = old.magnitude(f0_hz);
    let new_reference = new.magnitude(f0_hz);
    if old_reference <= 0.0 || new_reference <= 0.0 {
        return Err(VoiceError::new("Invalid tension reference magnitude"));
    }
    let mut ratios = Vec::with_capacity(count);
    ratios.push(fast_ln(1.0));
    for harmonic in 1..count {
        let multiple = (harmonic + 1) as f32;
        let frequency =
            (harmonic as f32 * (f0_hz * count as f32 - f0_hz)) / (count - 1) as f32 + f0_hz;
        let before = old.magnitude(frequency) / old_reference / multiple;
        let after = new.magnitude(frequency) / new_reference / multiple;
        let ratio = after / before;
        if !ratio.is_finite() || ratio <= 0.0 {
            return Err(VoiceError::new("Invalid tension harmonic magnitude"));
        }
        ratios.push(fast_ln(ratio));
    }
    let begin = count as f32;
    let end = f0_hz * count as f32;
    let scale = count as f32 / (end - begin);
    let result: Vec<_> = frequencies
        .iter()
        .map(|frequency| {
            if *frequency < begin {
                return ratios[0];
            }
            let position = (*frequency - begin) * scale;
            if position + 1.01 >= count as f32 {
                return ratios[count - 1];
            }
            let lower = position as usize;
            (ratios[lower + 1] - ratios[lower]) * (position - lower as f32) + ratios[lower]
        })
        .collect();
    if result.iter().any(|value| !value.is_finite()) {
        return Err(VoiceError::new("Tension spectrum is not finite"));
    }
    Ok(result)
}

struct Pulse {
    period: f32,
    te: f32,
    ta: f32,
    wp: f32,
    sin_te: f32,
    cos_te: f32,
    epsilon: f32,
    alpha: f32,
    e0: f32,
    frequency_scale: f32,
}

impl Pulse {
    fn new(shape: f32, f0_hz: f32) -> Result<Self> {
        let (ra, rk, rg) = if shape >= 2.7 {
            let relative_end = 2.0 * (-0.460_829_5 / shape + 1.0);
            let rg = 5.96 / (7.96 - relative_end) + 0.009_355_2;
            (0.323 / shape, rg * relative_end - 1.0428, rg)
        } else {
            let ra = if shape < 0.21 {
                1e-6
            } else {
                shape * 0.048 - 0.01
            };
            let rk = shape * 0.118 + 0.224;
            let rg = (rk * 0.25) / ((shape * 0.11) / (rk * 1.2 + 0.5) - ra);
            (ra, rk, rg)
        };
        let input_period = 1.0 / f0_hz;
        let period = input_period.max(0.00125);
        let frequency_scale = period / input_period;
        let tp = period * (0.5 / rg);
        let te = period * ((rk + 1.0) * (0.5 / rg));
        let ta = ra * period;
        if !period.is_finite()
            || tp <= 0.0
            || !tp.is_finite()
            || te <= tp
            || te >= period
            || !te.is_finite()
            || ta <= 0.0
            || !ta.is_finite()
        {
            return Err(VoiceError::new("Invalid tension pulse timing"));
        }
        let wp = std::f32::consts::PI / tp;
        let angle = te * wp;
        let sin_te = f64::from(angle).sin() as f32;
        let cos_te = f64::from(angle).cos() as f32;
        let delta = te - period;
        let epsilon = epsilon(ta, delta)?;
        let decay = f64::from(epsilon * delta).exp() as f32;
        let integral = delta * decay / (ta * epsilon) + (1.0 - decay) / (epsilon * epsilon * ta);
        let a = integral * sin_te;
        let b = wp * wp * a - cos_te * wp;
        let twice_a = (integral + integral) * sin_te;
        let mut alpha = (b + wp) / (te * wp - sin_te);
        for _ in 0..7 {
            let exponential = f64::from(alpha * -te).exp();
            let numerator = (exponential * f64::from(wp)
                + f64::from(b)
                + f64::from((alpha * a + sin_te) * alpha)) as f32;
            let denominator =
                (exponential * f64::from(angle) - f64::from(alpha * twice_a + sin_te)) as f32;
            alpha += numerator / denominator;
        }
        let e0 = (-1.0 / (f64::from(sin_te) * f64::from(alpha * te).exp())) as f32;
        if [period, te, ta, wp, epsilon, alpha, e0]
            .iter()
            .any(|value| !value.is_finite())
        {
            return Err(VoiceError::new("Invalid tension pulse coefficients"));
        }
        Ok(Self {
            period,
            te,
            ta,
            wp,
            sin_te,
            cos_te,
            epsilon,
            alpha,
            e0,
            frequency_scale,
        })
    }

    fn magnitude(&self, frequency: f32) -> f32 {
        let omega =
            (f64::from(frequency) * std::f64::consts::TAU / f64::from(self.frequency_scale)) as f32;
        let z = Complex(self.alpha, -omega);
        let square = z.mul(z);
        let scale = Complex(self.e0, 0.0).div(Complex(square.0 + self.wp * self.wp, square.1));
        let open_shape = Complex(
            self.alpha * self.sin_te - self.cos_te * self.wp,
            -omega * self.sin_te,
        );
        let open_exp = Complex::polar(fast_exp(self.alpha * self.te), -omega * self.te);
        let product = open_exp.mul(open_shape);
        let open = scale.mul(Complex(self.wp + product.0, product.1));
        let ta_epsilon = self.ta * self.epsilon;
        let return_coefficient = (1.0 - ta_epsilon) * self.epsilon;
        let return_scale = Complex::polar(1.0, -omega * self.te)
            .div(Complex(0.0, ta_epsilon * omega).mul(Complex(self.epsilon, omega)));
        let return_shape = if return_coefficient < 1.0 {
            Complex(0.0, -ta_epsilon * omega)
        } else {
            let angle = -omega * (self.period - self.te);
            Complex(
                (1.0 - fast_sin(angle + std::f32::consts::FRAC_PI_2)) * return_coefficient,
                -fast_sin(angle) * return_coefficient - ta_epsilon * omega,
            )
        };
        let returning = return_scale.mul(return_shape);
        let real = open.0 + returning.0;
        let imaginary = open.1 + returning.1;
        (real * real + imaginary * imaginary).sqrt()
    }
}

fn epsilon(ta: f32, delta: f32) -> Result<f32> {
    let evaluate =
        |value: f32| (1.0 - (f64::from(value * ta) + f64::from(value * delta).exp())) as f32;
    let mut a = 1.0_f32;
    let mut b = (2.0 / (f64::from(ta) + 1e-9)) as f32;
    let mut fa = evaluate(a);
    let mut fb = evaluate(b);
    if fa * fb >= 0.0 {
        return Ok(b * 0.5 + 0.5);
    }
    if fa.abs() < fb.abs() {
        std::mem::swap(&mut a, &mut b);
        std::mem::swap(&mut fa, &mut fb);
    }
    let mut c = a;
    let mut fc = fa;
    let mut d = c;
    let mut bisected = true;
    for _ in 0..128 {
        if fb.abs() <= 1e-8 || (a - b).abs() <= 1e-8 {
            return Ok(b);
        }
        let mut s = if fa != fc && fb != fc {
            a * fb * fc / ((fa - fb) * (fa - fc))
                + b * fa * fc / ((fb - fa) * (fb - fc))
                + c * fa * fb / ((fc - fa) * (fc - fb))
        } else {
            ((a - b) * fb) / (fb - fa) + b
        };
        let previous_width = if bisected {
            (b - c).abs()
        } else {
            (c - d).abs()
        };
        if s < (b + 3.0 * a) * 0.25
            || s > b
            || (s - b).abs() >= previous_width * 0.5
            || previous_width < 1e-8
        {
            s = (a + b) * 0.5;
            if s == a || s == b {
                return Ok(b);
            }
            bisected = true;
        } else {
            bisected = false;
        }
        let fs = evaluate(s);
        d = c;
        c = b;
        fc = fb;
        if fa * fs < 0.0 {
            b = s;
            fb = fs;
        } else {
            a = s;
            fa = fs;
        }
        if fa.abs() < fb.abs() {
            std::mem::swap(&mut a, &mut b);
            std::mem::swap(&mut fa, &mut fb);
        }
    }
    Err(VoiceError::new("Tension pulse solver did not converge"))
}

#[derive(Clone, Copy)]
struct Complex(f32, f32);

impl Complex {
    fn polar(magnitude: f32, phase: f32) -> Self {
        Self(
            magnitude * fast_sin(phase + std::f32::consts::FRAC_PI_2),
            magnitude * fast_sin(phase),
        )
    }

    fn mul(self, other: Self) -> Self {
        Self(
            self.0 * other.0 - self.1 * other.1,
            self.0 * other.1 + self.1 * other.0,
        )
    }

    fn div(self, other: Self) -> Self {
        let norm = other.0 * other.0 + other.1 * other.1;
        Self(
            (self.0 * other.0 + self.1 * other.1) / norm,
            (self.1 * other.0 - self.0 * other.1) / norm,
        )
    }
}

fn fast_ln(value: f32) -> f32 {
    let mantissa = f32::from_bits(value.to_bits() & 0x007f_ffff | 0x3f00_0000);
    let linear = mantissa * -1.498_030_3 + value.to_bits() as f32 * 1.192_092_9e-7 - 124.225_52;
    (-1.72588 / (mantissa + 0.352_088_72) + linear) * std::f32::consts::LN_2
}

fn fast_exp(value: f32) -> f32 {
    let exponent = value * std::f32::consts::LOG2_E;
    let negative = exponent < 0.0;
    let exponent = exponent.max(-126.0);
    let fraction = exponent - exponent.trunc() + if negative { 1.0 } else { 0.0 };
    f32::from_bits(
        ((fraction * -1.490_129_1 + exponent + 121.274_055 + 27.728_024 / (4.842_525_5 - fraction))
            * 8_388_608.0) as u32,
    )
}

fn fast_sin(angle: f32) -> f32 {
    let half = if angle < 0.0 { -0.5 } else { 0.5 };
    let reduced = (angle * std::f32::consts::FRAC_1_PI * 0.5).trunc();
    let reduced = (reduced + half) * std::f32::consts::TAU - angle;
    let parabola = reduced * -0.405_284_73 * reduced.abs() + reduced * 1.273_239_5;
    let square = parabola * parabola;
    let sign = reduced.to_bits() & 0x8000_0000;
    ((f32::from_bits(sign ^ 0xbb53_3217) * square + f32::from_bits(sign | 0x3c77_ce9a)) * square
        + f32::from_bits(sign | 0x3e50_86d7))
        * square
        + parabola * 0.784_444_87
}
