//! Vocoder postfilter controller.
//!
//! Analysis, noise and interpolated conditioning retain their own latency;
//! queues align their output before the conditioned network. Its persistent
//! model runtime returns only stable newly available subband frames.

mod condition;
mod noise;

use std::collections::VecDeque;

use crate::synthesis::{
    execution::checkpoint,
    inference::{ModelRuntime, Sequence},
    voice::{PreparedModule, PreparedTree, Result, VoiceError},
};

use super::{
    configuration::Modules,
    filterbank::{AnalysisBank, BANDS, SynthesisBank},
};
use condition::ConditionUpsampler;
use noise::PostfilterNoise;

const CONDITION_CHANNELS: usize = 72;
const INPUT_CHANNELS: usize = 2 * BANDS;
const MAX_BATCH_FRAMES: usize = 256;

pub struct Postfilter {
    analysis: AnalysisBank,
    synthesis: SynthesisBank,
    upsampler: ConditionUpsampler,
    noise: PostfilterNoise,
    network: usize,
    projection: usize,
    frame_samples: usize,
    gate_noise: bool,
    condition_queue: VecDeque<f32>,
    input_queue: VecDeque<f32>,
    noise_queue: VecDeque<f32>,
    pcm_queue: VecDeque<f32>,
    input_frames: usize,
    output_frames: usize,
    finished: bool,
}

