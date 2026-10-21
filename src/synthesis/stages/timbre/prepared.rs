use crate::synthesis::{
    stages::f0::{
        TimedPhone,
        features::{FeatureEncoding, FeatureInputs, FeatureNetwork},
        frontend::{FrontendNote, NoteFeatures, NoteFrontend},
    },
    voice::{Result, VoiceError},
};

use super::TimbreNetwork;

/// Encoded note/phone rows, before duration expansion or acoustic sampling.
pub type PreparedTimbreFeatures = FeatureEncoding;

pub(super) struct TimbreFeatureInputs<'a> {
    pub network: &'a FeatureNetwork<'a>,
    pub notes: NoteFeatures,
    pub normalized_notes: Vec<f32>,
    pub phone_ids: Vec<usize>,
    pub language_ids: Vec<usize>,
    pub phone_to_note: Vec<usize>,
    pub phone_durations: Vec<u32>,
}

impl TimbreNetwork<'_> {
    /// Prepare the dense note/phone features independently of frame rendering.
    pub fn prepare_features(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
    ) -> Result<PreparedTimbreFeatures> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("timbre.prepare_features");
        let input = self.feature_inputs(notes, phones)?;
        input.network.encode(&FeatureInputs {
            notes: &input.normalized_notes,
            style: None,
            phone_to_note: &input.phone_to_note,
            phone_ids: &input.phone_ids,
            language_ids: &input.language_ids,
        })
    }

    /// Rebuild the scalar conditions and validate the current note/phone timing.
    pub(super) fn feature_inputs(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
    ) -> Result<TimbreFeatureInputs<'_>> {
        let frontend = NoteFrontend::prepare(self.tree, self.layout.frontend)?;
        let network = &self.feature_network;
        let notes = frontend.encode(notes)?;
        let mut normalized_notes = vec![0.0; notes.values.len()];
        self.tree
            .range_normalizer(frontend.normalizer_node)?
            .normalize(&notes.values, &mut normalized_notes, true)
            .map_err(|error| VoiceError::new(error.to_string()))?;
        let mut phone_ids = Vec::with_capacity(phones.len());
        let mut language_ids = Vec::with_capacity(phones.len());
        let mut phone_to_note = Vec::with_capacity(phones.len());
        let mut phone_durations = Vec::with_capacity(phones.len());
        let mut total_phone_frames = 0_usize;
        let mut last_note = 0;
        for phone in phones {
            if phone.note_index >= notes.frame_counts.len() || phone.note_index < last_note {
                return Err(VoiceError::new(
                    "Timbre phones do not follow their note records",
                ));
            }
            last_note = phone.note_index;
            total_phone_frames = total_phone_frames
                .checked_add(phone.frames)
                .ok_or_else(|| VoiceError::new("Timbre phone length overflows"))?;
            let resolved = self.tree.resolve_phone(&phone.language, &phone.token)?;
            phone_ids.push(resolved.unified_phone_index);
            language_ids.push(resolved.language_index);
            phone_to_note.push(phone.note_index);
            phone_durations.push(
                u32::try_from(phone.frames)
                    .map_err(|_| VoiceError::new("Timbre phone exceeds frame index range"))?,
            );
        }
        if notes.frame_positions.len() != total_phone_frames {
            return Err(VoiceError::new(
                "Timbre notes, phones and F0 do not cover the same frames",
            ));
        }
        Ok(TimbreFeatureInputs {
            network,
            notes,
            normalized_notes,
            phone_ids,
            language_ids,
            phone_to_note,
            phone_durations,
        })
    }
}
