use super::super::{
    TimbreNetwork, TimbreOutput,
    features::{assemble_normalized_acoustics, insert_vocoder_pitch, pitch_embedding},
    refinement::Refinement,
    sampling::SamplerChildren,
};
use super::{
    conditions,
    input::Input,
    queue::{Rows, align},
};
use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    voice::{Result, VoiceError},
};

pub(super) struct Program16<'network, 'tree> {
    network: &'network TimbreNetwork<'tree>,
    runtime: ModelRuntime<'tree>,
    pub input: Input,
    pub consumed: usize,
    emitted: usize,
    ppg_rows: usize,
    refinement_rows: usize,
    timbre_rows: usize,
    features: Rows,
    mean_features: Rows,
    pitch: Rows,
    impulse: Rows,
    ppg_hidden: Rows,
    pointwise: Rows,
    refined_sampler: Rows,
    refined_spectral: Rows,
    refined_auxiliary: Rows,
    timbre_latent: Rows,
    spectral_auxiliary: Rows,
    spectral_output: Rows,
    auxiliary_output: Rows,
    controls: Rows,
}

impl<'network, 'tree> Program16<'network, 'tree> {
    pub fn new(network: &'network TimbreNetwork<'tree>, input: Input) -> Self {
        Self {
            network,
            runtime: ModelRuntime::new(network.tree),
            input,
            consumed: 0,
            emitted: 0,
            ppg_rows: 0,
            refinement_rows: 0,
            timbre_rows: 0,
            features: Rows::new(128),
            mean_features: Rows::new(128),
            pitch: Rows::new(128),
            impulse: Rows::new(128),
            ppg_hidden: Rows::new(network.sampler.config.ppg_hidden),
            pointwise: Rows::new(network.sampler.config.timbre_prior),
            refined_sampler: Rows::new(256),
            refined_spectral: Rows::new(256),
            refined_auxiliary: Rows::new(256),
            timbre_latent: Rows::new(network.layout.timbre_noise_channels),
            spectral_auxiliary: Rows::new(64),
            spectral_output: Rows::new(64),
            auxiliary_output: Rows::new(7),
            controls: Rows::new(2),
        }
    }

