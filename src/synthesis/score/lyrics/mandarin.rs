//! Mandarin lookup from the configured pronunciation resources.
//!
//! Exact lexical lookup does not disambiguate pronunciation from context.
//! Different phoneme readings require explicit phonemes; tone-only
//! alternatives share the score's sung pitch.

use std::{collections::HashMap, fs, path::Path};

use crate::synthesis::voice::{Result, VoiceError};

#[derive(Debug)]
pub struct MandarinLexicon {
    syllables: HashMap<String, Vec<String>>,
    words: HashMap<String, Vec<Vec<String>>>,
    longest_word: usize,
}

impl MandarinLexicon {
    pub fn load(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref();
        let syllable_path = directory.join("mandarin-xsampa-dict.txt");
        let word_path = directory.join("cedict.txt");
        let mut syllables = HashMap::new();
        for (index, line) in fs::read_to_string(&syllable_path)?.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let key = fields.next().expect("nonempty line");
            let phones: Vec<String> = fields.map(str::to_owned).collect();
            if phones.is_empty() {
                return Err(VoiceError::new(format!(
                    "{}:{}: Mandarin syllable has no phonemes",
                    syllable_path.display(),
                    index + 1
                )));
            }
            if let Some(previous) = syllables.insert(key.to_owned(), phones.clone())
                && previous != phones
            {
                return Err(VoiceError::new(format!(
                    "Conflicting Mandarin phoneme entries for {key:?}"
                )));
            }
        }
        let mut words: HashMap<String, Vec<Vec<String>>> = HashMap::new();
        let mut longest_word = 0;
        for (index, line) in fs::read_to_string(&word_path)?.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((spellings, pronunciation)) = line.split_once('[') else {
                return Err(malformed_word(&word_path, index));
            };
            let Some(pronunciation) = pronunciation.strip_suffix(']') else {
                return Err(malformed_word(&word_path, index));
            };
            let spellings: Vec<_> = spellings.split_whitespace().collect();
            let pronunciation: Vec<_> = pronunciation
                .split_whitespace()
                .map(|syllable| syllable.to_lowercase().replace("u:", "v").replace('ü', "v"))
                .collect();
            if spellings.len() != 2 || pronunciation.is_empty() {
                return Err(malformed_word(&word_path, index));
            }
            for spelling in spellings {
                longest_word = longest_word.max(spelling.chars().count());
                let alternatives = words.entry(spelling.to_owned()).or_default();
                if !alternatives.contains(&pronunciation) {
                    alternatives.push(pronunciation.clone());
                }
            }
        }
        if syllables.is_empty() || words.is_empty() {
            return Err(VoiceError::new("Mandarin dictionaries are empty"));
        }
        Ok(Self {
            syllables,
            words,
            longest_word,
        })
    }

    /// Returns one phoneme list per syllable, preserving lyric order.
    pub fn resolve(&self, lyrics: &str) -> Result<Vec<Vec<String>>> {
        let lyrics = lyrics.trim();
        if lyrics.is_empty() {
            return Err(VoiceError::new(
                "Empty Mandarin lyrics require explicit phonemes",
            ));
        }
        if self.words.contains_key(lyrics) {
            return self.resolve_word(lyrics);
        }
        let mut result = Vec::new();
        for token in lyrics.split(|ch: char| ch.is_whitespace() || ch == '\'' || ch == '’') {
            if token.is_empty() {
                continue;
            }
            if self.words.contains_key(token) {
                result.extend(self.resolve_word(token)?);
            } else if token.chars().any(is_han) {
                result.extend(self.resolve_phrase(token)?);
            } else {
                result.push(self.resolve_syllable(token)?);
            }
        }
        if result.is_empty() {
            return Err(VoiceError::new("Mandarin lyrics contain no syllables"));
        }
        Ok(result)
    }

    fn resolve_word(&self, word: &str) -> Result<Vec<Vec<String>>> {
        let alternatives = self.words.get(word).ok_or_else(|| unknown(word))?;
        let mut phoneme_readings = Vec::new();
        for alternative in alternatives {
            let phones = alternative
                .iter()
                .map(|syllable| self.resolve_syllable(syllable))
                .collect::<Result<Vec<_>>>()?;
            if !phoneme_readings.contains(&phones) {
                phoneme_readings.push(phones);
            }
        }
        if phoneme_readings.len() > 1 {
            let readings = alternatives
                .iter()
                .map(|entry| entry.join(" "))
                .collect::<Vec<_>>();
            return Err(VoiceError::new(format!(
                "Ambiguous Mandarin word {word:?}: {}; provide explicit phonemes or pinyin",
                readings.join(" / ")
            )));
        }
        phoneme_readings.pop().ok_or_else(|| unknown(word))
    }

    fn resolve_phrase(&self, phrase: &str) -> Result<Vec<Vec<String>>> {
        let mut boundaries: Vec<_> = phrase.char_indices().map(|(index, _)| index).collect();
        boundaries.push(phrase.len());
        let mut start = 0;
        let mut result = Vec::new();
        while start + 1 < boundaries.len() {
            let last = (start + self.longest_word).min(boundaries.len() - 1);
            let matched = ((start + 1)..=last).rev().find(|&end| {
                self.words
                    .contains_key(&phrase[boundaries[start]..boundaries[end]])
            });
            let Some(end) = matched else {
                return Err(unknown(&phrase[boundaries[start]..]));
            };
            result.extend(self.resolve_word(&phrase[boundaries[start]..boundaries[end]])?);
            start = end;
        }
        Ok(result)
    }

    fn resolve_syllable(&self, syllable: &str) -> Result<Vec<String>> {
        let key = pinyin_key(syllable).ok_or_else(|| unknown(syllable))?;
        self.syllables
            .get(&key)
            .cloned()
            .ok_or_else(|| unknown(syllable))
    }
}

fn malformed_word(path: &Path, zero_based_line: usize) -> VoiceError {
    VoiceError::new(format!(
        "{}:{}: expected traditional simplified [pinyin syllables]",
        path.display(),
        zero_based_line + 1
    ))
}

fn unknown(text: &str) -> VoiceError {
    VoiceError::new(format!(
        "No exact installed Mandarin pronunciation for {text:?}; provide explicit phonemes or spaced pinyin"
    ))
}

fn is_han(ch: char) -> bool {
    matches!(ch, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}'
        | '\u{f900}'..='\u{faff}' | '\u{20000}'..='\u{323af}')
}

/// Tone spelling does not change this dictionary's toneless phoneme inventory;
/// the score supplies the sung pitch. No syllable segmentation is inferred.
fn pinyin_key(syllable: &str) -> Option<String> {
    let mut value = syllable.to_lowercase().replace("u:", "v");
    if value.ends_with(['0', '1', '2', '3', '4', '5']) {
        value.pop();
    }
    let mut key = String::with_capacity(value.len());
    for ch in value.chars() {
        key.push(match ch {
            'ā' | 'á' | 'ǎ' | 'à' => 'a',
            'ē' | 'é' | 'ě' | 'è' => 'e',
            'ī' | 'í' | 'ǐ' | 'ì' => 'i',
            'ō' | 'ó' | 'ǒ' | 'ò' => 'o',
            'ū' | 'ú' | 'ǔ' | 'ù' => 'u',
            'ü' | 'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' => 'v',
            'a'..='z' => ch,
            _ => return None,
        });
    }
    (!key.is_empty()).then_some(key)
}
