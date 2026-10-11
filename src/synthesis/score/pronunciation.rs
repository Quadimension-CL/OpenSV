use super::LyricFrontend;
use crate::{
    model::{Language, PronunciationDictionary},
    synthesis::{
        plan::NoteInput,
        voice::{PreparedTree, Result, VoiceError},
    },
};

pub(super) fn resolve_note(
    note: &NoteInput,
    language: Language,
    frontend: &LyricFrontend,
    dictionary: Option<&PronunciationDictionary>,
    tree: &PreparedTree,
) -> Result<Vec<Vec<String>>> {
    let explicit = (!note.phonemes.trim().is_empty())
        .then_some(note.phonemes.as_str())
        .or_else(|| {
            dictionary
                .filter(|dict| dict.language == language)
                .and_then(|dict| dict.entries.get(note.lyrics.trim()).map(String::as_str))
        });
    let syllables = if let Some(phones) = explicit {
        split_syllables(phones, language, tree)?
    } else {
        frontend.resolve(language, &note.lyrics).map_err(|error| {
            VoiceError::new(format!("音符 {} · {}：{error}", note.id, language.label()))
        })?
    };
    if note
        .attributes
        .get("evenSyllableDuration")
        .and_then(serde_json::Value::as_bool)
        == Some(false)
    {
        Ok(vec![syllables.into_iter().flatten().collect()])
    } else {
        Ok(syllables)
    }
}

fn split_syllables(
    phones: &str,
    language: Language,
    tree: &PreparedTree,
) -> Result<Vec<Vec<String>>> {
    let tokens: Vec<String> = phones.split_whitespace().map(str::to_owned).collect();
    let mut nuclei = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let phone = tree.resolve_phone(language.id(), token)?;
        if matches!(phone.source_class, Some("vowel" | "diphthong")) {
            nuclei.push(index);
        }
    }
    let mut parts = Vec::new();
    let mut start = 0;
    for &end in nuclei.iter().take(nuclei.len().saturating_sub(1)) {
        parts.push(tokens[start..=end].to_vec());
        start = end + 1;
    }
    parts.push(tokens[start..].to_vec());
    Ok(parts)
}
