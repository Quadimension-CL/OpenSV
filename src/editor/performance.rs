mod rap;

use super::{Editor, SelectionState};
use crate::model::{EntityId, Note, NoteMode, NotePerformance};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PerformanceParameter {
    Expression,
    Vibrato,
    RapTone,
    RapIntonation,
}

pub struct PerformanceSelection {
    pub mode: Option<NoteMode>,
    pub expression_x: Option<f64>,
    pub expression_y: Option<f64>,
    pub vibrato: Option<f64>,
    /// Outer None represents a mixed selection; inner None means automatic.
    pub r_tone: Option<Option<f32>>,
    pub r_intonation: Option<Option<f32>>,
    /// The empty string selects the automatically determined accent.
    pub accent: Option<String>,
}

pub(super) struct PerformanceEdit {
    parameter: PerformanceParameter,
    selection: SelectionState,
    group: String,
    notes: BTreeSet<EntityId>,
}

impl Editor {
    pub fn performance_selection(&self) -> Result<Option<PerformanceSelection>, String> {
        let notes = self.performance_notes();
        let values = notes
            .iter()
            .map(|note| NotePerformance::from_note(note))
            .collect::<Result<Vec<_>, _>>()?;
        let Some(first) = values.first() else {
            return Ok(None);
        };
        Ok(Some(PerformanceSelection {
            mode: values
                .iter()
                .all(|value| value.mode == first.mode)
                .then_some(first.mode),
            expression_x: common_value(&values, |value| value.expression_x),
            expression_y: common_value(&values, |value| value.expression_y),
            vibrato: common_value(&values, |value| value.vibrato),
            r_tone: common_optional(&values, |value| value.r_tone),
            r_intonation: common_optional(&values, |value| value.r_intonation),
            accent: rap::common_accent(&notes),
        }))
    }

    pub fn set_note_mode(&mut self, mode: NoteMode) -> bool {
        if self.is_editing() {
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
        self.begin("切换音符模式");
        if let Some(group) = self.project.group_mut(&group) {
            for note in &mut group.notes {
                if notes.contains(&note.id) {
                    note.musical_type = mode.id().into();
                }
            }
        }
        self.finish();
        true
    }

    pub fn begin_performance_edit(&mut self, parameter: PerformanceParameter) -> bool {
        if self.is_editing() {
            return false;
        }
        let Some(group) = self
            .active_ref()
            .map(|reference| reference.group_id.clone())
        else {
            return false;
        };
        let notes = self
            .performance_notes()
            .iter()
            .map(|note| note.id)
            .collect::<BTreeSet<_>>();
        if notes.is_empty() || self.performance_selection().is_err() {
            return false;
        }
        self.begin(match parameter {
            PerformanceParameter::Expression => "调整音符演绎",
            PerformanceParameter::Vibrato => "调整颤音调制",
            PerformanceParameter::RapTone => "调整说唱音调",
            PerformanceParameter::RapIntonation => "调整说唱语调",
        });
        self.performance_edit = Some(PerformanceEdit {
            parameter,
            selection: self.selection(),
            group,
            notes,
        });
        true
    }

    pub fn update_performance_expression(&mut self, x: f64, y: f64) -> bool {
        if !NotePerformance::valid_expression(x, y) {
            return false;
        }
        self.update_performance(PerformanceParameter::Expression, |note| {
            NotePerformance::write_expression(note, x, y);
        })
    }

    pub fn update_performance_vibrato(&mut self, value: f64) -> bool {
        if !NotePerformance::valid_vibrato(value) {
            return false;
        }
        self.update_performance(PerformanceParameter::Vibrato, |note| {
            NotePerformance::write_vibrato(note, value);
        })
    }

    pub fn has_performance_edit(&self) -> bool {
        self.performance_edit.is_some()
    }

    pub fn performance_edit_matches_selection(&self) -> bool {
        self.performance_edit.as_ref().is_some_and(|edit| {
            self.is_editing()
                && self.track == edit.selection.track
                && self.reference == edit.selection.reference
                && self.selected == edit.selection.notes
                && self
                    .active_ref()
                    .is_some_and(|reference| reference.group_id == edit.group)
                && self.project.group(&edit.group).is_some_and(|group| {
                    group
                        .notes
                        .iter()
                        .filter(|note| edit.notes.contains(&note.id))
                        .count()
                        == edit.notes.len()
                })
        })
    }

    pub fn finish_performance_edit(&mut self) {
        if self.has_performance_edit() {
            self.finish();
        }
    }

    pub fn cancel_performance_edit(&mut self) {
        if self.has_performance_edit() {
            self.cancel();
        }
    }

    fn performance_notes(&self) -> Vec<&Note> {
        let Some(reference) = self.active_ref() else {
            return Vec::new();
        };
        self.notes()
            .iter()
            .filter(|note| self.selected.contains(&note.id) && reference.note_range(note).is_some())
            .collect()
    }

    fn update_performance(
        &mut self,
        parameter: PerformanceParameter,
        mut update: impl FnMut(&mut Note),
    ) -> bool {
        if !self.performance_edit_matches_selection() {
            self.finish_performance_edit();
            return false;
        }
        let edit = self.performance_edit.as_ref().unwrap();
        if edit.parameter != parameter {
            return false;
        }
        if let Some(group) = self.project.group_mut(&edit.group) {
            for note in &mut group.notes {
                if edit.notes.contains(&note.id) {
                    update(note);
                }
            }
        }
        true
    }
}

fn common_value(
    values: &[NotePerformance],
    value: impl Fn(&NotePerformance) -> f64,
) -> Option<f64> {
    let first = value(&values[0]);
    values
        .iter()
        .all(|item| (value(item) - first).abs() < 1e-6)
        .then_some(first)
}

fn common_optional(
    values: &[NotePerformance],
    value: impl Fn(&NotePerformance) -> Option<f32>,
) -> Option<Option<f32>> {
    let first = value(&values[0]);
    values
        .iter()
        .all(|item| value(item) == first)
        .then_some(first)
}
