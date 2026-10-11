//! Prepared per-note/phone context and transposition shared by F0 requests.

use super::{
    F0Stage, TimedPhone,
    features::{FeatureEncoding, FeatureInputs},
    frontend::FrontendNote,
};
use crate::synthesis::voice::{Result, VoiceError};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct PreparedPitchFeatures {
    pub encoding: FeatureEncoding,
    pub transpose: Arc<[f32]>,
}

impl PreparedPitchFeatures {
    pub fn reuse_rows_from(
        &self,
        previous: &Self,
        notes: &[Option<usize>],
        phones: &[Option<usize>],
    ) -> Result<Self> {
        self.validate()?;
        previous.validate()?;
        let encoding = self
            .encoding
            .reuse_rows_from(&previous.encoding, notes, phones)?;
        let mut transpose = self.transpose.to_vec();
        for (value, &index) in transpose.iter_mut().zip(notes) {
            if let Some(index) = index {
                *value = previous.transpose[index];
            }
        }
        Ok(Self {
            encoding,
            transpose: transpose.into(),
        })
    }

    pub(super) fn validate(&self) -> Result<()> {
        if self.transpose.len() != self.encoding.note_count
            || self.transpose.iter().any(|v| !v.is_finite())
        {
            return Err(VoiceError::new(
                "F0 saved transpositions differ from encoded note rows",
            ));
        }
        Ok(())
    }
}

impl F0Stage<'_> {
    pub fn prepare_features(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
    ) -> Result<PreparedPitchFeatures> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("f0.prepare_features");
        let shift = self
            .session
            .transposition(notes.iter().map(|note| note.pitch + note.pitch_offset))?;
        let transpose: Arc<[f32]> = vec![shift; notes.len()].into();
        let input = self.prepare_input(notes, phones, &transpose)?;
        let encoding = self.features.encode(&FeatureInputs {
            notes: &input.normalized_notes,
            style: Some(&input.fixed_style),
            phone_to_note: &input.phone_to_note,
            phone_ids: &input.phone_ids,
            language_ids: &input.language_ids,
        })?;
        Ok(PreparedPitchFeatures {
            encoding,
            transpose,
        })
    }
}
