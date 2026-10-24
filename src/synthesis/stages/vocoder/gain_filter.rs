use std::{f32::consts::TAU, sync::Arc};

use crate::synthesis::{
    execution::checkpoint,
    inference::Sequence,
    voice::{PreparedTree, Result, TensorStorage, VoiceError},
};

pub struct GainFilter {
    factors: Arc<[f32]>,
    hop: usize,
    effective_sample_rate: f32,
    window: Vec<f32>,
    overlap: Vec<f32>,
    work: Vec<f32>,
    first: bool,
    finished: bool,
    native_amplitude_limit: bool,
}

impl GainFilter {

    pub fn prepare(
        tree: &PreparedTree,
        factors_node: usize,
        hop: usize,
        frame_period: f32,
    ) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.gain_filter.prepare");
        let TensorStorage::FloatVector { values: factors } = tree.tensor(factors_node)? else {
            return Err(VoiceError::new(
                "Vocoder harmonic factors must be a float vector",
            ));
        };
        if factors.is_empty() || factors.iter().any(|value| !value.is_finite()) {
            return Err(VoiceError::new(
                "Vocoder harmonic factors are empty or non-finite",
            ));
        }
        if hop == 0
            || hop > i32::MAX as usize / 2
            || !frame_period.is_finite()
            || frame_period <= 0.0
        {
            return Err(VoiceError::new("Invalid harmonic renderer time axis"));
        }
        let effective_sample_rate = hop as f32 / frame_period;
        if !effective_sample_rate.is_finite() {
            return Err(VoiceError::new(
                "Harmonic renderer sample rate is not finite",
            ));
        }
        let length = hop * 2;

        let window = (0..length)
            .map(|sample| (1.0 - (sample as f32 / length as f32 * TAU).cos()) * 0.5)
            .collect();
        Ok(Self {
            factors: Arc::clone(factors),
            hop,
            effective_sample_rate,
            window,
            overlap: vec![0.0; length],
            work: vec![0.0; length],
            first: true,
            finished: false,
            native_amplitude_limit: false,
        })
    }

    pub fn parameter_channels(&self) -> usize {
        self.factors.len() * 2
    }

    pub fn set_native_amplitude_limit(&mut self) {
        self.native_amplitude_limit = true;
    }

    pub fn reset(&mut self) {
        self.overlap.fill(0.0);
        self.work.fill(0.0);
        self.first = true;
        self.finished = false;
    }

    pub fn render(
        &mut self,
        frequency_hz: &[f32],
        phase_cycles: &[f32],
        parameters: &Sequence,
        flush: bool,
    ) -> Result<Sequence> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.gain_filter.render");
        if self.finished {
            return Err(VoiceError::new(
                "Harmonic renderer was already flushed; reset it first",
            ));
        }
        parameters.validate()?;
        if parameters.channels != self.parameter_channels()
            || frequency_hz.len() != parameters.frames
            || phase_cycles.len() != parameters.frames
        {
            return Err(VoiceError::new(
                "Harmonic controls and network outputs do not align",
            ));
        }
        if frequency_hz
            .iter()
            .chain(phase_cycles)
            .chain(&parameters.values)
            .any(|value| !value.is_finite())
        {
            return Err(VoiceError::new(
                "Harmonic controls contain non-finite values",
            ));
        }
        let frames = parameters.frames - usize::from(self.first && parameters.frames > 0)
            + usize::from(flush);
        let mut output = Sequence::zeros(frames, self.hop)?;
        let mut emitted = 0;
        for frame in 0..parameters.frames {
            checkpoint()?;
            self.overlap.copy_within(self.hop.., 0);
            self.overlap[self.hop..].fill(0.0);
            self.work.fill(0.0);
            let controls = parameters.frame(frame)?;
            for (band, &factor) in self.factors.iter().enumerate() {

                let log_amplitude = if self.native_amplitude_limit {
                    controls[band].min(0.0)
                } else {
                    controls[band]
                };
                let amplitude = log_amplitude.exp();
                let phase =
                    phase_cycles[frame] * factor * TAU + controls[self.factors.len() + band];
                if !amplitude.is_finite() || !phase.is_finite() {
                    return Err(VoiceError::new("Harmonic amplitude or phase overflow"));
                }
                for (sample, target) in self.work.iter_mut().enumerate() {
                    let angle =
                        ((sample as i32 - self.hop as i32) as f32 * frequency_hz[frame] * factor
                            / self.effective_sample_rate)
                            * TAU
                            + phase;
                    *target += angle.cos() * amplitude;
                }
            }
            for ((target, &value), &weight) in
                self.overlap.iter_mut().zip(&self.work).zip(&self.window)
            {
                *target += value * weight;
                if !target.is_finite() {
                    return Err(VoiceError::new(
                        "Harmonic overlap-add produced a non-finite sample",
                    ));
                }
            }
            if !self.first {
                output.values[emitted * self.hop..(emitted + 1) * self.hop]
                    .copy_from_slice(&self.overlap[..self.hop]);
                emitted += 1;
            }
            self.first = false;
        }
        if flush {
            output.values[emitted * self.hop..(emitted + 1) * self.hop]
                .copy_from_slice(&self.overlap[self.hop..]);
            self.finished = true;
        }
        Ok(output)
    }
}
