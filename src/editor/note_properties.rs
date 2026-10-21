use super::Editor;
use crate::model::Language;

impl Editor {
    pub fn set_note_phonemes(&mut self, group_id: &str, notes: &[u64], phonemes: &str) {
        let value = phonemes.split_whitespace().collect::<Vec<_>>().join(" ");
        self.begin("编辑音素");
        if let Some(group) = self.project.group_mut(group_id) {
            for note in &mut group.notes {
                if notes.contains(&note.id) && note.phonemes != value {
                    note.phonemes.clone_from(&value);
                    note.attributes.remove("phonemes");
                }
            }
        }
        self.finish();
    }

    pub fn set_note_language(&mut self, language: Option<Language>) {
        self.edit_selected_notes("切换音符语言", |note| {
            let current_language = note.attributes.get("languageOverride").cloned();
            let current_alphabet = note.attributes.get("phonesetOverride").cloned();
            if let Some(language) = language {
                note.attributes
                    .insert("languageOverride".into(), language.id().into());
                note.attributes.insert(
                    "phonesetOverride".into(),
                    language.phoneme_alphabet().into(),
                );
            } else {
                note.attributes.remove("languageOverride");
                note.attributes.remove("phonesetOverride");
            }
            if current_language.as_ref() != note.attributes.get("languageOverride")
                || current_alphabet.as_ref() != note.attributes.get("phonesetOverride")
            {
                note.attributes.remove("phonemes");
            }
        });
    }

    pub fn set_even_syllables(&mut self, even: bool) {
        self.edit_selected_notes("调整音节分配", |note| {
            note.attributes
                .insert("evenSyllableDuration".into(), even.into());
        });
    }

    fn edit_selected_notes(&mut self, label: &str, mut edit: impl FnMut(&mut crate::model::Note)) {
        if self.selected.is_empty() {
            return;
        }
        self.begin(label);
        let selected = self.selected.clone();
        if let Some(group) = self.group_mut() {
            for note in &mut group.notes {
                if selected.contains(&note.id) {
                    edit(note);
                }
            }
        }
        self.finish();
    }
}
