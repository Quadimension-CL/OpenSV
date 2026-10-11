//! Prepared SV2 acoustic topology.

use crate::synthesis::{
    inference::Sequence,
    stages::f0::features::FeatureNetwork,
    voice::{PreparedTree, Result, VoiceError},
};

use super::{
    config::TimbreLayout, records::AcousticRecords, refinement::Refinement,
    sampling::AcousticSampler, styles::TimbreStyles,
};

pub struct TimbreNetwork<'a> {
    pub(super) tree: &'a PreparedTree,
    pub layout: TimbreLayout,
    pub(super) sampler: AcousticSampler,
    pub(super) refinement: Refinement,
    pub(super) feature_network: FeatureNetwork<'a>,
    pub styles: TimbreStyles,
}

/// Prepared frame conditions have explicit rates. No dimensions or conditions
/// are inferred from an ownership tree or filled with a synthetic waveform.
pub struct TimbreConditions<'a> {
    /// 128-channel result of the feature network's temporal projection.
    pub frame_features: &'a Sequence,
    /// Frontend pitch normalizer, channel 1, after MIDI range clipping.
    pub normalized_pitch: &'a [f32],
    /// Optional phoneme-local impulse; zero when the phoneme omits it.
    pub phone_impulse: &'a [f32],
    pub pitch_hz: &'a [f32],
    /// Full-rate controls consumed by the sampler's learned downsampling head.
    pub controls: &'a Sequence,
    pub ppg_noise: &'a Sequence,
    pub timbre_noise: &'a Sequence,
    pub ppg_shape: &'a [f32],
    pub ppg_intensity: &'a [f32],
    pub timbre_shape: &'a [f32],
    /// Optional low-rate style rows. None uses the model's neutral style row.
    pub primary_style: Option<&'a Sequence>,
    pub secondary_style: Option<&'a Sequence>,
}

pub struct TimbreOutput {
    /// Absolute source frame of the first returned row, including left context.
    pub first_frame: usize,
    /// 72 channels at a 5 ms cadence, ready for the vocoder frontend.
    pub acoustics: Sequence,
    /// Learned per-frame metadata; version 1 timbre models have no such head.
    pub controls: Option<Sequence>,
}

impl<'a> TimbreNetwork<'a> {
    pub fn prepare(tree: &'a PreparedTree) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("timbre.prepare");
        let layout = TimbreLayout::prepare(tree)?;
        let sampler = AcousticSampler::prepare(tree, layout.sampler)?;
        let refinement = Refinement::prepare(tree, layout.refinement)?;
        let styles = TimbreStyles::prepare(tree, layout.styles)?;
        let feature_network = FeatureNetwork::prepare(tree, layout.feature_network)?;
        if sampler.config.ppg_noise != layout.ppg_noise_channels
            || sampler.config.timbre_noise != layout.timbre_noise_channels
        {
            return Err(VoiceError::new(
                "Timbre sampler and session noise widths disagree",
            ));
        }
        if refinement.input_channels() != layout.ppg_noise_channels + 2 * styles.neutral.len()
            || refinement.latent_channels() != sampler.config.timbre_prior
            || refinement.output_channels() != 256
        {
            return Err(VoiceError::new(
                "Timbre refinement dimensions differ from the acoustic program",
            ));
        }
        Ok(Self {
            tree,
            layout,
            sampler,
            refinement,
            feature_network,
            styles,
        })
    }

    /// Consume full-owner records using the persistent acoustic program.
    pub fn execute_records(
        &self,
        frame_features: &Sequence,
        records: &AcousticRecords,
    ) -> Result<TimbreOutput> {
        self.execute_records_until(frame_features, records, records.total_frames)
    }

    pub fn execute_records_until(
        &self,
        frame_features: &Sequence,
        records: &AcousticRecords,
        end_frame: usize,
    ) -> Result<TimbreOutput> {
        if frame_features.frames != records.total_frames {
            return Err(VoiceError::new(
                "Acoustic records and feature timeline differ",
            ));
        }
        super::stream::TimbreStream::from_records(
            self,
            frame_features.slice_frames(records.first_frame..records.total_frames)?,
            records,
        )?
        .collect_until(end_frame)
    }

    /// Whole-segment collection uses exactly the same numerical program as
    /// preview streaming, including natural final-frame holds.
    pub fn execute_full(&self, input: TimbreConditions<'_>) -> Result<TimbreOutput> {
        let frames = input.frame_features.frames;
        super::stream::TimbreStream::from_conditions(self, input)?.collect_until(frames)
    }
}
