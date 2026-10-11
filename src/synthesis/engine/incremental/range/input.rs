use crate::synthesis::plan::OwnerInput;
use std::ops::Range;

/// Pitch, lyric and explicit phoneme edits share the same local pipeline. All
/// other dependencies remain exact; a new layout is classified before inference.
/// In particular, changed/erased manual curves require a fresh F0 prediction:
/// the retained pitch already contains the preceding manual overrides.
pub(super) fn changed_notes(old: &OwnerInput, new: &OwnerInput) -> Option<Range<usize>> {
    if old.notes.len() != new.notes.len() || new.notes.is_empty() {
        return None;
    }
    let mut equivalent = old.clone();
    let mut first = None;
    let mut end = 0;
    for (index, (previous, current)) in equivalent.notes.iter_mut().zip(&new.notes).enumerate() {
        if previous.pitch != current.pitch
            || previous.lyrics != current.lyrics
            || previous.phonemes != current.phonemes
        {
            first.get_or_insert(index);
            end = index + 1;
        }
        previous.pitch = current.pitch;
        previous.lyrics.clone_from(&current.lyrics);
        previous.phonemes.clone_from(&current.phonemes);
    }
    if equivalent != *new
        || new.content_offset.0 < 0
        || new.notes.iter().any(|note| {
            note.onset.0 < 0
                || note.duration <= 0
                || note
                    .attributes
                    .get("muted")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
        })
        || new
            .notes
            .windows(2)
            .any(|pair| pair[0].onset.0.saturating_add(pair[0].duration) != pair[1].onset.0)
    {
        return None;
    }
    Some(first?..end)
}
