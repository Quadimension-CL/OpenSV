use serde::{Deserialize, Serialize};

use super::{FRAME_SECONDS, HOP_SAMPLES, SAMPLE_RATE};
use crate::synthesis::{
    stages::{
        duration::{DurationNote, DurationRecord, ExplicitPhone},
        f0::frontend::{FrontendNote, frame_count},
    },
    voice::{PreparedTree, Result, VoiceError, VoiceFamily},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScorePhone {
    pub language: String,
    pub token: String,
    #[serde(default)]
    pub onset_offset_seconds: Option<f64>,
}

/// One syllable/record. For a split note, duration_seconds remains the whole
/// source-note duration and parts/part_index select its temporal partition.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScoreNote {
    #[serde(default)]
    pub source_note_id: Option<u64>,
    pub duration_seconds: f64,
    pub pitch: f32,
    pub phones: Vec<ScorePhone>,
    #[serde(default)]
    pub kind: u32,
    #[serde(default)]
    pub flag: bool,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default = "one")]
    pub parts: u32,
    #[serde(default)]
    pub part_index: u32,
}

fn one() -> u32 {
    1
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExplicitScore {
    /// Global seconds of the first record, before output cropping.
    pub start_seconds: f64,
    pub visible_begin_seconds: f64,
    #[serde(default)]
    pub visible_end_seconds: Option<f64>,
    /// Chronological source records, including explicit sil between notes.
    /// prepare adds boundary context and redistributes consonant onsets.
    pub notes: Vec<ScoreNote>,
}

#[derive(Clone, Debug)]
pub struct PreparedPhrase {
    /// Native curve/point conditioning, sampled after the model clock is known.
    pub pitch_controls: Option<super::PreparedPitchControls>,
    /// These two arrays have identical indexing, including silence records.
    pub records: Vec<DurationRecord>,
    pub notes: Vec<FrontendNote>,
    /// Stable score identity across phoneme redistribution and syllable splits.
    /// Synthetic context and explicit rests have no source note.
    pub source_note_ids: Vec<Option<u64>>,
    pub start_seconds: f64,
    pub visible_begin_seconds: f64,
    pub visible_end_seconds: Option<f64>,
}

impl ExplicitScore {
    pub fn prepare(
        &self,
        duration_tree: &PreparedTree,
        family: VoiceFamily,
    ) -> Result<PreparedPhrase> {
        if !self.start_seconds.is_finite()
            || !self.visible_begin_seconds.is_finite()
            || self
                .visible_end_seconds
                .is_some_and(|end| !end.is_finite() || end <= self.visible_begin_seconds)
        {
            return Err(VoiceError::new("Invalid score placement or visible bounds"));
        }
        let super::context::ContextNotes {
            start_seconds,
            notes: source,
            has_alignment_prefix,
        } = super::context::padded(self, family);
        let mut records = Vec::with_capacity(source.len());
        let mut notes = Vec::with_capacity(source.len());
        for (index, note) in source.iter().enumerate() {
            if !note.duration_seconds.is_finite()
                || note.duration_seconds <= 0.0
                || !note.pitch.is_finite()
                || !(0.0..=127.0).contains(&note.pitch)
                || note.phones.is_empty()
                || note.parts == 0
                || note.part_index >= note.parts
            {
                return Err(VoiceError::new(format!(
                    "Score record {index} has invalid duration, pitch, syllable partition or empty phones"
                )));
            }
            for phone in &note.phones {
                if phone.token.is_empty()
                    || phone.token.chars().any(char::is_whitespace)
                    || phone
                        .onset_offset_seconds
                        .is_some_and(|value| !value.is_finite())
                {
                    return Err(VoiceError::new(format!(
                        "Score record {index} contains an invalid explicit phone"
                    )));
                }
                let resolved = duration_tree.resolve_phone(&phone.language, &phone.token)?;
                if resolved.used_silence_fallback
                    || (!phone.language.is_empty()
                        && !resolved.selected_phone_set.contains(&phone.language))
                {
                    return Err(VoiceError::new(format!(
                        "Score record {index}: unsupported phone {:?} in language {:?}",
                        phone.token, phone.language
                    )));
                }
            }
            let frontend = FrontendNote {
                phones: note
                    .phones
                    .iter()
                    .map(|phone| phone.token.clone())
                    .collect(),
                pitch: note.pitch,
                pitch_offset: 0.0,
                duration_seconds: note.duration_seconds,
                frame_seconds: FRAME_SECONDS,
                kind: note.kind,
                flag: note.flag,
                tag: note.tag.clone(),
                parts: note.parts,
                part_index: note.part_index,
                appended_durations: Vec::new(),
            };
            // The internal alignment remainder may round to zero frames. Its
            // time still participates in phrase placement; source notes must
            // always span at least one model frame.
            if family != VoiceFamily::Sv1
                && frame_count(&frontend)? == 0
                && !(has_alignment_prefix && index == 0)
            {
                return Err(VoiceError::new(format!(
                    "Score record {index} is shorter than one model frame"
                )));
            }
            records.push(DurationRecord {
                phones: note
                    .phones
                    .iter()
                    .map(|phone| ExplicitPhone {
                        language: phone.language.clone(),
                        token: phone.token.clone(),
                        onset_offset_seconds: phone.onset_offset_seconds,
                    })
                    .collect(),
                note: DurationNote {
                    sample_rate: SAMPLE_RATE,
                    hop_samples: HOP_SAMPLES as f64,
                    duration_seconds: note.duration_seconds,
                    pitch: note.pitch,
                },
                linked_durations_seconds: Vec::new(),
                parts: note.parts,
                part_index: note.part_index,
                pitch_offset: 0.0,
            });
            notes.push(frontend);
        }
        super::records::redistribute_onsets(duration_tree, &mut records, &mut notes, family)?;
        Ok(PreparedPhrase {
            pitch_controls: None,
            records,
            notes,
            source_note_ids: source.iter().map(|note| note.source_note_id).collect(),
            start_seconds,
            visible_begin_seconds: self.visible_begin_seconds,
            visible_end_seconds: self.visible_end_seconds,
        })
    }
}
