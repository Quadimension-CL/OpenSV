use super::Editor;
use crate::model::EntityId;
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

pub struct LyricSelection {
    group: String,
    notes: Vec<EntityId>,
}

impl LyricSelection {
    pub fn len(&self) -> usize {
        self.notes.len()
    }
}

impl Editor {
    pub fn lyric_selection(&self) -> Option<(LyricSelection, String)> {
        let reference = self.active_ref()?;
        let mut notes: Vec<_> = self
            .notes()
            .iter()
            .filter(|n| self.selected.contains(&n.id) && reference.note_range(n).is_some())
            .collect();
        if notes.is_empty() {
            return None;
        }
        notes.sort_by_key(|n| (n.onset, n.id));
        let text = notes
            .iter()
            .map(|n| n.lyrics.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        Some((
            LyricSelection {
                group: reference.group_id.clone(),
                notes: notes.iter().map(|n| n.id).collect(),
            },
            text,
        ))
    }

    pub fn fill_lyrics(
        &mut self,
        selection: &LyricSelection,
        words: &[String],
        repeat: bool,
    ) -> usize {
        if words.is_empty() {
            return 0;
        }
        let count = if repeat {
            selection.len()
        } else {
            selection.len().min(words.len())
        };
        let lyrics: BTreeMap<_, _> = selection
            .notes
            .iter()
            .take(count)
            .zip(words.iter().cycle())
            .map(|(&id, word)| (id, word))
            .collect();
        self.begin("填入歌词");
        let mut changed = 0;
        if let Some(group) = self.project.group_mut(&selection.group) {
            for note in &mut group.notes {
                if let Some(word) = lyrics.get(&note.id)
                    && note.lyrics != **word
                {
                    note.lyrics.clone_from(word);
                    // An explicit pronunciation must not override the newly entered lyric.
                    note.phonemes.clear();
                    note.attributes.remove("phonemes");
                    changed += 1;
                }
            }
        }
        self.finish();
        changed
    }
}

pub fn split_lyrics(text: &str, by_character: bool) -> Vec<String> {
    let mut result = Vec::new();
    for word in text.split_whitespace() {
        if !by_character {
            result.push(word.to_owned());
            continue;
        }
        let mut letters = String::new();
        let first = result.len();
        for grapheme in word.graphemes(true) {
            let ch = grapheme.chars().next().expect("nonempty grapheme");
            if matches!(ch,
                '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' |
                '\u{f900}'..='\u{faff}' | '\u{20000}'..='\u{323af}' |
                '\u{3041}'..='\u{3096}' | '\u{30a1}'..='\u{30fa}' |
                '\u{30fc}' | '\u{ff66}'..='\u{ff9d}' |
                '\u{1100}'..='\u{11ff}' | '\u{ac00}'..='\u{d7a3}'
            ) {
                if !letters.is_empty() {
                    result.push(std::mem::take(&mut letters));
                }
                // Keep small kana with their preceding kana, e.g. きゃ and ファ.
                let previous_kana = result.len() > first
                    && result.last().is_some_and(|s| {
                        s.ends_with(|c| matches!(c, '\u{3041}'..='\u{3096}' | '\u{30a1}'..='\u{30fa}' | '\u{ff66}'..='\u{ff9d}'))
                    });
                if previous_kana && "ぁぃぅぇぉゃゅょゎァィゥェォャュョヮｧｨｩｪｫｬｭｮ".contains(ch)
                {
                    result.last_mut().unwrap().push_str(grapheme);
                } else {
                    result.push(grapheme.to_owned());
                }
            } else {
                letters.push_str(grapheme);
            }
        }
        if !letters.is_empty() {
            result.push(letters);
        }
    }
    result
}
