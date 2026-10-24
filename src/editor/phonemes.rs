use super::Editor;
use crate::{model::EntityId, synthesis::score::PhoneSource};
use serde_json::{Map, Value};
impl Editor {
    pub fn set_phoneme_attribute(
        &mut self,
        group_id: &str,
        source: PhoneSource,
        key: &str,
        value: Option<f32>,
    ) -> Result<(), String> {
        if !matches!(key, "leftOffset" | "strength" | "activity" | "position")
            || value.is_some_and(|v| !v.is_finite())
        {
            return Err("无效音素属性".into());
        }
        let note = self
            .project
            .group_mut(group_id)
            .and_then(|g| g.notes.iter_mut().find(|n| n.id == source.note_id))
            .ok_or("音符已改变")?;
        let rows = note
            .attributes
            .entry("phonemes".into())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or("音素属性必须为数组")?;
        if source.phone_index > 4096 {
            return Err("音素序号无效".into());
        }
        rows.resize_with(rows.len().max(source.phone_index + 1), || {

            let mut row = Map::new();
            row.insert("strength".into(), Value::from(0.0));
            Value::Object(row)
        });
        let row = rows[source.phone_index]
            .as_object_mut()
            .ok_or("音素属性必须为对象")?;
        if let Some(value) = value {
            row.insert(key.into(), Value::from(value));
        } else {
            row.remove(key);
        }

        if rows.is_empty() {
            note.attributes.remove("phonemes");
        }
        Ok(())
    }
    pub fn phoneme_note(&self, note: EntityId) -> Option<&crate::model::Note> {
        self.notes().iter().find(|n| n.id == note)
    }
}
