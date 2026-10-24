use crate::synthesis::{
    stages::f0::{
        TimedPhone,
        features::{FeatureEncoding, FeatureInputs, FeatureNetwork,FeaturePhones},
        frontend::{FrontendNote, NoteFeatures, NoteFrontend},
    },
    voice::{Result, VoiceError},
};

use super::TimbreNetwork;

pub type PreparedTimbreFeatures = FeatureEncoding;

pub(super) struct TimbreFeatureInputs<'a> {
    pub network: &'a FeatureNetwork<'a>,
    pub notes: NoteFeatures,
    pub normalized_notes: Vec<f32>,
    pub phone_features: FeaturePhones,
    pub phone_to_note: Vec<usize>,
    pub phone_durations: Vec<u32>,
}

impl TimbreNetwork<'_> {

    pub fn prepare_features(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
    ) -> Result<PreparedTimbreFeatures> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("timbre.prepare_features");
        let input = self.feature_inputs(notes, phones)?;
        let style = self.feature_style(notes.len())?;
        input.network.encode(&FeatureInputs {
            notes: &input.normalized_notes,
            style: style.as_deref(),
            phone_to_note: &input.phone_to_note,
            phones: &input.phone_features,
        })
    }

    pub(super) fn feature_style(&self, notes: usize) -> Result<Option<Vec<f32>>> {
        let width = self.feature_network.configuration().style_channels;
        if width == 0 {
            return Ok(None);
        }
        let vector = crate::synthesis::voice::speakers::vector(
            self.tree,
            self.layout.default_style,
            self.speaker,
        )?;
        if vector.len() != width {
            return Err(VoiceError::new(
                "Timbre note identity differs from feature style width",
            ));
        }
        Ok(Some(vector.repeat(notes)))
    }

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
        let phone_features=network.prepare_phones(phones)?;
        let mut phone_to_note = Vec::with_capacity(phones.len());
        let mut phone_durations = Vec::with_capacity(phones.len());
        let mut per_note_frames = vec![0_usize; notes.frame_counts.len()];
        let mut last_note = 0;
        for phone in phones {
            if phone.note_index >= per_note_frames.len() || phone.note_index < last_note {
                return Err(VoiceError::new(
                    "Timbre phones do not follow their note records",
                ));
            }
            last_note = phone.note_index;
            per_note_frames[phone.note_index] = per_note_frames[phone.note_index]
                .checked_add(phone.frames)
                .ok_or_else(|| VoiceError::new("Timbre phone length overflows"))?;
            phone_to_note.push(phone.note_index);
            phone_durations.push(
                u32::try_from(phone.frames)
                    .map_err(|_| VoiceError::new("Timbre phone exceeds frame index range"))?,
            );
        }
        if notes.frame_counts.iter().sum::<usize>() != per_note_frames.iter().sum::<usize>() {
            return Err(VoiceError::new(
                "Timbre notes, phones and F0 do not cover the same frames",
            ));
        }
        Ok(TimbreFeatureInputs {
            network,
            notes,
            normalized_notes,
            phone_features,
            phone_to_note,
            phone_durations,
        })
    }
}
