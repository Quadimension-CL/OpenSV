use crate::synthesis::voice::{PreparedModule, Result, VoiceError};

use super::{
    F0Stage, TimedPhone,
    frontend::{FrontendNote, NoteFeatures},
};

pub(super) struct PitchInput {
    pub notes: Vec<FrontendNote>,
    pub note_features: NoteFeatures,
    pub normalized_notes: Vec<f32>,
    pub fixed_style: Vec<f32>,
    pub phone_ids: Vec<usize>,
    pub language_ids: Vec<usize>,
    pub phone_to_note: Vec<usize>,
    pub phone_frames: Vec<usize>,
    pub tokens: Vec<String>,
    pub vowel_mask: Vec<bool>,
}

impl F0Stage<'_> {
    /// Rebuild cheap scalar/timing inputs, independently of cached neural rows.
    /// Reuse each note's saved transposition.
    pub(super) fn prepare_input(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
        transpose: &[f32],
    ) -> Result<PitchInput> {
        if notes.len() != transpose.len() || transpose.iter().any(|v| !v.is_finite()) {
            return Err(VoiceError::new(
                "F0 saved transpositions differ from note records",
            ));
        }
        let mut notes = notes.to_vec();
        for (note, &transpose) in notes.iter_mut().zip(transpose) {
            note.pitch_offset += transpose;
        }
        let note_features = self.frontend.encode(&notes)?;
        let frames = note_features.frame_positions.len();
        let mut normalized_notes = vec![0.0; note_features.values.len()];
        self.note_normalizer
            .normalize(&note_features.values, &mut normalized_notes, true)
            .map_err(|e| VoiceError::new(e.to_string()))?;
        let mut fixed_style = Vec::with_capacity(notes.len() * self.fixed_style.len());
        for _ in &notes {
            fixed_style.extend_from_slice(&self.fixed_style);
        }
        let mut phone_ids = Vec::with_capacity(phones.len());
        let mut language_ids = Vec::with_capacity(phones.len());
        let mut phone_to_note = Vec::with_capacity(phones.len());
        let mut phone_frames = Vec::with_capacity(phones.len());
        let mut tokens = Vec::with_capacity(phones.len());
        let mut vowel_mask = Vec::with_capacity(frames);
        let mut last_note = 0;
        let mut total_phone_frames = 0_usize;
        for phone in phones {
            if phone.note_index >= notes.len() || phone.note_index < last_note {
                return Err(VoiceError::new(
                    "F0 phones are not ordered by their prepared note records",
                ));
            }
            last_note = phone.note_index;
            total_phone_frames = total_phone_frames
                .checked_add(phone.frames)
                .ok_or_else(|| VoiceError::new("F0 phone duration sum overflow"))?;
            if total_phone_frames > frames {
                return Err(VoiceError::new(
                    "F0 phone timeline exceeds the note timeline",
                ));
            }
            let resolved = self.tree.resolve_phone(&phone.language, &phone.token)?;
            let table = self
                .tree
                .nodes()
                .iter()
                .find_map(|node| match &node.module {
                    PreparedModule::PhoneSet(table)
                        if table.name == resolved.selected_phone_set =>
                    {
                        Some(table)
                    }
                    _ => None,
                })
                .ok_or_else(|| VoiceError::new("F0 phone classification table is missing"))?;
            let is_vowel = table
                .lookup(&phone.token)
                .is_some_and(|phone| matches!(phone.class, "vowel" | "diphthong"));
            vowel_mask.extend(std::iter::repeat_n(is_vowel, phone.frames));
            phone_ids.push(resolved.unified_phone_index);
            language_ids.push(resolved.language_index);
            phone_to_note.push(phone.note_index);
            phone_frames.push(phone.frames);
            tokens.push(phone.token.clone());
        }
        // Phone ownership conditions the feature rows; its edited boundaries
        // expand independently from the score note positions.
        if total_phone_frames != frames || vowel_mask.len() != frames {
            return Err(VoiceError::new(
                "F0 phone and note timelines have different total frame counts",
            ));
        }
        Ok(PitchInput {
            notes,
            note_features,
            normalized_notes,
            fixed_style,
            phone_ids,
            language_ids,
            phone_to_note,
            phone_frames,
            tokens,
            vowel_mask,
        })
    }
}
