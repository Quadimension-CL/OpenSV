//! Exact installed dictionary lookups. This does not implement the editor's
//! statistical pronunciation models or choose between ambiguous word readings.

use crate::{
    model::Language,
    synthesis::voice::{Result, VoiceError},
};
use std::{collections::HashMap, fs, path::Path};

pub(super) type Phones = Vec<String>;

#[derive(Default)]
pub(super) struct PhoneTable {
    entries: HashMap<String, Vec<Phones>>,
    max_key_bytes: usize,
}

impl PhoneTable {
    pub(super) fn load(path: &Path, english: bool) -> Result<Self> {
        let source = read(path)?;
        let mut table = Self::default();
        for line in source
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with(['#', ';']))
        {
            let mut fields = line.trim_start_matches('\u{feff}').split_whitespace();
            let Some(key) = fields.next() else {
                continue;
            };
            let key = if english {
                key.split('(').next().unwrap_or(key)
            } else {
                key
            };
            let phones: Phones = fields
                .map(|phone| {
                    if english {
                        phone.trim_end_matches(['0', '1', '2']).to_lowercase()
                    } else {
                        phone.to_owned()
                    }
                })
                .collect();
            let key = key.to_lowercase();
            table.max_key_bytes = table.max_key_bytes.max(key.len());
            let alternatives = table.entries.entry(key).or_default();
            if !alternatives.contains(&phones) {
                alternatives.push(phones);
            }
        }
        if table.entries.is_empty() {
            return Err(VoiceError::new(format!("词典为空：{}", path.display())));
        }
        Ok(table)
    }

    pub(super) fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub(super) fn include_literal(&mut self, phone: &str) {
        let key = phone.to_lowercase();
        self.max_key_bytes = self.max_key_bytes.max(key.len());
        self.entries
            .entry(key)
            .or_insert_with(|| vec![vec![phone.to_owned()]]);
    }

    pub(super) fn get(&self, key: &str) -> Result<Phones> {
        let alternatives = self.entries.get(key).ok_or_else(|| unknown(key))?;
        if alternatives.len() != 1 {
            return Err(VoiceError::new(format!(
                "{key:?} 有多个词典读音，请填写显式音素"
            )));
        }
        Ok(alternatives[0].clone())
    }

    /// Longest literal key matching, used by the supplied romanized syllable
    /// tables. Unknown spans are errors, never replaced with guessed phones.
    pub(super) fn segment(&self, token: &str) -> Result<Vec<Phones>> {
        let mut rest = token;
        let mut output = Vec::new();
        while !rest.is_empty() {
            let end = rest
                .char_indices()
                .map(|(index, _)| index)
                .chain(std::iter::once(rest.len()))
                .take_while(|&end| end <= self.max_key_bytes)
                .filter(|&end| end > 0 && self.contains(&rest[..end]))
                .max()
                .ok_or_else(|| unknown(rest))?;
            let phones = self.get(&rest[..end])?;
            if !phones.is_empty() {
                output.push(phones);
            }
            rest = &rest[end..];
        }
        Ok(output)
    }
}

pub(super) struct Dictionary {
    language: Language,
    phones: PhoneTable,
    words: WordReadings,
    nuclei: Vec<String>,
}

