//! Stateful CPU spectral filter.
//!
//! The network predicts liftered real cepstra, not FFT magnitudes. Each frame
//! becomes a complex exponential response applied to a centered Hann window.
//! Window history, the overlap buffer and partial output frames survive calls.

use std::{collections::VecDeque, f32::consts::TAU};

use super::fourier::{fft, multiply};
use crate::synthesis::{
    execution::checkpoint,
    inference::Sequence,
    voice::{Result, VoiceError},
};

#[derive(Debug)]
pub struct DynamicFilter {
    frame_samples: usize,
    window_frames: usize,
    fft_size: usize,
    parameter_channels: Option<usize>,
    window: Vec<f32>,
    roots: Vec<[f32; 2]>,
    spectrum: Vec<[f32; 2]>,
    response: Vec<[f32; 2]>,
    overlap: Vec<f32>,
    excitation: VecDeque<Vec<f32>>,
    parameters: VecDeque<Vec<f32>>,
    output_partial: Vec<f32>,
    output_position: i64,
    accepted_frames: usize,
    emitted_frames: usize,
    finished: bool,
}

impl DynamicFilter {
    pub fn new(frame_samples: usize, window_samples: usize, fft_size: usize) -> Result<Self> {
        if frame_samples == 0
            || frame_samples
                .checked_mul(2)
                .is_none_or(|hop| !window_samples.is_multiple_of(hop))
            || window_samples == 0
            || !fft_size.is_power_of_two()
            || window_samples
                .checked_add(32)
                .is_none_or(|minimum| fft_size < minimum)
            || fft_size > i32::MAX as usize
        {
            return Err(VoiceError::new("Invalid vocoder dynamic filter dimensions"));
        }
        let window_frames = window_samples / frame_samples;
        let normalization: f32 = (0..window_samples)
            .step_by(frame_samples)
            .map(|sample| (1.0 - (sample as f32 / window_samples as f32 * TAU).cos()) * 0.5)
            .sum();
        let window = (0..window_samples)
            .map(|sample| {
                ((1.0 - (sample as f32 / window_samples as f32 * TAU).cos()) * 0.5) / normalization
            })
            .collect();
        let roots = (0..fft_size / 2)
            .map(|sample| {
                let angle = -std::f64::consts::TAU * sample as f64 / fft_size as f64;
                let (sin, cos) = angle.sin_cos();
                [cos as f32, sin as f32]
            })
            .collect();
        Ok(Self {
            frame_samples,
            window_frames,
            fft_size,
            parameter_channels: None,
            window,
            roots,
            spectrum: vec![[0.0; 2]; fft_size],
            response: vec![[0.0; 2]; fft_size],
            overlap: vec![0.0; fft_size],
            excitation: (0..window_frames / 2)
                .map(|_| vec![0.0; frame_samples])
                .collect(),
            parameters: VecDeque::new(),
            output_partial: Vec::with_capacity(frame_samples),
            output_position: -(fft_size as i64 / 2),
            accepted_frames: 0,
            emitted_frames: 0,
            finished: false,
        })
    }

    /// Input parameters and excitation must have the same frame count. The
    /// final call flushes precisely the accepted sample count, never a tail.
    pub fn process(
        &mut self,
        parameters: &Sequence,
        excitation: &Sequence,
        final_batch: bool,
    ) -> Result<Sequence> {
        self.validate(parameters, excitation)?;
        let accepted_frames = self
            .accepted_frames
            .checked_add(parameters.frames)
            .ok_or_else(|| VoiceError::new("Vocoder filter frame count overflows"))?;
        self.parameter_channels = Some(parameters.channels);
        self.accepted_frames = accepted_frames;
        self.parameters.extend(
            parameters
                .values
                .chunks_exact(parameters.channels)
                .map(<[f32]>::to_vec),
        );
        self.excitation.extend(
            excitation
                .values
                .chunks_exact(self.frame_samples)
                .map(<[f32]>::to_vec),
        );
        let mut output = Vec::new();
        if final_batch {
            while self.emitted_frames < self.accepted_frames {
                while self.excitation.len() < self.window_frames {
                    self.excitation.push_back(vec![0.0; self.frame_samples]);
                }
                self.process_window(&mut output)?;
            }
        } else {
            while self.excitation.len() >= self.window_frames && !self.parameters.is_empty() {
                self.process_window(&mut output)?;
            }
        }
        self.finished = final_batch;
        Sequence::from_values(self.frame_samples, output)
    }

