//! Family-specific boundary records for complete phrases.
//! These are neural inputs, not audible padding added to the exported waveform.

use super::{ExplicitScore, FRAME_SECONDS, SAMPLE_RATE, ScoreNote, ScorePhone};
use crate::synthesis::voice::VoiceFamily;

const CONTEXT_SECONDS: f64 = 0.5;
const ALIGNMENT_FRAMES: f64 = 8.0;

pub(super) struct ContextNotes {
    pub start_seconds: f64,
    pub notes: Vec<ScoreNote>,
    pub has_alignment_prefix: bool,
}

pub(super) fn padded(score: &ExplicitScore, family: VoiceFamily) -> ContextNotes {
    match family {
        VoiceFamily::Sv1 => padded_sv1(score),
        VoiceFamily::Sv2Compatible | VoiceFamily::Sv2 => padded_sv2(score),
    }
}

/// SV1 phrase context supplies half a second of silence at either edge,
/// with the edge pitch and flag set, without an extra alignment record.
/// Rounding the output origin to a sample affects PCM placement only;
/// it does not quantize any note or phone duration supplied to the models.
fn padded_sv1(score: &ExplicitScore) -> ContextNotes {
    let Some(first) = score.notes.first() else {
        return ContextNotes {
            start_seconds: score.start_seconds,
            notes: Vec::new(),
            has_alignment_prefix: false,
        };
    };
    let last = score.notes.last().expect("nonempty score");
    let mut leading = silence(first, CONTEXT_SECONDS);
    let mut trailing = silence(last, CONTEXT_SECONDS);
    leading.flag = true;
    trailing.flag = true;
    let mut notes = Vec::with_capacity(score.notes.len() + 2);
    notes.push(leading);
    notes.extend(score.notes.iter().cloned());
    notes.push(trailing);
    ContextNotes {
        start_seconds: ((score.start_seconds - CONTEXT_SECONDS) * f64::from(SAMPLE_RATE)).round()
            / f64::from(SAMPLE_RATE),
        notes,
        has_alignment_prefix: false,
    }
}

/// SV2 phrase context adds half a second on both sides, then rounds the
/// start down to an eight-frame grid with a separate leading silence record.
/// Compatible voices share this input preparation.
fn padded_sv2(score: &ExplicitScore) -> ContextNotes {
    let Some(first) = score.notes.first() else {
        return ContextNotes {
            start_seconds: score.start_seconds,
            notes: Vec::new(),
            has_alignment_prefix: false,
        };
    };
    let last = score.notes.last().expect("nonempty score");
    let mut origin = score.start_seconds - CONTEXT_SECONDS;
    let aligned =
        (origin / FRAME_SECONDS / ALIGNMENT_FRAMES).floor() * FRAME_SECONDS * ALIGNMENT_FRAMES;
    let remainder = origin - aligned;
    let has_alignment_prefix = remainder > 0.0001;
    let mut notes = Vec::with_capacity(score.notes.len() + 3);
    if has_alignment_prefix {
        let mut prefix = silence(first, remainder);
        prefix.flag = false;
        notes.push(prefix);
        origin = aligned;
    }
    notes.push(silence(first, CONTEXT_SECONDS));
    notes.extend(score.notes.iter().cloned());
    notes.push(silence(last, CONTEXT_SECONDS));
    ContextNotes {
        start_seconds: origin,
        notes,
        has_alignment_prefix,
    }
}

fn silence(edge: &ScoreNote, duration_seconds: f64) -> ScoreNote {
    ScoreNote {
        source_note_id: None,
        retake_seeds: Default::default(),
        performance: Default::default(),
        duration_seconds,
        pitch: edge.pitch,
        phones: vec![ScorePhone {
            language: edge
                .phones
                .first()
                .map_or_else(String::new, |phone| phone.language.clone()),
            token: "sil".into(),
            source: None,
            attributes: Default::default(),
            source_note_onset_seconds: None,
        }],
        kind: 1,
        flag: edge.flag,
        tag: None,
        parts: 1,
        part_index: 0,
    }
}
