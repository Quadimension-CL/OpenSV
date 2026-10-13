//! Frame-aligned source-shape and spectral-envelope controls.

use crate::synthesis::voice::{Result, VoiceError};

use super::{fourier::Complex, voice_control_tension};

/// Controls applied to the learned filter spectra, before excitation filtering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceControl {
    /// Signed LF-shape adjustment; increasing vocal tension uses a negative value.
    pub tension: f32,
    pub primary_gain: f32,
    pub secondary_gain: f32,
    pub formant_ratio: f32,
}

impl Default for VoiceControl {
    fn default() -> Self {
        Self {
            tension: 0.0,
            primary_gain: 1.0,
            secondary_gain: 1.0,
            formant_ratio: 1.0,
        }
    }
}

impl VoiceControl {
    pub(super) fn validate(self) -> Result<()> {
        if !self.tension.is_finite()
            || !self.primary_gain.is_finite()
            || self.primary_gain < 0.0
            || !self.secondary_gain.is_finite()
            || self.secondary_gain < 0.0
            || !self.formant_ratio.is_finite()
            || self.formant_ratio <= 0.0
        {
            return Err(VoiceError::new("Invalid vocoder voice control"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpectralControl {
    pub voice: VoiceControl,
    pub f0_hz: f32,
    pub shape: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Component {
    Primary,
    Secondary,
}

/// Scratch belongs to each filter so frame-window latency never changes which
/// control record is paired with a learned frequency response.
#[derive(Debug)]
pub(super) struct SpectralControlFilter {
    component: Component,
    nyquist_hz: f32,
    frequencies: Vec<f32>,
    magnitudes: Vec<f32>,
    warped: Vec<f32>,
}

impl SpectralControlFilter {
    pub fn new(component: Component, sample_rate: u32, fft_size: usize) -> Self {
        let count = fft_size / 2;
        let nyquist_hz = sample_rate as f32 * 0.5;
        Self {
            component,
            nyquist_hz,
            frequencies: (0..count)
                .map(|i| i as f32 * nyquist_hz / count as f32)
                .collect(),
            magnitudes: vec![0.0; count],
            warped: vec![0.0; count],
        }
    }

    pub fn apply(&mut self, spectrum: &mut [Complex], control: SpectralControl) -> Result<()> {
        let voice = control.voice;
        if voice == VoiceControl::default() {
            return Ok(());
        }
        let count = self.frequencies.len();
        let warp = (voice.formant_ratio - 1.0).abs() > 0.01;
        for (target, value) in self.magnitudes.iter_mut().zip(&spectrum[..count]) {
            *target = value[0].hypot(value[1]).max(f32::MIN_POSITIVE).ln();
        }
        if warp {
            // Keep the fundamental and the upper spectrum anchored while the
            // middle formants move. This changes magnitude, never FFT phase.
            let last = self.frequencies[count - 1];
            let source = [
                0.0,
                control.f0_hz,
                (4000.0 * voice.formant_ratio).clamp(100.0, 7000.0),
                8000.0,
                last,
            ];
            let destination = [0.0, control.f0_hz, 4000.0, 8000.0, last];
            for (i, &frequency) in self.frequencies.iter().enumerate() {
                let coordinate = piecewise(&source, &destination, frequency);
                self.warped[i] = uniform(
                    &self.magnitudes,
                    coordinate * count as f32 / self.nyquist_hz,
                );
            }
        } else {
            self.warped.copy_from_slice(&self.magnitudes);
        }
        match self.component {
            Component::Primary => {
                let adjusted = adjusted_shape(control.shape, voice.tension);
                if (adjusted - control.shape).abs() > 0.01 {
                    let ratio = voice_control_tension::log_ratio(
                        control.f0_hz,
                        control.shape,
                        adjusted,
                        self.nyquist_hz,
                        &self.frequencies,
                    )?;
                    for (magnitude, change) in self.warped.iter_mut().zip(ratio) {
                        *magnitude += change;
                    }
                }
                for (i, value) in spectrum.iter_mut().take(count).enumerate() {
                    let magnitude = self.warped[i].exp() * voice.primary_gain;
                    replace_magnitude(value, magnitude);
                }
                spectrum[count][0] *= voice.primary_gain;
                spectrum[count][1] *= voice.primary_gain;
            }
            Component::Secondary => {
                let ratio = (f64::from(voice.secondary_gain) + 1e-8)
                    / (f64::from(voice.primary_gain) + 1e-8);
                let ratio = ratio as f32;
                let amount = ratio.log2().abs().min(1.0);
                for (i, value) in spectrum.iter_mut().take(count).enumerate() {
                    let frequency = self.frequencies[i];
                    let ramp = if ratio >= 1.0 {
                        ((frequency - 500.0) / 1500.0).clamp(0.0, 1.0)
                    } else {
                        ((frequency - 3000.0) / 4000.0).clamp(0.0, 1.0)
                    };
                    let weight =
                        ((f64::from(ramp) * 0.3 + 0.7 - 1.0) * f64::from(amount) + 1.0) as f32;
                    let gain = if voice.secondary_gain < 1.0 {
                        f64::from((1.0 - weight) * (1.0 - voice.secondary_gain))
                            + f64::from(voice.secondary_gain)
                            + 1e-8
                    } else {
                        (f64::from(weight) * (f64::from(voice.secondary_gain) + 1e-8).ln()).exp()
                    } as f32;
                    let magnitude = self.warped[i].exp() * gain;
                    replace_magnitude(value, magnitude);
                }
            }
        }
        if spectrum[..=count]
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err(VoiceError::new(
                "Vocoder voice controls produced a non-finite response",
            ));
        }
        Ok(())
    }
}

fn adjusted_shape(shape: f32, control: f32) -> f32 {
    let base = shape.clamp(0.1, 3.0);
    let control = control.clamp(-1.0, 1.0);
    let adjusted = base
        + control
            * if control > 0.0 {
                3.0 - base
            } else {
                base - 0.1
            };
    let convert = |value: f32| (value + 0.0749).ln() * -3.1276 - 6.7906;
    let original = convert(base);
    let difference = convert(adjusted) - original;
    let difference = if difference > 0.0 {
        ((f64::from(difference) / 3.0).tanh() * 3.0) as f32
    } else {
        difference
    };
    (((difference + original + 6.7906) / -3.1276).exp() - 0.0749).clamp(0.1, 3.0)
}

fn piecewise(x: &[f32; 5], y: &[f32; 5], value: f32) -> f32 {
    if value <= x[0] {
        return y[0];
    }
    let mut lower = 0;
    while lower + 1 < x.len() && x[lower + 1] < value {
        lower += 1;
    }
    if lower + 1 == x.len() {
        return y[lower];
    }
    (value - x[lower]) * (y[lower + 1] - y[lower]) / (x[lower + 1] - x[lower]) + y[lower]
}

fn uniform(values: &[f32], position: f32) -> f32 {
    if position <= 0.0 {
        return values[0];
    }
    if position + 1.01 >= values.len() as f32 {
        return values[values.len() - 1];
    }
    let lower = position as usize;
    values[lower] + (values[lower + 1] - values[lower]) * (position - lower as f32)
}

fn replace_magnitude(value: &mut Complex, magnitude: f32) {
    let phase = value[1].atan2(value[0]);
    let (sin, cos) = phase.sin_cos();
    *value = [magnitude * cos, magnitude * sin];
}