impl Dictionary {
    pub(super) fn load(directory: &Path, language: Language) -> Result<Self> {
        let english = language == Language::English;
        let name = if english {
            "cmudict-07b.txt".into()
        } else {
            format!("{}-dict.txt", language.phone_set())
        };
        let phones = PhoneTable::load(&directory.join(name), english)?;
        let words = if language == Language::Cantonese {
            WordReadings::load(&directory.join("cedict-canto.txt"))?
        } else {
            WordReadings::default()
        };
        let nuclei = read(&directory.join(format!("{}-phones.txt", language.phone_set())))?
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let phone = fields.next()?;
                matches!(fields.next()?, "vowel" | "diphthong").then(|| phone.to_owned())
            })
            .collect();
        Ok(Self {
            language,
            phones,
            words,
            nuclei,
        })
    }

    pub(super) fn resolve(&self, lyrics: &str) -> Result<Vec<Phones>> {
        let mut result = Vec::new();
        for token in lyrics.split_whitespace() {
            if self.language == Language::Cantonese && !token.is_ascii() {
                for reading in self.words.resolve(token)? {
                    result.extend(self.romanized(&reading)?);
                }
            } else {
                result.extend(self.romanized(token)?);
            }
        }
        if result.is_empty() {
            return Err(unknown(lyrics));
        }
        Ok(result)
    }

    fn romanized(&self, token: &str) -> Result<Vec<Phones>> {
        let token = token.to_lowercase();
        match self.language {
            Language::English => Ok(split_nuclei(self.phones.get(&token)?, &self.nuclei)),
            Language::Cantonese => {
                Ok(vec![self.phones.get(
                    token.trim_end_matches(['1', '2', '3', '4', '5', '6']),
                )?])
            }
            // Korean dictionary entries are romanized syllables. The installed
            // korean-dict.txt contains placeholder readings, not usable G2P.
            Language::Korean => Ok(vec![self.phones.get(&token)?]),
            Language::Spanish => {
                let phones = self.phones.segment(&token)?.into_iter().flatten().collect();
                Ok(split_nuclei(phones, &self.nuclei))
            }
            _ => unreachable!("dedicated language adapters"),
        }
    }
}

fn split_nuclei(phones: Phones, nuclei: &[String]) -> Vec<Phones> {
    let positions: Vec<_> = phones
        .iter()
        .enumerate()
        .filter_map(|(i, phone)| nuclei.contains(phone).then_some(i))
        .collect();
    let mut start = 0;
    let mut result = Vec::new();
    for &index in positions.iter().take(positions.len().saturating_sub(1)) {
        result.push(phones[start..=index].to_vec());
        start = index + 1;
    }
    if start < phones.len() {
        result.push(phones[start..].to_vec());
    }
    result
}

#[derive(Default)]
pub(super) struct WordReadings {
    entries: HashMap<String, Vec<String>>,
    max_key_bytes: usize,
}

impl WordReadings {
    pub(super) fn load(path: &Path) -> Result<Self> {
        let mut result = Self::default();
        for (index, line) in read(path)?.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (keys, reading) = line
                .split_once('[')
                .and_then(|(keys, rest)| Some((keys, rest.strip_suffix(']')?)))
                .ok_or_else(|| {
                    VoiceError::new(format!(
                        "{}:{}: 词典读音格式错误",
                        path.display(),
                        index + 1
                    ))
                })?;
            for key in keys.split_whitespace() {
                let values = result.entries.entry(key.to_owned()).or_default();
                // Ignore Cantonese tone-only alternatives in a sung score.
                let reading = reading
                    .split_whitespace()
                    .map(|s| s.trim_end_matches(['1', '2', '3', '4', '5', '6']))
                    .collect::<Vec<_>>()
                    .join(" ");
                if !values.contains(&reading) {
                    values.push(reading);
                }
                result.max_key_bytes = result.max_key_bytes.max(key.len());
            }
        }
        Ok(result)
    }

    pub(super) fn resolve(&self, text: &str) -> Result<Vec<String>> {
        let mut rest = text;
        let mut result = Vec::new();
        while !rest.is_empty() {
            let end = rest
                .char_indices()
                .map(|(index, _)| index)
                .chain(std::iter::once(rest.len()))
                .take_while(|&end| end <= self.max_key_bytes)
                .filter(|&end| end > 0 && self.entries.contains_key(&rest[..end]))
                .max()
                .ok_or_else(|| unknown(rest))?;
            let readings = &self.entries[&rest[..end]];
            if readings.len() != 1 {
                return Err(VoiceError::new(format!(
                    "{:?} 有多个读音，请使用注音或显式音素",
                    &rest[..end]
                )));
            }
            result.extend(readings[0].split_whitespace().map(str::to_owned));
            rest = &rest[end..];
        }
        Ok(result)
    }
}

pub(super) fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .map_err(|error| VoiceError::new(format!("无法读取发音词典 {}：{error}", path.display())))
}

pub(super) fn unknown(text: &str) -> VoiceError {
    VoiceError::new(format!(
        "词典未覆盖歌词 {text:?}，请使用对应语言的注音或显式音素"
    ))
}
