use super::{excitation_fourier::ChirpSynthesizer, excitation_spectrum};
use crate::synthesis::{
    execution::checkpoint,
    voice::{Result, VoiceError},
};

#[derive(Clone, Copy, Debug)]
pub struct ExcitationFrame {
    pub f0_hz: f32,
    pub shape: f32,
    pub voiced: bool,
}

#[derive(Debug)]
pub struct ExcitationBatch {

    pub samples: Vec<f32>,

    pub phase_cycles: Vec<f64>,
}

pub struct Excitation {
    hop: usize,
    frame_period: f32,
    sample_rate: f32,
    window: Vec<f32>,
    frequencies: Vec<f32>,
    real: Vec<f32>,
    imag: Vec<f32>,
    waveform: Vec<f32>,
    overlap: Vec<f32>,
    fourier: ChirpSynthesizer,
    phase: f64,
    priming: bool,
    finished: bool,
}

impl Excitation {

    pub fn new(hop: usize, frame_period: f32, gain: f32) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.excitation.prepare");
        if hop == 0
            || hop > i32::MAX as usize / 2
            || !frame_period.is_finite()
            || frame_period <= 0.0
            || !gain.is_finite()
            || gain < 0.0
        {
            return Err(VoiceError::new("Invalid vocoder excitation configuration"));
        }
        let length = hop * 2;
        let sample_rate = hop as f32 / frame_period;
        if !sample_rate.is_finite() {
            return Err(VoiceError::new(
                "Vocoder excitation sample rate is not finite",
            ));
        }
        let window = (0..length)
            .map(|index| {
                let angle = (index as f32 / length as f32) * std::f32::consts::TAU;
                (1.0 - angle.cos()) * 0.5 * gain
            })
            .collect();
        Ok(Self {
            hop,
            frame_period,
            sample_rate,
            window,
            frequencies: (1..=length)
                .map(|harmonic| (harmonic * 100) as f32)
                .collect(),
            real: vec![0.0; length + 1],
            imag: vec![0.0; length + 1],
            waveform: vec![0.0; length],
            overlap: vec![0.0; length],
            fourier: ChirpSynthesizer::new(length)?,
            phase: 0.0,
            priming: true,
            finished: false,
        })
    }

    pub fn reset(&mut self) {
        self.overlap.fill(0.0);
        self.phase = 0.0;
        self.priming = true;
        self.finished = false;
    }

    pub fn process(
        &mut self,
        frames: &[ExcitationFrame],
        final_batch: bool,
    ) -> Result<ExcitationBatch> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.excitation.process");
        if self.finished {
            return Err(VoiceError::new(
                "Vocoder excitation stream has already finished",
            ));
        }
        for frame in frames {
            if !frame.f0_hz.is_finite()
                || frame.f0_hz < 0.0
                || (frame.voiced && (frame.f0_hz == 0.0 || !frame.shape.is_finite()))
            {
                return Err(VoiceError::new("Invalid vocoder excitation frame"));
            }
        }
        let output_frames = frames
            .len()
            .checked_add(usize::from(final_batch))
            .and_then(|count| count.checked_sub(usize::from(self.priming && !frames.is_empty())))
            .ok_or_else(|| VoiceError::new("Vocoder excitation frame count overflow"))?;
        let sample_count = output_frames
            .checked_mul(self.hop)
            .ok_or_else(|| VoiceError::new("Vocoder excitation sample count overflow"))?;
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(sample_count)
            .map_err(|_| VoiceError::new("Could not allocate vocoder excitation samples"))?;
        let mut phase_cycles = Vec::new();
        phase_cycles
            .try_reserve_exact(frames.len())
            .map_err(|_| VoiceError::new("Could not allocate vocoder excitation phase history"))?;

        let mut overlap = self.overlap.clone();
        let mut phase = self.phase;
        let mut priming = self.priming;
        for frame in frames {
            checkpoint()?;

            phase += f64::from(self.frame_period * frame.f0_hz);
            if !phase.is_finite() {
                return Err(VoiceError::new("Vocoder excitation phase overflow"));
            }
            overlap.copy_within(self.hop.., 0);
            overlap[self.hop..].fill(0.0);
            if frame.voiced {
                self.synthesize_frame(*frame, phase)?;
                for ((sample, &value), &window) in
                    overlap.iter_mut().zip(&self.waveform).zip(&self.window)
                {
                    *sample += window * value;
                }
            }
            if !priming {
                samples.extend_from_slice(&overlap[..self.hop]);
            }
            phase_cycles.push(phase);
            priming = false;
        }
        if final_batch {
            samples.extend_from_slice(&overlap[self.hop..]);
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(VoiceError::new(
                "Vocoder excitation produced non-finite samples",
            ));
        }
        self.overlap = overlap;
        self.phase = phase;
        self.priming = priming;
        self.finished = final_batch;
        Ok(ExcitationBatch {
            samples,
            phase_cycles,
        })
    }

    fn synthesize_frame(&mut self, frame: ExcitationFrame, phase: f64) -> Result<()> {
        let limit = (self.sample_rate * 0.5) / frame.f0_hz;

        let count = if limit < i32::MAX as f32 {
            limit as usize
        } else {
            0
        };
        let count = count.min(self.hop * 2);
        self.waveform.fill(0.0);
        if count == 0 {
            return Ok(());
        }
        self.real.fill(0.0);
        self.imag.fill(0.0);
        excitation_spectrum::spectrum(
            frame.shape,
            100.0,
            &self.frequencies[..count],
            &mut self.real[1..=count],
            &mut self.imag[1..=count],
        )?;
        let phase_reference = -self.imag[1];
        let fraction = phase.fract() as f32;
        for harmonic in 1..=count {
            let amplitude =
                (((self.real[harmonic] / count as f32) / frame.f0_hz) * self.sample_rate) * 0.1;
            let angle = (fraction - self.frame_period * frame.f0_hz)
                * harmonic as f32
                * std::f32::consts::TAU
                + phase_reference * harmonic as f32
                + self.imag[harmonic];
            let (sine, cosine) = angle.sin_cos();
            self.real[harmonic] = cosine * amplitude;
            self.imag[harmonic] = sine * amplitude;
        }
        self.fourier.synthesize(
            (frame.f0_hz * -std::f32::consts::TAU) / self.sample_rate,
            &self.real,
            &self.imag,
            &mut self.waveform,
        )
    }
}
