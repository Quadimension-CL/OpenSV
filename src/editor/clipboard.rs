mod plan;

use super::Editor;
use crate::model::{Note, NotePerformance, TakeSet};
use plan::PastePlan;
use serde::{Deserialize, Serialize};

const PREFIX: &str = "RynthV notes\n";
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_NOTES: usize = 100_000;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteClipboard {
    notes: Vec<Note>,
}

impl NoteClipboard {
    fn validate(&self) -> Result<(), String> {
        if self.notes.is_empty() || self.notes.len() > MAX_NOTES {
            return Err(format!("音符剪贴板须包含 1 到 {MAX_NOTES} 个音符。"));
        }
        if self.notes.iter().map(|note| note.onset).min() != Some(0) {
            return Err("音符剪贴板的起始时间无效。".into());
        }
        for note in &self.notes {
            if note.onset < 0 || note.duration <= 0 {
                return Err("音符剪贴板包含无效的起始时间或时长。".into());
            }
            if !(0..=127).contains(&note.pitch) {
                return Err("音符剪贴板的音高超出 MIDI 范围。".into());
            }
            note.onset
                .checked_add(note.duration)
                .ok_or("音符剪贴板的时间超出可用范围。")?;
            NotePerformance::from_note(note)
                .map_err(|error| format!("音符剪贴板的演绎参数无效：{error}"))?;
            TakeSet::from_extra(&note.extra)
                .map_err(|error| format!("音符剪贴板的重录采样无效：{error}"))?;
        }
        Ok(())
    }

    fn to_text(&self) -> Result<String, String> {
        let json = serde_json::to_string(self).map_err(|error| format!("无法复制音符：{error}"))?;
        if json.len() > MAX_BYTES - PREFIX.len() {
            return Err("所选音符数据过大，无法复制。".into());
        }
        Ok(format!("{PREFIX}{json}"))
    }

    fn from_text(text: &str) -> Result<Self, String> {
        if text.len() > MAX_BYTES {
            return Err("剪贴板数据过大，无法粘贴音符。".into());
        }
        let json = text
            .strip_prefix(PREFIX)
            .ok_or("剪贴板中没有 RynthV 音符，请先复制音符。")?;
        let clipboard: Self =
            serde_json::from_str(json).map_err(|error| format!("音符剪贴板数据无效：{error}"))?;
        clipboard.validate()?;
        Ok(clipboard)
    }
}

impl Editor {

    pub fn copy(&self) -> Result<Option<String>, String> {
        let Some(clipboard) = self.capture_note_selection()? else {
            return Ok(None);
        };
        clipboard.to_text().map(Some)
    }

    pub fn paste_text(&mut self, text: &str, at: i64) -> Result<usize, String> {
        let clipboard = NoteClipboard::from_text(text)?;
        let plan = PastePlan::prepare(self, &clipboard, at, false)?;
        Ok(self.apply_note_paste(plan, "粘贴音符"))
    }

    pub fn duplicate_selected(&mut self) -> Result<usize, String> {
        let Some(clipboard) = self.capture_note_selection()? else {
            return Ok(0);
        };
        let offset = self.active_ref().ok_or("请先选择音符组。")?.blick_offset;
        let at = self
            .notes()
            .iter()
            .filter(|note| self.selected.contains(&note.id))
            .map(|note| {
                note.onset
                    .checked_add(note.duration)
                    .and_then(|end| end.checked_add(offset))
                    .ok_or("所选音符的结束时间超出可用范围。")
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .ok_or("请先选择音符。")?;
        let plan = PastePlan::prepare(self, &clipboard, at, true)?;
        Ok(self.apply_note_paste(plan, "重复音符"))
    }

    fn capture_note_selection(&self) -> Result<Option<NoteClipboard>, String> {
        let Some(reference) = self.active_ref() else {
            return Ok(None);
        };
        let mut notes: Vec<_> = self
            .notes()
            .iter()
            .filter(|note| self.selected.contains(&note.id))
            .cloned()
            .collect();
        let Some(first) = notes.iter().map(|note| note.onset).min() else {
            return Ok(None);
        };
        for note in &mut notes {
            note.onset = note
                .onset
                .checked_sub(first)
                .ok_or("所选音符的时间跨度超出可用范围。")?;
            note.pitch = note
                .pitch
                .checked_add(reference.pitch_offset)
                .ok_or("所选音符的音高超出可用范围。")?;
        }
        notes.sort_by_key(|note| (note.onset, note.pitch, note.id));
        let clipboard = NoteClipboard { notes };
        clipboard.validate()?;
        Ok(Some(clipboard))
    }

    fn apply_note_paste(&mut self, plan: PastePlan, label: &str) -> usize {
        self.finish();
        self.begin(label);
        let count = plan.apply(self);
        self.finish();
        count
    }
}
