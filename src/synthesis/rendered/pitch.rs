use crate::synthesis::{
    score::PreparedPhrase,
    stages::f0::{PitchResult, frontend::frame_count},
    voice::{Result, VoiceError},
};
use std::{ops::Range, sync::Arc};

/// Final model pitch on the same frame clock used by timbre synthesis.
/// Values are absolute MIDI pitches, including the group's pitch offset.
#[derive(Clone, Debug)]
pub struct PitchTrack {
    pub start_seconds: f64,
    pub frame_seconds: f64,
    pub values: Arc<[f32]>,
    /// Maximal contiguous sounding-note ranges. Rest records break the curve;
    /// adjacent syllables remain joined so their predicted transition survives.
    pub segments: Vec<Range<usize>>,
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

    /// Publish using the generating model's note clock. Compatibility DDS
    /// uses cumulative model-frame boundaries; version 2 uses per-record counts.
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
        let mut cursor = 0_usize;
        for (note, &count) in phrase.notes.iter().zip(frames) {
            let end = cursor
                .checked_add(count)
                .ok_or_else(|| VoiceError::new("Rendered pitch frame count overflow"))?;
            // This is the shared note frontend's explicit rest/breath condition,
            // not an amplitude threshold or an invented voicing estimate.
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
        })
    }

    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of_val(self.values.as_ref()).saturating_add(
            self.segments
                .capacity()
                .saturating_mul(std::mem::size_of::<Range<usize>>()),
        )
    }
}
