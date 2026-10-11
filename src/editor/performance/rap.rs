use super::{Editor, Note, NotePerformance, PerformanceParameter};
use std::collections::BTreeSet;

impl Editor {
    pub fn set_rap_accent(&mut self, accent: Option<u8>) -> bool {
        if self.is_editing() || accent.is_some_and(|value| !(1..=5).contains(&value)) {
            return false;
        }
        let Some(group) = self
            .active_ref()
            .map(|reference| reference.group_id.clone())
        else {
            return false;
        };
        let notes: BTreeSet<_> = self
            .performance_notes()
            .iter()
            .map(|note| note.id)
            .collect();
        if notes.is_empty() {
            return false;
        }
        self.begin("切换说唱声调");
        let value = accent.map_or_else(String::new, |accent| accent.to_string());
        if let Some(group) = self.project.group_mut(&group) {
            for note in &mut group.notes {
                if notes.contains(&note.id) {
                    note.extra.insert("accent".into(), value.clone().into());
                }
            }
        }
        self.finish();
        true
    }

    pub fn update_performance_rap_tone(&mut self, value: Option<f32>) -> bool {
        if !NotePerformance::valid_rap_tone(value) {
            return false;
        }
        self.update_performance(PerformanceParameter::RapTone, |note| {
            NotePerformance::write_rap_tone(note, value);
        })
    }

    pub fn update_performance_rap_intonation(&mut self, value: Option<f32>) -> bool {
        if !NotePerformance::valid_rap_intonation(value) {
            return false;
        }
        self.update_performance(PerformanceParameter::RapIntonation, |note| {
            NotePerformance::write_rap_intonation(note, value);
        })
    }
}

pub(super) fn common_accent(notes: &[&Note]) -> Option<String> {
    fn accent(note: &Note) -> &str {
        note.extra
            .get("accent")
            .and_then(|value| value.as_str())
            .unwrap_or("")
    }
    let first = accent(notes[0]);
    notes
        .iter()
        .all(|note| accent(note) == first)
        .then(|| first.to_owned())
}
