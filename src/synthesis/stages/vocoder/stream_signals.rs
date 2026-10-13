//! Persistent source/filter program, joined by acoustic frame index.

use std::collections::VecDeque;

use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    voice::{Result, VoiceError},
};

use super::{
    excitation::Excitation,
    gain_filter::GainFilter,
    interpolation::{SpectralFilterConfig, SpectralFilterStream},
    postfilter::Postfilter,
    stage::{VocoderParameters, VocoderStage},
    stream_queue::{FrameQueue, paired},
    voice_control::{Component, SpectralControl},
};

pub(crate) struct SignalStream {
    source: Excitation,
    gain: GainFilter,
    harmonic_filter: SpectralFilterStream,
    modulated_filter: SpectralFilterStream,
    noise_filter: SpectralFilterStream,
    postfilter: Postfilter,
    hop: usize,
    trailing_scalar: f32,
    spectra: [FrameQueue; 3],
    controls: [VecDeque<SpectralControl>; 3],
    normalized: FrameQueue,
    source_harmonic: FrameQueue,
    source_modulated: FrameQueue,
    source_postfilter: FrameQueue,
    harmonic: FrameQueue,
    modulated: FrameQueue,
    noise: FrameQueue,
    filtered: [FrameQueue; 3],
}

impl SignalStream {
    pub(crate) fn new(stage: &VocoderStage<'_>) -> Result<Self> {
        let hop = stage.condition.frame_samples;
        let period = stage.condition.frame_seconds;
        let make_filter = |node, component| {
            SpectralFilterStream::new(
                SpectralFilterConfig::prepare(stage.tree, node)?,
                hop,
                stage.condition.sample_rate,
                component,
            )
        };
        Ok(Self {
            source: Excitation::new(hop, period, stage.config.excitation_log_gain.exp())?,
            gain: GainFilter::prepare(stage.tree, stage.modules.coefficients, hop, period)?,
            harmonic_filter: make_filter(stage.modules.harmonic_filter, Component::Primary)?,
            modulated_filter: make_filter(stage.modules.third_filter, Component::Primary)?,
            noise_filter: make_filter(stage.modules.noise_filter, Component::Secondary)?,
            postfilter: Postfilter::prepare(
                stage.tree,
                &stage.modules,
                hop,
                stage.config.gate_postfilter_noise,
            )?,
            hop,
            trailing_scalar: stage.config.trailing_scalar,
            spectra: std::array::from_fn(|_| FrameQueue::new(512)),
            controls: std::array::from_fn(|_| VecDeque::new()),
            normalized: FrameQueue::new(72),
            source_harmonic: FrameQueue::new(hop),
            source_modulated: FrameQueue::new(hop),
            source_postfilter: FrameQueue::new(hop),
            harmonic: FrameQueue::new(hop),
            modulated: FrameQueue::new(hop),
            noise: FrameQueue::new(hop),
            filtered: std::array::from_fn(|_| FrameQueue::new(hop)),
        })
    }

    pub(crate) fn process(
        &mut self,
        runtime: &mut ModelRuntime<'_>,
        parameters: VocoderParameters,
        end: bool,
    ) -> Result<(Sequence, Sequence)> {
        if parameters.normalized.frames == 0 && !end {
            return Ok((Sequence::zeros(0, self.hop)?, Sequence::zeros(0, self.hop)?));
        }
        for queue in &mut self.controls {
            queue.extend(parameters.voice_controls.iter().copied());
        }
        self.normalized.push(parameters.normalized)?;
        for (queue, spectrum) in self.spectra.iter_mut().zip(parameters.spectra) {
            queue.push(spectrum)?;
        }
        let excited = self.source.process(&parameters.excitation, end)?;
        let fundamental = parameters
            .excitation
            .iter()
            .map(|frame| frame.f0_hz)
            .collect::<Vec<_>>();
        let phases = excited
            .phase_cycles
            .iter()
            .map(|phase| *phase as f32)
            .collect::<Vec<_>>();
        let excitation = Sequence::from_values(self.hop, excited.samples)?;
        self.source_harmonic.push(excitation.clone())?;
        self.source_modulated.push(excitation.clone())?;
        self.source_postfilter.push(excitation)?;
        self.harmonic.push(self.gain.render(
            &fundamental,
            &phases,
            &parameters.harmonic_parameters,
            end,
        )?)?;

        let (source, gain) = paired(&mut self.source_modulated, &mut self.harmonic, end)?;
        self.modulated.push(Sequence::new(
            source.frames,
            self.hop,
            source
                .values
                .iter()
                .zip(&gain.values)
                .map(|(&source, &gain)| source * (self.trailing_scalar + gain))
                .collect(),
        )?)?;

        // The harmonic filter receives raw LF; the modulated filter receives
        // the learned harmonic modulation.
        let (spectrum, source) = paired(&mut self.spectra[0], &mut self.source_harmonic, end)?;
        self.filtered[0].push({
            #[cfg(feature = "synth-profile")]
            let _profile = crate::synthesis::profiling::span("vocoder.spectral_filter.harmonic");
            self.harmonic_filter.process(
                &spectrum,
                &source,
                &take_controls(&mut self.controls[0], spectrum.frames)?,
                end,
            )?
        })?;
        let (spectrum, source) = paired(&mut self.spectra[2], &mut self.modulated, end)?;
        self.filtered[2].push({
            #[cfg(feature = "synth-profile")]
            let _profile = crate::synthesis::profiling::span("vocoder.spectral_filter.modulated");
            self.modulated_filter.process(
                &spectrum,
                &source,
                &take_controls(&mut self.controls[2], spectrum.frames)?,
                end,
            )?
        })?;

        let (condition, source) = paired(&mut self.normalized, &mut self.source_postfilter, end)?;
        self.noise
            .push(self.postfilter.process(runtime, &condition, &source, end)?)?;
        let (spectrum, noise) = paired(&mut self.spectra[1], &mut self.noise, end)?;
        self.filtered[1].push({
            #[cfg(feature = "synth-profile")]
            let _profile = crate::synthesis::profiling::span("vocoder.spectral_filter.noise");
            self.noise_filter.process(
                &spectrum,
                &noise,
                &take_controls(&mut self.controls[1], spectrum.frames)?,
                end,
            )?
        })?;
        let frames = self.filtered.iter().map(FrameQueue::frames).min().unwrap();
        if end
            && (self.filtered.iter().any(|queue| queue.frames() != frames)
                || self.controls.iter().any(|queue| !queue.is_empty()))
        {
            return Err(VoiceError::new("Vocoder final PCM branches do not align"));
        }
        let mut primary = self.filtered[0].take(frames)?;
        let third = self.filtered[2].take(frames)?;
        for (first, third) in primary.values.iter_mut().zip(third.values) {
            *first += third;
        }
        Ok((primary, self.filtered[1].take(frames)?))
    }
}

fn take_controls(
    queue: &mut VecDeque<SpectralControl>,
    frames: usize,
) -> Result<Vec<SpectralControl>> {
    if queue.len() < frames {
        return Err(VoiceError::new(
            "Vocoder spectral controls lost frame alignment",
        ));
    }
    Ok(queue.drain(..frames).collect())
}
