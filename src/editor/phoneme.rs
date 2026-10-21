use super::{Editor, SelectionState};
use crate::model::{Note, PhonemeAttributes, PhonemeParameter, SourcePhone};

pub(super) struct PhonemeEdit {
    selection: SelectionState,
    group: String,
}

impl Editor {
    pub fn phoneme_attributes(&self, source: SourcePhone) -> Result<PhonemeAttributes, String> {
        let note = self
            .phoneme_note(source)
            .ok_or("当前音符组中没有此音素的源音符")?;
        PhonemeAttributes::get_at(&note.attributes, source.phone_index)
    }

    pub fn begin_phoneme_edit(&mut self, source: SourcePhone) -> bool {
        if self.is_editing() || self.phoneme_attributes(source).is_err() {
            return false;
        }
        let Some(group) = self
            .active_ref()
            .map(|reference| reference.group_id.clone())
        else {
            return false;
        };
        self.begin("调整音素时值");
        self.phoneme_edit = Some(PhonemeEdit {
            selection: self.selection(),
            group,
        });
        true
    }

    /// The neighbouring boundary can belong to another source note in this group.
    pub fn update_phoneme_attributes(
        &mut self,
        source: SourcePhone,
        attributes: PhonemeAttributes,
    ) -> bool {
        if !self.phoneme_edit_matches_selection() {
            self.finish_phoneme_edit();
            return false;
        }
        if attributes.validate().is_err() || self.phoneme_note(source).is_none() {
            return false;
        }
        let Some(note) = self.group_mut().and_then(|group| {
            group
                .notes
                .iter_mut()
                .find(|note| note.id == source.note_id)
        }) else {
            return false;
        };
        PhonemeAttributes::set_at(&mut note.attributes, source.phone_index, attributes).is_ok()
    }

    pub fn reset_phoneme_attributes(
        &mut self,
        source: SourcePhone,
        parameter: Option<PhonemeParameter>,
    ) -> Result<(), String> {
        if self.is_editing() {
            return Err("请先完成当前编辑".into());
        }
        let mut attributes = self.phoneme_attributes(source)?;
        if let Some(parameter) = parameter {
            attributes.set(parameter, None);
        } else {
            attributes = PhonemeAttributes::default();
        }
        if !self.begin_phoneme_edit(source) {
            return Err("无法编辑当前音素".into());
        }
        if self.update_phoneme_attributes(source, attributes) {
            self.finish_phoneme_edit();
            Ok(())
        } else {
            self.cancel_phoneme_edit();
            Err("无法重置当前音素".into())
        }
    }

    pub fn has_phoneme_edit(&self) -> bool {
        self.phoneme_edit.is_some()
    }

    pub fn phoneme_edit_matches_selection(&self) -> bool {
        self.phoneme_edit.as_ref().is_some_and(|edit| {
            self.is_editing()
                && self.track == edit.selection.track
                && self.reference == edit.selection.reference
                && self.selected == edit.selection.notes
                && self
                    .active_ref()
                    .is_some_and(|reference| reference.group_id == edit.group)
        })
    }

    pub fn finish_phoneme_edit(&mut self) {
        if self.has_phoneme_edit() {
            self.finish();
        }
    }

    pub fn cancel_phoneme_edit(&mut self) {
        if self.has_phoneme_edit() {
            self.cancel();
        }
    }

    fn phoneme_note(&self, source: SourcePhone) -> Option<&Note> {
        let reference = self.active_ref()?;
        self.notes()
            .iter()
            .find(|note| note.id == source.note_id && reference.note_range(note).is_some())
    }
}
