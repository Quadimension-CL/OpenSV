//! Persistent acoustic DAG. Queues join rows by their cumulative position,
//! never by the sizes returned by a single convolution call.
use super::super::{
    TimbreNetwork, TimbreOutput,
    features::{assemble_normalized_acoustics, insert_vocoder_pitch, pitch_embedding},
};
use super::{
    conditions,
    input::Input,
    queue::{Rows, TailHold, align},
};
use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    voice::{Result, VoiceError},
};

pub(super) struct Program<'network, 'tree> {
    network: &'network TimbreNetwork<'tree>,
    runtime: ModelRuntime<'tree>,
    pub input: Input,
    pub consumed: usize,
    pub emitted: usize,
    ppg_rows: usize,
    refinement_rows: usize,
    timbre_rows: usize,
    features: Rows,
    pitch: Rows,
    impulse: Rows,
    encoded: Rows,
    down_controls: Rows,
    ppg_hidden: Rows,
    refined_low: Rows,
    refined_spectral: Rows,
    refined_auxiliary: Rows,
    decoded: Rows,
    spectral_auxiliary: Rows,
    spectral_output: Rows,
    auxiliary_output: Rows,
    controls: Rows,
    control_hold: TailHold,
    refinement_hold: TailHold,
    decoder_hold: TailHold,
}

