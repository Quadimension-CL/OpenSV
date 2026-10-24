use super::fourier::{Complex, fft, multiply};
use crate::synthesis::voice::{Result, VoiceError};

#[derive(Debug)]
pub struct ChirpSynthesizer {
    chirp: Vec<Complex>,
    input: Vec<Complex>,
    kernel: Vec<Complex>,
    roots: Vec<Complex>,
}

impl ChirpSynthesizer {
    pub fn new(sample_count: usize) -> Result<Self> {
        if sample_count == 0 {
            return Err(VoiceError::new(
                "Excitation Fourier sample count must be positive",
            ));
        }
        let fft_count = sample_count
            .checked_next_power_of_two()
            .and_then(|count| count.checked_mul(2))
            .ok_or_else(|| VoiceError::new("Excitation Fourier scratch size overflows"))?;
        let mut roots = buffer(fft_count / 2)?;
        for (index, root) in roots.iter_mut().enumerate() {
            let angle = -std::f64::consts::TAU * index as f64 / fft_count as f64;
            let (sin, cos) = angle.sin_cos();
            *root = [cos as f32, sin as f32];
        }
        Ok(Self {
            chirp: buffer(sample_count)?,
            input: buffer(fft_count)?,
            kernel: buffer(fft_count)?,
            roots,
        })
    }

    pub fn synthesize(
        &mut self,
        angle: f32,
        real: &[f32],
        imag: &[f32],
        output: &mut [f32],
    ) -> Result<()> {
        let count = self.chirp.len();
        if real.len() < count || imag.len() < count || output.len() != count {
            return Err(VoiceError::new(
                "Excitation Fourier buffers have invalid lengths",
            ));
        }
        if !angle.is_finite()
            || real[..count]
                .iter()
                .chain(&imag[..count])
                .any(|x| !x.is_finite())
        {
            return Err(VoiceError::new("Excitation Fourier input must be finite"));
        }
        self.input.fill([0.0; 2]);
        self.kernel.fill([0.0; 2]);
        let fft_count = self.input.len();
        for (index, chirp) in self.chirp.iter_mut().enumerate() {

            let phase = (index as f64 * index as f64 * (angle as f64 * -0.5)) as f32;
            if !phase.is_finite() {
                return Err(VoiceError::new("Excitation Fourier chirp angle overflows"));
            }
            let (sin, cos) = f64::from(phase).sin_cos();
            *chirp = [cos as f32, -(sin as f32)];
            self.input[index] = multiply([real[index], imag[index]], conjugate(*chirp));
            self.kernel[index] = *chirp;
            if index != 0 {
                self.kernel[fft_count - index] = *chirp;
            }
        }
        fft(&mut self.input, &self.roots, false);
        fft(&mut self.kernel, &self.roots, false);
        for (input, kernel) in self.input.iter_mut().zip(&self.kernel) {
            *input = multiply(*input, *kernel);
        }
        fft(&mut self.input, &self.roots, true);
        let scale = 1.0 / fft_count as f32;
        for (index, value) in output.iter_mut().enumerate() {
            let [real, imag] = self.input[index];
            let [chirp_real, chirp_imag] = self.chirp[index];
            *value = (real * chirp_real + imag * chirp_imag) * scale;
            if !value.is_finite() {
                return Err(VoiceError::new("Excitation Fourier output is not finite"));
            }
        }
        Ok(())
    }
}

fn buffer(count: usize) -> Result<Vec<Complex>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| VoiceError::new("Could not allocate excitation Fourier scratch"))?;
    values.resize(count, [0.0; 2]);
    Ok(values)
}

fn conjugate([real, imag]: Complex) -> Complex {
    [real, -imag]
}
