use crate::synthesis::{
    score::PreparedPhrase,
    stages::f0::{PitchResult, frontend::frame_count},
    voice::{Result, VoiceError},
};
use std::{ops::Range, sync::Arc};

#[derive(Clone, Debug)]
pub struct PitchTrack {
    pub start_seconds: f64,
    pub frame_seconds: f64,
    pub values: Arc<[f32]>,

    pub segments: Vec<Range<usize>>,
    pub rap_controls: Arc<[RenderedRapControl]>,

    pub unison_range: Option<[Arc<[f32]>; 2]>,
}

#[derive(Clone, Debug)]
pub struct RenderedRapControl {
    pub note_id: u64,
    pub tone: f32,
    pub intonation: f32,
}

impl PitchTrack {
    pub fn from_phrase(phrase: &PreparedPhrase, prediction: PitchResult) -> Result<Self> {
        let frames = phrase
            .notes
            .iter()
            .map(frame_count)
            .collect::<Result<Vec<_>>>()?;
        Self::from_phrase_with_frames(phrase, prediction, &frames)
    }

    pub(crate) fn from_phrase_with_frames(
        phrase: &PreparedPhrase,
        prediction: PitchResult,
        frames: &[usize],
    ) -> Result<Self> {
        if frames.len() != phrase.notes.len() {
            return Err(VoiceError::new(
                "Rendered pitch note clock differs from its phrase",
            ));
        }
        if !phrase.start_seconds.is_finite()
            || !prediction.frame_seconds.is_finite()
            || prediction.frame_seconds <= 0.0
            || prediction.pitch.iter().any(|pitch| !pitch.is_finite())
        {
            return Err(VoiceError::new(
                "Rendered pitch has invalid values or timing",
            ));
        }
        let mut segments: Vec<Range<usize>> = Vec::new();
        let mut rap_controls = Vec::new();
        let mut cursor = 0_usize;
        for (index, (note, &count)) in phrase.notes.iter().zip(frames).enumerate() {
            let end = cursor
                .checked_add(count)
                .ok_or_else(|| VoiceError::new("Rendered pitch frame count overflow"))?;
            if count > 0 && let Some(note_id) = phrase.source_note_ids[index] {
                let tone = prediction.predicted_controls.offset.get(cursor);
                let intonation = prediction.predicted_controls.slope.get(cursor);
                if let (Some(&tone), Some(&intonation)) = (tone, intonation)
                    && tone.is_finite() && intonation.is_finite()
                    && !rap_controls.iter().any(|v: &RenderedRapControl| v.note_id == note_id)
                {
                    rap_controls.push(RenderedRapControl {note_id, tone, intonation});
                }
            }

            let silent = note.phones.len() == 1
                && (note.kind == 1 || matches!(note.phones[0].as_str(), "sil" | "pau" | "br"));
            if !silent && cursor < end {
                if let Some(previous) = segments.last_mut()
                    && previous.end == cursor
                {
                    previous.end = end;
                } else {
                    segments.push(cursor..end);
                }
            }
            cursor = end;
        }
        if cursor != prediction.pitch.len() {
            return Err(VoiceError::new(
                "Rendered pitch does not cover the prepared note timeline",
            ));
        }
        Ok(Self {
            start_seconds: phrase.start_seconds,
            frame_seconds: prediction.frame_seconds,
            values: prediction.pitch.into(),
            segments,
            rap_controls: rap_controls.into(),
            unison_range: None,
        })
    }

    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of_val(self.values.as_ref()).saturating_add(
            self.segments
                .capacity()
                .saturating_mul(std::mem::size_of::<Range<usize>>()),
        ).saturating_add(std::mem::size_of_val(self.rap_controls.as_ref()))
        .saturating_add(self.unison_range.as_ref().map_or(0,|range|
            range.iter().map(|values|std::mem::size_of_val(values.as_ref())).sum()))
    }
}