    fn validate(&self, parameters: &Sequence, excitation: &Sequence) -> Result<()> {
        parameters.validate()?;
        excitation.validate()?;
        if self.finished {
            return Err(VoiceError::new(
                "Vocoder filter received input after its final batch",
            ));
        }
        if parameters.frames != excitation.frames || excitation.channels != self.frame_samples {
            return Err(VoiceError::new(
                "Vocoder filter input frame dimensions differ",
            ));
        }
        if parameters.channels < 2
            || !parameters.channels.is_multiple_of(2)
            || parameters.channels > self.fft_size
            || self
                .parameter_channels
                .is_some_and(|channels| channels != parameters.channels)
        {
            return Err(VoiceError::new(
                "Vocoder cepstrum width must be stable, even and no larger than the FFT",
            ));
        }
        if parameters
            .values
            .iter()
            .chain(&excitation.values)
            .any(|sample| !sample.is_finite())
        {
            return Err(VoiceError::new(
                "Vocoder filter input contains non-finite samples",
            ));
        }
        Ok(())
    }

    fn process_window(&mut self, output: &mut Vec<f32>) -> Result<()> {
        checkpoint()?;
        self.response.fill([0.0; 2]);
        if let Some(parameters) = self.parameters.pop_front() {
            let half = parameters.len() / 2;
            for (sample, &coefficient) in parameters[..=half].iter().enumerate() {
                self.response[sample][0] = coefficient * 10.0 / (sample + 1) as f32;
            }
            for (sample, &coefficient) in parameters[half + 1..].iter().enumerate() {
                self.response[self.fft_size - half + 1 + sample][0] =
                    coefficient * 10.0 / (half - sample) as f32;
            }
        }
        fft(&mut self.response, &self.roots, false);
        for value in &mut self.response[..=self.fft_size / 2] {
            let amplitude = value[0].exp();
            let (sin, cos) = value[1].sin_cos();
            *value = [amplitude * cos, amplitude * sin];
        }
        for bin in 1..self.fft_size / 2 {
            let [real, imaginary] = self.response[bin];
            self.response[self.fft_size - bin] = [real, -imaginary];
        }
        self.spectrum.fill([0.0; 2]);
        let padding = (self.fft_size - self.window.len()) / 2;
        for (frame, excitation) in self.excitation.iter().take(self.window_frames).enumerate() {
            for (sample, &value) in excitation.iter().enumerate() {
                let index = frame * self.frame_samples + sample;
                self.spectrum[padding + index][0] = value * self.window[index];
            }
        }
        fft(&mut self.spectrum, &self.roots, false);
        for (sample, &response) in self.spectrum.iter_mut().zip(&self.response) {
            *sample = multiply(*sample, response);
        }
        fft(&mut self.spectrum, &self.roots, true);
        for (sample, (accumulated, value)) in
            self.overlap.iter_mut().zip(&self.spectrum).enumerate()
        {
            let taper = if sample < 16 {
                (sample + 1) as f32 / 16.0
            } else if sample >= self.fft_size - 16 {
                (self.fft_size - sample - 1) as f32 / 16.0
            } else {
                1.0
            };
            *accumulated += value[0] / self.fft_size as f32 * taper;
            if !accumulated.is_finite() {
                return Err(VoiceError::new(
                    "Vocoder dynamic filtering produced a non-finite sample",
                ));
            }
        }
        for (sample, &value) in self.overlap[..self.frame_samples].iter().enumerate() {
            if self.output_position + sample as i64 >= 0
                && self.emitted_frames < self.accepted_frames
            {
                self.output_partial.push(value);
                if self.output_partial.len() == self.frame_samples {
                    output.append(&mut self.output_partial);
                    self.emitted_frames += 1;
                }
            }
        }
        self.output_position += self.frame_samples as i64;
        self.overlap.copy_within(self.frame_samples.., 0);
        self.overlap[self.fft_size - self.frame_samples..].fill(0.0);
        self.excitation.pop_front();
        Ok(())
    }
}
