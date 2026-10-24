use crate::synthesis::{
    inference::Sequence,
    stages::f0::features::FeatureNetwork,
    voice::{PreparedTree, Result, VoiceError},
};

use super::{
    config::{TimbreArchitecture, TimbreLayout},
    records::AcousticRecords,
    refinement::Refinement,
    sampling::{AcousticSampler, SamplerChildren},
    styles::TimbreStyles,
};

pub struct TimbreNetwork<'a> {
    pub(super) tree: &'a PreparedTree,
    pub layout: TimbreLayout,
    pub(super) sampler: AcousticSampler,
    pub(super) refinement: Refinement,
    pub(super) feature_network: FeatureNetwork<'a>,
    pub styles: TimbreStyles,
    pub(super) speaker: Option<usize>,
    pub(super) retained_noise_only: bool,
}

pub struct TimbreConditions<'a> {

    pub frame_features: &'a Sequence,

    pub normalized_pitch: &'a [f32],

    pub phone_impulse: &'a [f32],
    pub pitch_hz: &'a [f32],

    pub controls: &'a Sequence,
    pub ppg_noise: &'a Sequence,
    pub timbre_noise: &'a Sequence,
    pub ppg_shape: &'a [f32],
    pub ppg_intensity: &'a [f32],
    pub timbre_intensity: &'a [f32],

    pub primary_style: Option<&'a Sequence>,
    pub secondary_style: Option<&'a Sequence>,
}

pub struct TimbreOutput {

    pub first_frame: usize,

    pub acoustics: Sequence,

    pub controls: Option<Sequence>,
}

impl<'a> TimbreNetwork<'a> {
    pub fn prepare(tree: &'a PreparedTree, speaker: Option<usize>, retained_noise_only: bool) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("timbre.prepare");
        let layout = TimbreLayout::prepare(tree, speaker)?;
        let sampler = AcousticSampler::prepare(tree, layout.sampler)?;
        let refinement = Refinement::prepare(tree, layout.refinement)?;
        let styles = TimbreStyles::prepare(
            tree,
            crate::synthesis::voice::speakers::style(tree, layout.styles, speaker)?,
        )?;
        let feature_network = FeatureNetwork::prepare(tree, layout.feature_network)?;
        if sampler.config.ppg_noise != layout.ppg_noise_channels
            || sampler.config.timbre_noise != layout.timbre_noise_channels
        {
            return Err(VoiceError::new(
                "Timbre sampler and session noise widths disagree",
            ));
        }
        if !matches!(
            (&layout.architecture, &sampler.children, &refinement),
            (
                TimbreArchitecture::Sixteen,
                SamplerChildren::Sixteen(_),
                Refinement::Sixteen(_)
            ) | (
                TimbreArchitecture::Nineteen { .. },
                SamplerChildren::Nineteen(_),
                Refinement::Nineteen(_)
            )
        ) {
            return Err(VoiceError::new(
                "Timbre root, sampler and refinement architectures disagree",
            ));
        }
        if refinement.input_channels()
            != layout.ppg_noise_channels + 2 * styles.neutral.len() + refinement.pitch_channels()
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
            speaker,
            retained_noise_only,
        })
    }

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

    pub fn execute_full(&self, input: TimbreConditions<'_>) -> Result<TimbreOutput> {
        let frames = input.frame_features.frames;
        super::stream::TimbreStream::from_conditions(self, input)?.collect_until(frames)
    }
}
