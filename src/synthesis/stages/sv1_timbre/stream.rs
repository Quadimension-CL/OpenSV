//! Incremental compatible acoustic DAG. Temporal kernels retain their state;
//! branch queues release only matching rows and flush only at the source end.

use std::sync::Arc;

use super::{
    PreparedSv1TimbreFrames, Sv1TimbreNetwork,
    decoder::Decoder,
    noise::Generator,
    queue::{Rows, align},
    sampling::SamplerInput,
};
use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    stages::timbre::{TimbreOutput, TimbreRequest},
    voice::{Result, VoiceError},
};

pub struct Sv1TimbreStream<'network, 'tree> {
    network: &'network Sv1TimbreNetwork<'tree>,
    runtime: ModelRuntime<'tree>,
    source: Arc<PreparedSv1TimbreFrames>,
    noise: Generator,
    convolution: Rows,
    pointwise: Rows,
    decoder: Decoder,
    first_frame: usize,
    consumed: usize,
    sampler_rows: usize,
    emitted: usize,
}

impl<'network, 'tree> Sv1TimbreStream<'network, 'tree> {
    pub(super) fn new(
        network: &'network Sv1TimbreNetwork<'tree>,
        request: &TimbreRequest<'_>,
        source: Arc<PreparedSv1TimbreFrames>,
    ) -> Result<Self> {
        if source.contract != network.contract
            || source.phone_frames.len() != request.phones.len()
            || source
                .phone_frames
                .iter()
                .zip(request.phones)
                .any(|(&frames, phone)| frames != phone.frames)
        {
            return Err(VoiceError::new(
                "Compatible acoustic frame cache differs from the owner",
            ));
        }
        let (phone, first_frame) = source.first_phone(request.requested_frame)?;
        let noise = Generator::new(
            &source.noise_ids[phone..],
            &source.phone_frames[phone..],
            &source.records.local_parameters[phone..],
            network.sampler.noise_channels,
            network.contract,
        )?;
        Ok(Self {
            network,
            runtime: ModelRuntime::new(network.tree),
            source,
            noise,
            convolution: Rows::new(network.sampler.context_channels),
            pointwise: Rows::new(network.sampler.feature_channels),
            decoder: Decoder::new(
                network.sampler.context_channels,
                network.sampler.noise_channels,
            ),
            first_frame,
            consumed: first_frame,
            sampler_rows: first_frame,
            emitted: first_frame,
        })
    }

    pub fn first_frame(&self) -> usize {
        self.first_frame
    }

    pub fn total_frames(&self) -> usize {
        self.source.len()
    }

    pub fn is_finished(&self) -> bool {
        self.consumed == self.total_frames()
    }

    /// Consume a bounded input batch; delayed kernels can emit zero stable rows.
    pub fn next(&mut self, max_frames: usize) -> Result<TimbreOutput> {
        crate::synthesis::execution::checkpoint()?;
        if max_frames == 0 || self.is_finished() {
            return Err(VoiceError::new(
                "Compatible acoustic stream must advance an unfinished source",
            ));
        }
        let end = self
            .consumed
            .saturating_add(max_frames)
            .min(self.total_frames());
        let flush = end == self.total_frames();
        let network = self.network;
        let context = network.context.process(
            &mut self.runtime,
            &self.source.expanded.slice_frames(self.consumed..end)?,
            flush,
        )?;
        self.convolution.push(&context.convolution)?;
        self.pointwise.push(&context.pointwise)?;
        let (context, features) = align(&mut self.convolution, &mut self.pointwise, flush)?;
        let range = self.sampler_rows..self.sampler_rows + context.frames;
        let mut noise = self.noise.next(context.frames)?;
        for (row, &scale) in noise
            .values
            .chunks_exact_mut(noise.channels)
            .zip(&self.source.records.noise_scale[range.clone()])
        {
            for value in row {
                *value *= scale;
            }
        }
        let latent = network.sampler.process(
            &mut self.runtime,
            SamplerInput {
                context: &context,
                features: &features,
                noise: &noise,
                shape: &self.source.records.shape[range.clone()],
                offset: &self.source.records.offset[range.clone()],
            },
            flush,
        )?;
        self.sampler_rows = range.end;
        let decoded =
            self.decoder
                .process(&mut self.runtime, network.decoder, &context, &latent, flush)?;
        let physical = network.frontend.inverse_output(&decoded)?;
        let output_end = self
            .emitted
            .checked_add(physical.frames)
            .filter(|&end| end <= self.total_frames())
            .ok_or_else(|| {
                VoiceError::new("Compatible acoustic stream exceeded its source timeline")
            })?;
        let mut acoustics = Sequence::zeros(physical.frames, physical.channels + 1)?;
        for ((target, source), &log_hz) in acoustics
            .values
            .chunks_exact_mut(acoustics.channels)
            .zip(physical.values.chunks_exact(physical.channels))
            .zip(&self.source.records.log_hz[self.emitted..output_end])
        {
            target[0] = source[0];
            target[1] = log_hz.exp();
            target[2..].copy_from_slice(&source[1..]);
        }
        let output = TimbreOutput {
            first_frame: self.emitted,
            acoustics,
            controls: None,
        };
        self.consumed = end;
        self.emitted = output_end;
        if flush
            && (self.emitted != self.total_frames() || self.sampler_rows != self.total_frames())
        {
            return Err(VoiceError::new(
                "Compatible acoustic final flush did not cover its source timeline",
            ));
        }
        Ok(output)
    }

    pub(super) fn collect(mut self) -> Result<TimbreOutput> {
        let first_frame = self.first_frame();
        let mut values = Vec::with_capacity((self.total_frames() - first_frame) * 72);
        while !self.is_finished() {
            values.extend(self.next(128)?.acoustics.values);
        }
        Ok(TimbreOutput {
            first_frame,
            acoustics: Sequence::from_values(72, values)?,
            controls: None,
        })
    }
}