    pub fn advance(&mut self, max_frames: usize) -> Result<TimbreOutput> {
        crate::synthesis::execution::checkpoint()?;
        let network = self.network;
        let layout = &network.layout;
        let SamplerChildren::Sixteen(children) = &network.sampler.children else {
            return Err(VoiceError::new(
                "16-child program received another sampler architecture",
            ));
        };
        let Refinement::Sixteen(refinement) = &network.refinement else {
            return Err(VoiceError::new(
                "16-child program received another refinement architecture",
            ));
        };
        let target = self.input.features.frames();
        if max_frames == 0 || self.consumed == target {
            return Err(VoiceError::new(
                "Timbre stream must advance an unfinished source",
            ));
        }
        let start = self.consumed;
        let end = start.saturating_add(max_frames).min(target);
        let flush = end == target;
        let input = &self.input;
        let runtime = &mut self.runtime;
        self.features
            .push(&input.features.slice_frames(network, start..end)?)?;
        if network.sampler.ppg_mean.is_some() {

            self.mean_features.push(&input.features.pointwise_slice_frames(network, start..end)?)?;
        }
        let embedded = Sequence::from_values(
            layout.pitch_embedding_channels,
            pitch_embedding(
                &input.normalized_pitch[start..end],
                layout.pitch_embedding_channels,
            )?,
        )?;
        self.pitch
            .push(&runtime.execute(layout.pitch_projection, &embedded, flush)?)?;
        self.impulse.push(&runtime.execute(
            layout.pulse_projection,
            &Sequence::from_values(1, input.impulse[start..end].to_vec())?,
            flush,
        )?)?;
        let (prior, mean_prior) = take_ppg_features(
            &mut self.features, &mut self.mean_features, &mut self.pitch,
            &mut self.impulse, network.sampler.ppg_mean.is_some(), flush,
        )?;
        let range = self.ppg_rows..self.ppg_rows + prior.frames;
        let noise = input.ppg_noise.slice_frames(range.clone())?;
        let controls = input.controls.slice_frames(range.clone())?;
        let condition = conditions::ppg(
            &network.sampler,
            &prior,
            &noise,
            &controls,
            &input.ppg_shape[range.clone()],
            &input.ppg_intensity[range.clone()],
        )?;
        self.ppg_rows = range.end;

        let mean = match network.sampler.ppg_mean {
            Some(node) => runtime.execute(node, mean_prior.as_ref().expect("aligned mean branch"), flush)?,
            None => noise.clone(),
        };
        let hidden = runtime.execute_conditioned(children[2], &mean, &condition, flush)?;
        self.ppg_hidden.push(&hidden)?;
        let latent = runtime.execute(children[3], &hidden, flush)?;
        self.controls
            .push(&runtime.execute(children[4], &hidden, flush)?)?;

        let range = self.refinement_rows..self.refinement_rows + latent.frames;
        let identity = crate::synthesis::voice::speakers::vector(
            network.tree,
            layout.default_secondary_style,
            network.speaker,
        )?;
        let identity = Sequence::new(
            latent.frames,
            identity.len(),
            identity.repeat(latent.frames),
        )?;
        let pitch = Sequence::from_values(
            refinement.pitch_channels,
            pitch_embedding(
                &input.refinement_pitch[range.clone()],
                refinement.pitch_channels,
            )?,
        )?;
        let refinement_input = Sequence::concat_channels(&[
            &latent,
            &pitch,
            &identity,
            &input.secondary_style.slice_frames(range.clone())?,
        ])?;
        self.refinement_rows = range.end;
        self.pointwise
            .push(&runtime.execute(refinement.pointwise, &refinement_input, flush)?)?;
        #[cfg(feature = "acoustic-diagnostic")]
        super::super::diagnostic::capture(0, input.first_frame, &refinement_input, &refinement_input)?;
        #[cfg(feature = "acoustic-diagnostic")]
        super::super::probe::capture("0x1b2ed9", &refinement_input, &refinement_input)?;
        let refined = runtime.execute_conditioned(
            refinement.conditional,
            &refinement_input,
            &refinement_input,
            flush,
        )?;
        self.refined_sampler.push(&refined)?;
        self.refined_spectral.push(&refined)?;
        self.refined_auxiliary.push(&refined)?;

        if flush
            && (self.pointwise.len() != self.refined_sampler.len()
                || self.pointwise.len() != self.ppg_hidden.len())
        {
            return Err(VoiceError::new(
                "16-child acoustic prior branches ended at different rows",
            ));
        }
        let count = self
            .pointwise
            .len()
            .min(self.refined_sampler.len())
            .min(self.ppg_hidden.len());
        let prior = self.pointwise.take(count)?;
        let refined = self.refined_sampler.take(count)?;
        let ppg = self.ppg_hidden.take(count)?;
        let range = self.timbre_rows..self.timbre_rows + count;
        let noise = input.timbre_noise.slice_frames(range.clone())?;
        self.timbre_rows = range.end;
        let mean = runtime.execute(children[1], &prior, flush)?;
        let data = Sequence::concat_channels(&[&mean, &ppg])?;
        let condition = Sequence::concat_channels(&[&refined, &noise])?;
        let hidden = runtime.execute_conditioned(children[5], &data, &condition, flush)?;
        self.timbre_latent
            .push(&runtime.execute(children[6], &hidden, flush)?)?;

        let (refined, latent) = align(&mut self.refined_spectral, &mut self.timbre_latent, flush)?;
        #[cfg(feature = "acoustic-diagnostic")]
        super::super::diagnostic::capture(1, input.first_frame, &refined, &latent)?;
        #[cfg(feature = "acoustic-diagnostic")]
        super::super::probe::capture("0x1ca741", &refined, &latent)?;
        let spectral =
            runtime.execute_conditioned(layout.spectral_network, &refined, &latent, flush)?;
        let spectral = runtime.execute(layout.spectral_head, &spectral, flush)?;
        self.spectral_auxiliary.push(&spectral)?;
        self.spectral_output.push(&spectral)?;
        let (refined, spectral) = align(
            &mut self.refined_auxiliary,
            &mut self.spectral_auxiliary,
            flush,
        )?;
        #[cfg(feature = "acoustic-diagnostic")]
        super::super::diagnostic::capture(2, input.first_frame, &refined, &spectral)?;
        #[cfg(feature = "acoustic-diagnostic")]
        super::super::probe::capture("0x1caba8", &refined, &spectral)?;
        let auxiliary =
            runtime.execute_conditioned(layout.auxiliary_network, &refined, &spectral, flush)?;
        self.auxiliary_output.push(&runtime.execute(
            layout.auxiliary_head,
            &auxiliary,
            flush,
        )?)?;
        if flush
            && (self.spectral_output.len() != self.auxiliary_output.len()
                || self.spectral_output.len() != self.controls.len())
        {
            return Err(VoiceError::new(
                "16-child acoustic output branches ended at different rows",
            ));
        }
        let count = self
            .spectral_output
            .len()
            .min(self.auxiliary_output.len())
            .min(self.controls.len());
        let spectral = self.spectral_output.take(count)?;
        let auxiliary = self.auxiliary_output.take(count)?;
        let controls = self.controls.take(count)?;
        let normalized = assemble_normalized_acoustics(&spectral.values, &auxiliary.values)?;
        let mut acoustics = vec![0.0; normalized.len()];
        network
            .tree
            .range_normalizer(layout.acoustic_normalizer)?
            .denormalize(&normalized, &mut acoustics)
            .map_err(|error| VoiceError::new(error.to_string()))?;
        let acoustics = insert_vocoder_pitch(
            &acoustics,
            &input.pitch_hz[self.emitted..self.emitted + count],
        )?;
        let output = TimbreOutput {
            first_frame: input.first_frame + self.emitted,
            acoustics: Sequence::from_values(72, acoustics)?,
            controls: Some(controls),
        };
        self.consumed = end;
        self.emitted += count;
        if flush && self.emitted != target {
            return Err(VoiceError::new(
                "16-child acoustic final flush did not cover its source timeline",
            ));
        }
        Ok(output)
    }
}