impl<'network, 'tree> Program<'network, 'tree> {
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
            pitch: Rows::new(128),
            impulse: Rows::new(128),
            encoded: Rows::new(256),
            down_controls: Rows::new(network.sampler.config.controls),
            ppg_hidden: Rows::new(network.sampler.config.ppg_hidden),
            refined_low: Rows::new(network.refinement.latent_channels()),
            refined_spectral: Rows::new(256),
            refined_auxiliary: Rows::new(256),
            decoded: Rows::new(64),
            spectral_auxiliary: Rows::new(64),
            spectral_output: Rows::new(64),
            auxiliary_output: Rows::new(7),
            controls: Rows::new(2),
            control_hold: TailHold::default(),
            refinement_hold: TailHold::default(),
            decoder_hold: TailHold::default(),
        }
    }

    pub fn advance(&mut self, max_frames: usize) -> Result<TimbreOutput> {
        crate::synthesis::execution::checkpoint()?;
        if max_frames == 0 || self.consumed == self.input.features.frames() {
            return Err(VoiceError::new(
                "Timbre stream must advance a nonempty unfinished source",
            ));
        }
        let network = self.network;
        let layout = &network.layout;
        let sampler = &network.sampler;
        let refinement = &network.refinement;
        let start = self.consumed;
        let end = start
            .saturating_add(max_frames)
            .min(self.input.features.frames());
        let final_batch = end == self.input.features.frames();
        let target = self.input.features.frames();
        let input = &self.input;
        let runtime = &mut self.runtime;
        self.features
            .push(&input.features.slice_frames(network, start..end)?)?;
        let embedded = Sequence::from_values(
            layout.pitch_embedding_channels,
            pitch_embedding(
                &input.normalized_pitch[start..end],
                layout.pitch_embedding_channels,
            )?,
        )?;
        self.pitch
            .push(&runtime.execute(layout.pitch_projection, &embedded, final_batch)?)?;
        self.impulse.push(&runtime.execute(
            layout.pulse_projection,
            &Sequence::from_values(1, input.impulse[start..end].to_vec())?,
            final_batch,
        )?)?;
        if final_batch
            && (self.features.len() != self.pitch.len()
                || self.features.len() != self.impulse.len())
        {
            return Err(VoiceError::new(
                "Timbre feature, pitch and impulse branches ended at different rows",
            ));
        }
        let count = self
            .features
            .len()
            .min(self.pitch.len())
            .min(self.impulse.len());
        let mut sum = self.features.take(count)?;
        let pitch = self.pitch.take(count)?;
        let impulse = self.impulse.take(count)?;
        for ((value, pitch), impulse) in sum.values.iter_mut().zip(pitch.values).zip(impulse.values)
        {
            *value = (*value + pitch) + impulse;
        }
        let encoded = runtime.execute(layout.encoder, &sum, final_batch)?;
        self.encoded
            .push(&runtime.execute(layout.encoded_affine, &encoded, final_batch)?)?;
        self.down_controls.push(&runtime.execute(
            sampler.children[2],
            &input.controls.slice_frames(start..end)?,
            final_batch,
        )?)?;
        let (encoded, controls) = align(&mut self.encoded, &mut self.down_controls, final_batch)?;
        let low_range = self.ppg_rows..self.ppg_rows + encoded.frames;
        let prior = Sequence::concat_channels(&[
            &encoded,
            &identity(network, layout.default_style, encoded.frames)?,
            &input.primary_style.slice_frames(low_range.clone())?,
        ])?;
        let noise = input.ppg_noise.slice_frames(low_range.clone())?;
        let condition = conditions::ppg(
            sampler,
            &prior,
            &noise,
            &controls,
            &input.ppg_shape[low_range.clone()],
            &input.ppg_intensity[low_range.clone()],
        )?;
        self.ppg_rows = low_range.end;
        let hidden =
            runtime.execute_conditioned(sampler.children[0], &noise, &condition, final_batch)?;
        self.ppg_hidden.push(&hidden)?;
        let ppg_latent = runtime.execute(sampler.children[1], &hidden, final_batch)?;
        let output_controls = runtime.execute(sampler.children[3], &hidden, final_batch)?;
        self.controls.push(&self.control_hold.extend(
            output_controls,
            target,
            final_batch,
            true,
        )?)?;

        let refine_range = self.refinement_rows..self.refinement_rows + ppg_latent.frames;
        let refinement_input = Sequence::concat_channels(&[
            &ppg_latent,
            &identity(network, layout.default_secondary_style, ppg_latent.frames)?,
            &input.secondary_style.slice_frames(refine_range.clone())?,
        ])?;
        self.refinement_rows = refine_range.end;
        let latent = runtime.execute_conditioned(
            refinement.conditional,
            &refinement_input,
            &refinement_input,
            final_batch,
        )?;
        self.refined_low.push(&latent)?;
        let expanded = runtime.execute(refinement.upsample, &latent, final_batch)?;
        let expanded = self
            .refinement_hold
            .extend(expanded, target, final_batch, false)?;
        let refined = runtime.execute(refinement.channel_affine, &expanded, final_batch)?;
        self.refined_spectral.push(&refined)?;
        self.refined_auxiliary.push(&refined)?;

        let (prior, hidden) = align(&mut self.refined_low, &mut self.ppg_hidden, final_batch)?;
        let timbre_range = self.timbre_rows..self.timbre_rows + prior.frames;
        let noise = input.timbre_noise.slice_frames(timbre_range.clone())?;
        let (data, condition) = conditions::timbre(
            sampler,
            &prior,
            &noise,
            &hidden,
            &input.timbre_shape[timbre_range.clone()],
        )?;
        self.timbre_rows = timbre_range.end;
        let hidden =
            runtime.execute_conditioned(sampler.children[4], &data, &condition, final_batch)?;
        let latent = runtime.execute(sampler.children[5], &hidden, final_batch)?;
        let decoded = runtime.execute(layout.decoder, &latent, final_batch)?;
        let decoded = self
            .decoder_hold
            .extend(decoded, target, final_batch, false)?;
        self.decoded
            .push(&runtime.execute(layout.decoded_affine, &decoded, final_batch)?)?;
        let (refined, decoded) = align(&mut self.refined_spectral, &mut self.decoded, final_batch)?;
        let spectral = runtime.execute_conditioned(
            layout.spectral_network,
            &refined,
            &decoded,
            final_batch,
        )?;
        let spectral = runtime.execute(layout.spectral_head, &spectral, final_batch)?;
        self.spectral_auxiliary.push(&spectral)?;
        self.spectral_output.push(&spectral)?;
        let (refined, spectral) = align(
            &mut self.refined_auxiliary,
            &mut self.spectral_auxiliary,
            final_batch,
        )?;
        let auxiliary = runtime.execute_conditioned(
            layout.auxiliary_network,
            &refined,
            &spectral,
            final_batch,
        )?;
        let auxiliary = runtime.execute(layout.auxiliary_head, &auxiliary, final_batch)?;
        self.auxiliary_output.push(&auxiliary)?;
        if final_batch
            && (self.spectral_output.len() != self.auxiliary_output.len()
                || self.spectral_output.len() != self.controls.len())
        {
            return Err(VoiceError::new(
                "Timbre spectral, auxiliary and control branches ended at different rows",
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
            .map_err(|e| VoiceError::new(e.to_string()))?;
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
        if final_batch && self.emitted != target {
            return Err(VoiceError::new(
                "Timbre final flush did not cover its complete source timeline",
            ));
        }
        Ok(output)
    }
}

fn identity(network: &TimbreNetwork<'_>, node: usize, frames: usize) -> Result<Sequence> {
    let vector = network
        .tree
        .tensor(node)?
        .vector()
        .ok_or_else(|| VoiceError::new("Expected a timbre identity vector"))?;
    Sequence::new(frames, vector.len(), vector.repeat(frames))
}