impl Postfilter {
    /// `gate_noise` gates noise with projection logits;
    /// it does not bypass this controller.
    pub fn prepare(
        tree: &PreparedTree,
        modules: &Modules,
        frame_samples: usize,
        gate_noise: bool,
    ) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.postfilter.prepare");
        if frame_samples == 0 || !frame_samples.is_multiple_of(BANDS) {
            return Err(VoiceError::new(
                "Postfilter frame size must be divisible by eight",
            ));
        }
        let network = modules.postfilter_network;
        let projection = modules.postfilter_projection;
        match tree.node(network)?.module {
            PreparedModule::Conditional(config)
                if config.input_channels == INPUT_CHANNELS
                    && config.output_channels == 64
                    && config.condition_channels == CONDITION_CHANNELS => {}
            _ => {
                return Err(VoiceError::new(
                    "Unexpected vocoder postfilter conditional network",
                ));
            }
        }
        if !matches!(tree.node(projection)?.module, PreparedModule::Sequential) {
            return Err(VoiceError::new(
                "Postfilter projection must be a recovered sequence",
            ));
        }
        Ok(Self {
            analysis: AnalysisBank::prepare(tree, modules.analysis_filterbank)?,
            synthesis: SynthesisBank::prepare(tree, modules.synthesis_filterbank)?,
            upsampler: ConditionUpsampler::new(frame_samples / BANDS)?,
            noise: PostfilterNoise::default(),
            network,
            projection,
            frame_samples,
            gate_noise,
            condition_queue: VecDeque::new(),
            input_queue: VecDeque::new(),
            noise_queue: VecDeque::new(),
            pcm_queue: VecDeque::new(),
            input_frames: 0,
            output_frames: 0,
            finished: false,
        })
    }

    /// Reset this state together with the `ModelRuntime` used to execute it.
    pub fn reset(&mut self) {
        self.analysis.reset();
        self.synthesis.reset();
        self.upsampler.reset();
        self.noise.reset();
        self.condition_queue.clear();
        self.input_queue.clear();
        self.noise_queue.clear();
        self.pcm_queue.clear();
        self.input_frames = 0;
        self.output_frames = 0;
        self.finished = false;
    }

    /// Conditions are 72-channel acoustic frames; excitation contains
    /// `frame_samples` consecutive mono samples per acoustic frame.
    pub fn process(
        &mut self,
        runtime: &mut ModelRuntime<'_>,
        conditions: &Sequence,
        excitation: &Sequence,
        final_batch: bool,
    ) -> Result<Sequence> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.postfilter.process");
        conditions.validate()?;
        excitation.validate()?;
        if self.finished {
            return Err(VoiceError::new("Postfilter stream has already finished"));
        }
        if conditions.channels != CONDITION_CHANNELS
            || excitation.channels != self.frame_samples
            || conditions.frames != excitation.frames
            || conditions
                .values
                .iter()
                .chain(&excitation.values)
                .any(|x| !x.is_finite())
        {
            return Err(VoiceError::new("Invalid paired postfilter input frames"));
        }
        self.input_frames = self
            .input_frames
            .checked_add(conditions.frames)
            .ok_or_else(|| VoiceError::new("Postfilter frame count overflow"))?;
        if conditions.frames == 0 {
            self.batch(runtime, conditions, excitation, final_batch)?;
        } else {
            for start in (0..conditions.frames).step_by(MAX_BATCH_FRAMES) {
                let end = (start + MAX_BATCH_FRAMES).min(conditions.frames);
                self.batch(
                    runtime,
                    &conditions.slice_frames(start..end)?,
                    &excitation.slice_frames(start..end)?,
                    final_batch && end == conditions.frames,
                )?;
            }
        }
        let frames = self.pcm_queue.len() / self.frame_samples;
        let values = self
            .pcm_queue
            .drain(..frames * self.frame_samples)
            .collect();
        self.output_frames = self
            .output_frames
            .checked_add(frames)
            .ok_or_else(|| VoiceError::new("Postfilter output count overflow"))?;
        if final_batch {
            if !self.condition_queue.is_empty()
                || !self.input_queue.is_empty()
                || !self.noise_queue.is_empty()
                || !self.pcm_queue.is_empty()
                || self.output_frames != self.input_frames
            {
                return Err(VoiceError::new(
                    "Postfilter ended with unaligned output frames",
                ));
            }
            self.finished = true;
        }
        Sequence::new(frames, self.frame_samples, values)
    }

    fn batch(
        &mut self,
        runtime: &mut ModelRuntime<'_>,
        conditions: &Sequence,
        excitation: &Sequence,
        end: bool,
    ) -> Result<()> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.postfilter.batch");
        checkpoint()?;
        let subbands = self.analysis.process(&excitation.values, end)?;
        for (frame, band_frame) in subbands.as_chunks::<BANDS>().0.iter().enumerate() {
            if frame.is_multiple_of(128) {
                checkpoint()?;
            }
            let mut noise = [0.0; BANDS];
            self.noise.fill(&mut noise);
            self.input_queue.extend(band_frame);
            self.input_queue.extend(noise);
            if self.gate_noise {
                self.noise_queue.extend(noise);
            }
        }
        self.condition_queue
            .extend(self.upsampler.process(conditions, end)?.values);
        let input_frames = self.input_queue.len() / INPUT_CHANNELS;
        let condition_frames = self.condition_queue.len() / CONDITION_CHANNELS;
        if end && input_frames != condition_frames {
            return Err(VoiceError::new(
                "Postfilter input and condition tails do not align",
            ));
        }
        let frames = input_frames.min(condition_frames);
        let input = Sequence::new(
            frames,
            INPUT_CHANNELS,
            self.input_queue.drain(..frames * INPUT_CHANNELS).collect(),
        )?;
        let condition = Sequence::new(
            frames,
            CONDITION_CHANNELS,
            self.condition_queue
                .drain(..frames * CONDITION_CHANNELS)
                .collect(),
        )?;
        let hidden = {
            #[cfg(feature = "synth-profile")]
            let _profile = crate::synthesis::profiling::span("vocoder.postfilter.network");
            runtime.execute_conditioned(self.network, &input, &condition, end)?
        };
        let mut projected = {
            #[cfg(feature = "synth-profile")]
            let _profile = crate::synthesis::profiling::span("vocoder.postfilter.projection");
            runtime.execute(self.projection, &hidden, end)?
        };
        projected.validate()?;
        if projected.channels != BANDS || projected.values.iter().any(|value| !value.is_finite()) {
            return Err(VoiceError::new("Invalid postfilter projection output"));
        }
        if self.gate_noise {
            if self.noise_queue.len() < projected.values.len() {
                return Err(VoiceError::new(
                    "Postfilter noise gate has insufficient aligned input",
                ));
            }
            for block in projected.values.chunks_mut(1024) {
                checkpoint()?;
                for value in block {
                    let noise = self.noise_queue.pop_front().expect("checked aligned noise");
                    *value = noise * (1.0 / (1.0 + (-*value).exp()));
                }
            }
        }
        self.pcm_queue
            .extend(self.synthesis.process(&projected.values, end)?);
        Ok(())
    }
}