fn take_ppg_features(
    features: &mut Rows, mean_features: &mut Rows, pitch: &mut Rows, impulse: &mut Rows,
    needs_mean: bool, flush: bool,
) -> Result<(Sequence, Option<Sequence>)> {
    if flush && (features.len() != pitch.len() || features.len() != impulse.len()
        || (needs_mean && features.len() != mean_features.len())) {
        return Err(VoiceError::new("16-child acoustic input branches ended at different rows"));
    }
    let mut count = features.len().min(pitch.len()).min(impulse.len());
    if needs_mean { count = count.min(mean_features.len()); }
    let mean_prior = if needs_mean { Some(mean_features.take(count)?) } else { None };
    let mut prior = features.take(count)?;
    let pitch = pitch.take(count)?;
    let impulse = impulse.take(count)?;
    for ((value, pitch), impulse) in prior.values.iter_mut().zip(pitch.values).zip(impulse.values) {
        *value = (*value + pitch) + impulse;
    }
    Ok((prior, mean_prior))
}

#[cfg(test)]
mod pointwise_ppg_tests {
    use super::*;

    #[test]
    fn native_different_chunk_availability_keeps_unsummed_mean_aligned() {
        let (mut feature, mut mean, mut pitch, mut impulse) = (Rows::new(1), Rows::new(1), Rows::new(1), Rows::new(1));
        let (mut incoming_mean, mut incoming_conv, mut consumed) = (0, 0, 0);
        for (batch, (mean_count, conv_count)) in [(128, 7), (128, 128), (128, 128), (116, 237)].into_iter().enumerate() {
            mean.push(&Sequence::new(mean_count, 1, (incoming_mean..incoming_mean + mean_count).map(|i| 1000.0 + i as f32).collect()).unwrap()).unwrap();
            feature.push(&Sequence::new(conv_count, 1, (incoming_conv..incoming_conv + conv_count).map(|i| i as f32).collect()).unwrap()).unwrap();
            pitch.push(&Sequence::new(conv_count, 1, vec![2.0; conv_count]).unwrap()).unwrap();
            impulse.push(&Sequence::new(conv_count, 1, vec![3.0; conv_count]).unwrap()).unwrap();
            incoming_mean += mean_count;
            incoming_conv += conv_count;
            let (condition, mean_input) = take_ppg_features(&mut feature, &mut mean, &mut pitch, &mut impulse, true, batch == 3).unwrap();
            assert_eq!(condition.values, (consumed..consumed + conv_count).map(|i| i as f32 + 5.0).collect::<Vec<_>>());
            assert_eq!(mean_input.unwrap().values, (consumed..consumed + conv_count).map(|i| 1000.0 + i as f32).collect::<Vec<_>>());
            consumed += conv_count;
        }
        assert_eq!(consumed, 500);
        assert_eq!((feature.len(), mean.len(), pitch.len(), impulse.len()), (0, 0, 0, 0));
    }

    #[test]
    fn absent_mean_does_not_require_or_substitute_a_pointwise_branch() {
        let (mut feature, mut mean, mut pitch, mut impulse) = (Rows::new(1), Rows::new(1), Rows::new(1), Rows::new(1));
        feature.push(&Sequence::new(1, 1, vec![1.0]).unwrap()).unwrap();
        pitch.push(&Sequence::new(1, 1, vec![2.0]).unwrap()).unwrap();
        impulse.push(&Sequence::new(1, 1, vec![3.0]).unwrap()).unwrap();
        let (condition, mean_input) = take_ppg_features(&mut feature, &mut mean, &mut pitch, &mut impulse, false, true).unwrap();
        assert_eq!(condition.values, vec![6.0]);
        assert!(mean_input.is_none());
    }

    #[test]
    fn present_mean_cannot_finalize_with_missing_pointwise_rows() {
        let (mut feature, mut mean, mut pitch, mut impulse) = (Rows::new(1), Rows::new(1), Rows::new(1), Rows::new(1));
        for queue in [&mut feature, &mut pitch, &mut impulse] {
            queue.push(&Sequence::new(1, 1, vec![1.0]).unwrap()).unwrap();
        }
        assert!(take_ppg_features(&mut feature, &mut mean, &mut pitch, &mut impulse, true, true).is_err());
        assert_eq!((feature.len(), mean.len(), pitch.len(), impulse.len()), (1, 0, 1, 1));
    }
}
