use crate::synthesis::plan::OwnerInput;
use std::ops::Range;

pub(super) fn changed_notes(old: &OwnerInput, new: &OwnerInput) -> Option<Range<usize>> {
    if old.notes.len() != new.notes.len() || new.notes.is_empty() {
        return None;
    }
    let mut equivalent = old.clone();
    let mut first = None;
    let mut end = 0;
    for (index, (previous, current)) in equivalent.notes.iter_mut().zip(&new.notes).enumerate() {
        let mut attributes=previous.attributes.clone();
        if let Some(phones)=current.attributes.get("phonemes") {attributes.insert("phonemes".into(),phones.clone());}
        else {attributes.remove("phonemes");}
        let phone_edit=attributes==current.attributes && previous.attributes!=current.attributes;
        if previous.pitch != current.pitch
            || previous.lyrics != current.lyrics
            || previous.phonemes != current.phonemes
            || phone_edit
        {
            first.get_or_insert(index);
            end = index + 1;
        }
        previous.pitch = current.pitch;
        previous.lyrics.clone_from(&current.lyrics);
        previous.phonemes.clone_from(&current.phonemes);
        if phone_edit {previous.attributes=attributes;}
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
            .any(|pair| pair[0].onset.0.saturating_add(pair[0].duration) > pair[1].onset.0)
    {
        return None;
    }
    Some(first?..end)
}
