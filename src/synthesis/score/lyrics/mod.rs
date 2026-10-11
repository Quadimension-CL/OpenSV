mod dictionary;
mod japanese;
mod mandarin;

use crate::{
    model::Language,
    synthesis::{
        plan::{NoteInput, OwnerInput},
        voice::{Result, VoiceError},
    },
};
use dictionary::Dictionary;
use mandarin::MandarinLexicon;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// Immutable resource dictionaries are loaded on first use of a language, not
/// on each edit and not all at application startup. They never cache rendered audio.
pub struct LyricFrontend {
    directory: PathBuf,
    default_language: Language,
    dictionaries: HashMap<Language, OnceLock<Result<Lexicon>>>,
}

enum Lexicon {
    Mandarin(MandarinLexicon),
    Japanese(japanese::Japanese),
    Dictionary(Dictionary),
}

impl LyricFrontend {
    pub fn new(
        directory: impl AsRef<Path>,
        default_language: &str,
        supported: &[String],
    ) -> Result<Self> {
        let parse = |id: &str| {
            Language::parse(id).ok_or_else(|| VoiceError::new(format!("无法识别声库语言：{id}")))
        };
        let default_language = parse(default_language)?;
        let dictionaries = supported
            .iter()
            .map(|id| Ok((parse(id)?, OnceLock::new())))
            .collect::<Result<HashMap<_, _>>>()?;
        if !dictionaries.contains_key(&default_language) {
            return Err(VoiceError::new("声库未声明支持其默认语言"));
        }
        Ok(Self {
            directory: directory.as_ref().to_owned(),
            default_language,
            dictionaries,
        })
    }

    pub(super) fn language(&self, input: &OwnerInput) -> Result<Language> {
        let language = super::options::language(input, self.default_language)?;
        if !self.dictionaries.contains_key(&language) {
            return Err(VoiceError::new(format!(
                "当前声库不支持{}",
                language.label()
            )));
        }
        Ok(language)
    }

    pub(super) fn note_language(&self, note: &NoteInput, inherited: Language) -> Result<Language> {
        let language = super::options::note_language(note, inherited)?;
        if !self.dictionaries.contains_key(&language) {
            return Err(VoiceError::new(format!(
                "当前声库不支持{}",
                language.label()
            )));
        }
        Ok(language)
    }

    pub(super) fn resolve(&self, language: Language, lyrics: &str) -> Result<Vec<Vec<String>>> {
        let dictionary = self
            .dictionaries
            .get(&language)
            .ok_or_else(|| VoiceError::new(format!("当前声库不支持{}", language.label())))?
            .get_or_init(|| match language {
                Language::Mandarin => MandarinLexicon::load(&self.directory).map(Lexicon::Mandarin),
                Language::Japanese => {
                    japanese::Japanese::load(&self.directory).map(Lexicon::Japanese)
                }
                _ => Dictionary::load(&self.directory, language).map(Lexicon::Dictionary),
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let result = match dictionary {
            Lexicon::Mandarin(dictionary) => dictionary.resolve(lyrics),
            Lexicon::Japanese(dictionary) => dictionary.resolve(lyrics),
            Lexicon::Dictionary(dictionary) => dictionary.resolve(lyrics),
        }?;
        if result.is_empty() || result.iter().any(Vec::is_empty) {
            return Err(VoiceError::new(format!(
                "{}歌词没有可合成的音素",
                language.label()
            )));
        }
        Ok(result)
    }
}
