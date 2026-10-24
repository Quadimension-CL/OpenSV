use super::LyricFrontend;
mod relaxed;
pub(super) mod special;
use crate::{
    model::{Language, PronunciationDictionary},
    synthesis::{
        plan::NoteInput,
        voice::{PreparedTree, Result, VoiceError},
    },
};

fn explicit<'a>(note: &'a NoteInput, language: Language,
    dictionary: Option<&'a PronunciationDictionary>) -> Option<&'a str> {
    (!note.phonemes.trim().is_empty())
        .then_some(note.phonemes.as_str())
        .or_else(|| {
            dictionary
                .filter(|dict| dict.language == language)
                .and_then(|dict| dict.entries.get(note.lyrics.trim()).map(String::as_str))
        })
}

fn automatic(note: &NoteInput, language: Language, frontend: &LyricFrontend, european: bool)
    -> Result<Vec<Vec<String>>> {
    if special::marker(&note.lyrics).is_some() {
        return Ok(vec![vec!["sil".into()]]);
    }
    frontend.resolve(language, special::spelling(&note.lyrics), european).map_err(|error| {
            VoiceError::new(format!("音符 {} · {}：{error}", note.id, language.label()))
        })
}

pub(super) struct ResolvedNote {
    pub language: Language,
    pub placeholder_language: Option<Language>,
    pub syllables: Vec<Vec<String>>,
    pub kind: u32,
    pub error: Option<String>,
}
impl ResolvedNote {
    fn new(language: Language, result: Result<Vec<Vec<String>>>) -> Self {
        match result {
            Ok(syllables) => Self {language, placeholder_language: None, syllables, kind: 0, error: None},
            Err(error) => Self {language, placeholder_language: None, syllables: vec![vec!["sil".into()]], kind: 1, error: Some(error.to_string())},
        }
    }
}

fn finish(note: &NoteInput, syllables: Vec<Vec<String>>) -> Vec<Vec<String>> {
    if note
        .attributes
        .get("evenSyllableDuration")
        .and_then(serde_json::Value::as_bool)
        == Some(false)
    {
        vec![syllables.into_iter().flatten().collect()]
    } else {
        syllables
    }
}

pub(super) fn resolve_owner(notes: &[&NoteInput], inherited: Language,
    input: &crate::synthesis::plan::OwnerInput, frontend: &LyricFrontend,
    tree: &PreparedTree, family: crate::synthesis::voice::VoiceFamily)
    -> Result<Vec<ResolvedNote>> {
    let setting = |key, default| match input.voice.instance_options.get("voice")
        .and_then(|voice| voice.get(key)).and_then(serde_json::Value::as_str) {
        Some("true") => true, Some("false") => false, _ => default,
    };
    let enabled = setting("relaxedPronunciation", inherited == Language::English)
        && family == crate::synthesis::voice::VoiceFamily::Sv2;
    let european = setting("europeanPronunciation", false);
    let dictionary = input.voice.dictionary.as_ref();
    let resolved_languages:Vec<_> = notes.iter().map(|note| frontend.note_language(note, inherited)).collect();
    let languages:Vec<_>=resolved_languages.iter().map(|language|language.as_ref().copied().unwrap_or(inherited)).collect();
    let mut output = Vec::with_capacity(notes.len());
    let mut start = 0;
    while start < notes.len() {
        let language = languages[start];
        let mut end = start + 1;
        while end < notes.len() && languages[end] == language
            && notes[end-1].onset.0.checked_add(notes[end-1].duration) == Some(notes[end].onset.0) {
            end += 1;
        }
        let block = &notes[start..end];
        if enabled && language == Language::English {
            let automatic = block.iter().map(|note| {
                if let Some(phones) = special::direct(&note.lyrics) {
                    split_syllables(phones, language, tree)
                } else { automatic(note, language, frontend, european) }
            }).collect::<Vec<_>>();
            let mut words = block.iter().zip(&automatic).map(|(note, syllables)| relaxed::Word {
                lyrics: note.lyrics.trim().to_lowercase(),
                beats: note.duration as f64 / crate::model::QUARTER as f64,
                syllables: syllables.as_ref().cloned().unwrap_or_else(|_| vec![vec!["sil".into()]]),
            }).collect::<Vec<_>>();
            relaxed::apply(&mut words, tree)?;
            for ((_note, word), original) in block.iter().zip(words).zip(automatic) {
                let syllables = original.map(|_| word.syllables);
                output.push(ResolvedNote::new(language, syllables));
            }
        } else {
            for note in block {

                let result = match special::direct(&note.lyrics) {
                    Some(phones) => split_syllables(phones, language, tree),
                    None => automatic(note, language, frontend, european),
                };
                output.push(ResolvedNote::new(language, result));
            }
            if matches!(language,Language::French|Language::Portuguese) {

                let indexes:Vec<_>=(start..end).filter(|&i|output[i].error.is_none()
                    && special::marker(&notes[i].lyrics).is_none()
                    && special::direct(&notes[i].lyrics).is_none()).collect();
                let lyrics:Vec<_>=indexes.iter().map(|&i|notes[i].lyrics.as_str()).collect();
                let mut words:Vec<_>=indexes.iter().map(|&i|output[i].syllables.clone()).collect();
                frontend.link_european_words(language,&lyrics,&mut words)?;
                for (index,syllables) in indexes.into_iter().zip(words) {output[index].syllables=syllables;}
            }
        }
        start = end;
    }
    special::distribute(notes, &mut output, tree, frontend, frontend.default_language())?;
    for ((note, row),language) in notes.iter().zip(&mut output).zip(resolved_languages) {

        if let Some(phones)=explicit(note,row.language,dictionary) {
            *row=ResolvedNote::new(row.language,split_syllables(phones,row.language,tree));
            special::validate(row,tree,frontend)?;
        }
        if let Err(error)=language {*row=ResolvedNote::new(inherited,Err(error));}
        row.syllables = finish(note, std::mem::take(&mut row.syllables));
    }
    Ok(output)
}

fn split_syllables(
    phones: &str,
    language: Language,
    tree: &PreparedTree,
) -> Result<Vec<Vec<String>>> {
    let tokens: Vec<String> = phones.split_whitespace().map(str::to_owned).collect();
    if language==Language::English {
        let classes=tokens.iter().map(|token|tree.resolve_phone(language.id(),token).map(|p|p.source_class))
            .collect::<Result<Vec<_>>>()?;
        return Ok(super::lyrics::english::split(tokens,&classes));
    }
    let mut nuclei = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let phone = tree.resolve_phone(language.id(), token)?;
        if matches!(phone.source_class, Some("vowel" | "diphthong")) {
            nuclei.push(index);
        }
    }
    if matches!(language,Language::French|Language::German|Language::Portuguese) {
        return Ok(super::lyrics::european::split(tokens,&nuclei,language));
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
