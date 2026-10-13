use std::sync::Arc;

use crate::synthesis::{
    inference::Sequence,
    stages::f0::{TimedPhone, features::FrameFeatureInputs, frontend::FrontendNote},
    voice::{Result, VoiceError},
};

use super::{
    PreparedTimbreFeatures, PreparedTimbreFrames, TimbreNetwork, TimbreOutput,
    records::{
        AcousticNote, AcousticPhone, FrameControl, PhoneImpulse, RecordsConfig, RecordsInput,
    },
    stream::TimbreStream,
};

pub struct TimbreRequest<'a> {
    pub notes: &'a [FrontendNote],
    pub phones: &'a [TimedPhone],
    pub midi_pitch: &'a [f32],
    pub note_modulation: &'a [AcousticNote],
    /// The duration model's paired boundary/transition predictions.
    pub phone_impulses: Option<&'a [Option<PhoneImpulse>]>,
    pub frame_controls: Option<&'a [FrameControl]>,
    /// Full-rate, full-owner vocal-mode weights in the model's style order.
    pub primary_style_weights: Option<&'a Sequence>,
    pub secondary_style_weights: Option<&'a Sequence>,
    pub requested_frame: usize,
}

impl<'tree> TimbreNetwork<'tree> {
    /// Full local acoustic path from duration-corrected phones and the F0 result.
    pub fn render(
        &self,
        request: &TimbreRequest<'_>,
        config: &RecordsConfig,
    ) -> Result<TimbreOutput> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("timbre.frontend_render");
        let features = self.prepare_features(request.notes, request.phones)?;
        self.render_prepared(request, config, &features)
    }

    /// Render current frame conditions using note/phone rows from this model.
    /// The caller must keep the cached rows aligned with the current notes/phones.
    pub fn render_prepared(
        &self,
        request: &TimbreRequest<'_>,
        config: &RecordsConfig,
        features: &PreparedTimbreFeatures,
    ) -> Result<TimbreOutput> {
        self.render_prepared_until(request, config, features, request.midi_pitch.len())
    }

    /// Keep full prepared record identity while limiting returned acoustic rows.
    /// The pipeline derives required right context from the model's operations.
    pub fn render_prepared_until(
        &self,
        request: &TimbreRequest<'_>,
        config: &RecordsConfig,
        features: &PreparedTimbreFeatures,
        end_frame: usize,
    ) -> Result<TimbreOutput> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("timbre.render_prepared");
        let frames =
            Arc::new(self.prepare_frame_features(request.notes, request.phones, features)?);
        self.start_prepared(request, config, frames)?
            .collect_until(end_frame)
    }

    /// Expand the current owner once, leaving frame projection demand-driven.
    pub fn prepare_frame_features(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
        features: &PreparedTimbreFeatures,
    ) -> Result<PreparedTimbreFrames> {
        let input = self.feature_inputs(notes, phones)?;
        let feature_config = input.network.configuration();
        if feature_config.frame_channels != 128 {
            return Err(VoiceError::new(
                "Timbre frame feature width differs from its program",
            ));
        }
        if feature_config.style_channels != 0 {
            return Err(VoiceError::new(
                "Feature note style presence or shape differs from model",
            ));
        }
        if features.note_count != notes.len()
            || features.phone_count != phones.len()
            || features.note_channels != feature_config.note_channels
            || features.phone_channels != feature_config.phone_channels
            || features.note_count.checked_mul(features.note_channels) != Some(features.notes.len())
            || features.phone_count.checked_mul(features.phone_channels)
                != Some(features.phones.len())
        {
            return Err(VoiceError::new(
                "Prepared timbre feature dimensions differ from the request or model",
            ));
        }
        let convert = |values: &[usize]| -> Result<Vec<u32>> {
            values
                .iter()
                .map(|&value| {
                    u32::try_from(value)
                        .map_err(|_| VoiceError::new("Timbre frame index exceeds u32"))
                })
                .collect()
        };
        let note_durations = convert(&input.notes.frame_counts)?;
        let positions = convert(&input.notes.frame_positions)?;
        let expanded = input.network.expand_encoding(
            &FrameFeatureInputs {
                notes: &input.normalized_notes,
                style: None,
                encoding: features,
                note_durations: &note_durations,
                note_positions: &positions,
                phone_durations: &input.phone_durations,
            },
            None,
        )?;
        Ok(PreparedTimbreFrames::new(
            expanded,
            feature_config.frame_channels,
            input.notes.frame_counts,
        ))
    }

    /// Prepare a persistent acoustic stream with current-owner frame conditions.
    pub fn start_prepared<'network>(
        &'network self,
        request: &TimbreRequest<'_>,
        config: &RecordsConfig,
        features: Arc<PreparedTimbreFrames>,
    ) -> Result<TimbreStream<'network, 'tree>> {
        if request.notes.len() != request.note_modulation.len()
            || request.notes.len() != features.note_frames.len()
            || request
                .phone_impulses
                .is_some_and(|values| values.len() != request.phones.len())
        {
            return Err(VoiceError::new(
                "Timbre modulation counts differ from notes/phones",
            ));
        }
        if request
            .note_modulation
            .iter()
            .zip(&features.note_frames)
            .any(|(note, &count)| note.frames != count)
            || features.len() != request.midi_pitch.len()
        {
            return Err(VoiceError::new(
                "Timbre notes, phones and F0 do not cover the same frames",
            ));
        }
        let acoustic_phones: Vec<_> = request
            .phones
            .iter()
            .enumerate()
            .map(|(index, phone)| AcousticPhone {
                token: &phone.token,
                frames: phone.frames,
                impulse: request.phone_impulses.and_then(|values| values[index]),
            })
            .collect();
        let records = config.prepare(RecordsInput {
            phones: &acoustic_phones,
            notes: request.note_modulation,
            midi_pitch: request.midi_pitch,
            controls: request.frame_controls,
            requested_frame: request.requested_frame,
        })?;
        TimbreStream::from_prepared(
            self,
            features,
            &records,
            request.primary_style_weights,
            request.secondary_style_weights,
        )
    }
}
