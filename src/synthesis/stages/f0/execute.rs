use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    voice::{Result, VoiceError},
};
use std::ops::Range;

use super::{
    F0Stage, PitchRequest, PitchResult, PreparedPitchFeatures,
    condition::finish_pitch,
    features::FrameFeatureInputs,
    input::PitchInput,
    postprocess::mix_controls,
    sampler::{SamplerControls, SamplerInputs},
    statistics::SyllableRange,
};

impl F0Stage<'_> {
    pub fn predict(&self, request: &PitchRequest<'_>) -> Result<PitchResult> {
        let features = self.prepare_features(request.notes, request.phones)?;
        self.predict_prepared(request, &features)
    }

    pub fn predict_prepared(
        &self,
        request: &PitchRequest<'_>,
        prepared: &PreparedPitchFeatures,
    ) -> Result<PitchResult> {
        self.predict_region(request, prepared, None)
    }

    /// Evaluate a complete-note frame interval using the full prepared context
    /// and full-timeline seed events. The interval is a work region, not an
    /// independently prepared phrase or a claim of a finite dirty radius.
    pub fn predict_prepared_frames(
        &self,
        request: &PitchRequest<'_>,
        prepared: &PreparedPitchFeatures,
        frames: Range<usize>,
    ) -> Result<PitchResult> {
        self.predict_region(request, prepared, Some(frames))
    }

    fn predict_region(
        &self,
        request: &PitchRequest<'_>,
        prepared: &PreparedPitchFeatures,
        frame_range: Option<Range<usize>>,
    ) -> Result<PitchResult> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("f0.predict_prepared");
        prepared.validate()?;
        let config = self.features.configuration();
        prepared
            .encoding
            .validate(config.note_channels, config.phone_channels)?;
        if request.notes.len() != request.modulation.len()
            || prepared.encoding.note_count != request.notes.len()
            || prepared.encoding.phone_count != request.phones.len()
        {
            return Err(VoiceError::new(
                "F0 prepared feature rows differ from the request",
            ));
        }
        let PitchInput {
            notes,
            note_features,
            normalized_notes,
            fixed_style,
            phone_frames,
            tokens,
            vowel_mask,
            ..
        } = self.prepare_input(request.notes, request.phones, &prepared.transpose)?;
        let frames = note_features.frame_positions.len();
        let frame_range = frame_range.unwrap_or(0..frames);
        if frame_range.start > frame_range.end || frame_range.end > frames {
            return Err(VoiceError::new(
                "F0 work interval is outside the prepared timeline",
            ));
        }
        if request.absolute_controls.len() != frames || request.expressiveness.len() != frames {
            return Err(VoiceError::new(
                "F0 controls do not match the note timeline",
            ));
        }
        self.style.validate_weights(frames, request.style_weights)?;
        if frame_range.is_empty() {
            return Ok(PitchResult {
                pitch: Vec::new(),
                frame_seconds: self.frontend.frame_seconds(),
                predicted_controls: super::statistics::SamplerStatistics {
                    slope: Vec::new(),
                    offset: Vec::new(),
                },
            });
        }
        let note_durations = as_u32(&note_features.frame_counts)?;
        let phone_durations = as_u32(&phone_frames)?;
        let positions = as_u32(&note_features.frame_positions)?;
        let expanded = self.features.expand_encoding(
            &FrameFeatureInputs {
                notes: &normalized_notes,
                style: Some(&fixed_style),
                encoding: &prepared.encoding,
                note_durations: &note_durations,
                note_positions: &positions,
                phone_durations: &phone_durations,
            },
            None,
        )?;
        // Keep source timing, noise and scalar smoothing on the complete owner,
        // but project only the sampler's work rows. The feature network derives
        // its read halo from the actual operators before cropping the result.
        let projected = self
            .features
            .project_range(&expanded, frame_range.clone())?;
        if projected.frames != frame_range.len() {
            return Err(VoiceError::new(
                "F0 feature network changed the frame count",
            ));
        }
        let source = projected
            .pointwise
            .as_ref()
            .ok_or_else(|| VoiceError::new("F0 feature network lacks its source head"))?;
        let mut modulation = request.modulation.to_vec();
        let mut syllables = Vec::with_capacity(notes.len());
        let mut cursor = 0;
        for (((note, modulation), &count), &transpose) in notes
            .iter()
            .zip(&mut modulation)
            .zip(&note_features.frame_counts)
            .zip(prepared.transpose.iter())
        {
            if modulation.frames != count {
                return Err(VoiceError::new("F0 note/control frame count differs"));
            }
            modulation.transpose = transpose;
            let end = cursor + count;
            if cursor < frame_range.end && end > frame_range.start {
                if cursor < frame_range.start || end > frame_range.end {
                    return Err(VoiceError::new(
                        "F0 work interval must include whole note records",
                    ));
                }
                syllables.push(SyllableRange {
                    start: cursor - frame_range.start,
                    end: end - frame_range.start,
                    baseline: note.pitch + note.pitch_offset,
                });
            }
            cursor += count;
        }
        let controls = self.session.condition(
            &modulation,
            &tokens,
            &phone_frames,
            request.absolute_controls,
            request.expressiveness,
            |input| {
                input
                    .iter()
                    .map(|&value| {
                        self.pitch_normalizer
                            .normalize_scalar(1, value, true)
                            .map_err(|e| VoiceError::new(e.to_string()))
                    })
                    .collect()
            },
        )?;
        let style = self.style.weighted_frames(frames, request.style_weights)?;
        let style_condition = super::window::matrix(&style, frames, &frame_range)?;
        let mut runtime = ModelRuntime::new(self.tree);
        let sampled = self.sampler.predict(
            &SamplerInputs {
                frames: frame_range.len(),
                frame_features: self.features.configuration().frame_channels,
                frame_condition: &projected.convolutional,
                source_condition: source,
                style_condition,
                controls: SamplerControls {
                    noise: &controls.noise[frame_range.clone()],
                    expressiveness: &controls.expressiveness[frame_range.clone()],
                    strength: &controls.strength[frame_range.clone()],
                    slope: &controls.pitch_center[frame_range.clone()],
                    offset: &controls.pitch_motion[frame_range.clone()],
                    absolute_control: &controls.absolute_control[frame_range.clone()],
                    control_weight: &controls.control_weight[frame_range.clone()],
                    positions: [
                        &controls.position_a[frame_range.clone()],
                        &controls.position_b[frame_range.clone()],
                        &controls.position_c[frame_range.clone()],
                    ],
                    style_weight: &controls.style_weight[frame_range.clone()],
                    accent: &controls.accent[frame_range.clone()],
                },
                vowel_mask: &vowel_mask[frame_range.clone()],
                syllables: &syllables,
            },
            |index, input, condition, frames| {
                if frames == 0 || !input.len().is_multiple_of(frames) {
                    return Err(VoiceError::new("Invalid F0 child input sequence"));
                }
                let input = Sequence::new(frames, input.len() / frames, input.to_vec())?;
                let output = if let Some(condition) = condition {
                    if !condition.len().is_multiple_of(frames) {
                        return Err(VoiceError::new("Invalid F0 child condition sequence"));
                    }
                    let condition =
                        Sequence::new(frames, condition.len() / frames, condition.to_vec())?;
                    runtime.execute_conditioned(index, &input, &condition, true)?
                } else {
                    runtime.execute(index, &input, true)?
                };
                if output.frames != frames {
                    return Err(VoiceError::new("F0 sampler child changed the frame count"));
                }
                Ok(output.values)
            },
            |input| {
                input
                    .iter()
                    .map(|&value| {
                        self.pitch_normalizer
                            .denormalize_scalar(1, value)
                            .map_err(|e| VoiceError::new(e.to_string()))
                    })
                    .collect()
            },
        )?;
        let absolute = sampled
            .normalized_pitch
            .iter()
            .map(|&value| {
                self.pitch_normalizer
                    .denormalize_scalar(1, value)
                    .map_err(|e| VoiceError::new(e.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut pitch = finish_pitch(&absolute, &controls.transpose[frame_range.clone()])?;
        mix_controls(&mut pitch, &request.absolute_controls[frame_range])
            .map_err(|e| VoiceError::new(e.to_string()))?;
        Ok(PitchResult {
            pitch,
            frame_seconds: self.frontend.frame_seconds(),
            predicted_controls: sampled.predicted_controls,
        })
    }
}

fn as_u32(values: &[usize]) -> Result<Vec<u32>> {
    values
        .iter()
        .map(|&value| {
            u32::try_from(value)
                .map_err(|_| VoiceError::new("F0 frame index exceeds its original u32 range"))
        })
        .collect()
}
