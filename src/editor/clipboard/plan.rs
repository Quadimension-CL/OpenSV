use super::{Editor, NoteClipboard};
use crate::model::{EntityId, Group, GroupRef, Note};

pub(super) struct PastePlan {
    track: EntityId,
    target: Target,
    notes: Vec<Note>,
}

enum Target {
    Existing {
        reference: EntityId,
        group: String,
        end: Option<i64>,
    },
    New(Box<(Group, GroupRef)>),
}

impl PastePlan {
    pub(super) fn prepare(
        editor: &Editor,
        clipboard: &NoteClipboard,
        at: i64,
        duplicate: bool,
    ) -> Result<Self, String> {
        clipboard.validate()?;
        if at < 0 {
            return Err("粘贴位置不能早于项目起点。".into());
        }
        let track = editor.current_track();
        if track.is_instrumental() {
            return Err("请先选择歌声音轨再粘贴音符。".into());
        }
        let reference = if duplicate {
            editor.active_ref()
        } else {
            editor.group_at(at).and_then(|id| {
                track
                    .visible_references()
                    .find(|reference| reference.id == id)
            })
        };
        let (offset, pitch_offset) = reference
            .map(|reference| (reference.blick_offset, reference.pitch_offset))
            .unwrap_or((at, 0));
        let mut notes = clipboard.notes.clone();
        let mut end = at;
        for note in &mut notes {
            let global = at
                .checked_add(note.onset)
                .ok_or("粘贴后的音符时间超出可用范围。")?;
            let global_end = global
                .checked_add(note.duration)
                .ok_or("粘贴后的音符结束时间超出可用范围。")?;
            note.onset = global
                .checked_sub(offset)
                .ok_or("粘贴后的组内时间超出可用范围。")?;
            note.onset
                .checked_add(note.duration)
                .ok_or("粘贴后的组内结束时间超出可用范围。")?;
            note.pitch = note
                .pitch
                .checked_sub(pitch_offset)
                .filter(|pitch| (0..=127).contains(pitch))
                .ok_or("目标音符组的移调使粘贴音高超出 MIDI 范围。")?;
            note.renew_identity();
            end = end.max(global_end);
        }
        let target = if let Some(reference) = reference {
            editor
                .project
                .group(&reference.group_id)
                .ok_or("目标音符组不存在，无法粘贴。")?;
            Target::Existing {
                reference: reference.id,
                group: reference.group_id.clone(),
                end: (reference.blick_absolute_end >= 0)
                    .then_some(reference.blick_absolute_end.max(end)),
            }
        } else {
            let group = Group::new(&track.name);
            let mut reference = track.main_ref.clone();
            reference.renew_identity();
            reference.group_id = group.uuid.clone();
            reference.blick_absolute_begin = at;
            reference.blick_absolute_end = end;
            reference.blick_offset = at;
            reference.pitch_offset = 0;
            Target::New(Box::new((group, reference)))
        };
        Ok(Self {
            track: track.id,
            target,
            notes,
        })
    }

    pub(super) fn apply(self, editor: &mut Editor) -> usize {
        let count = self.notes.len();
        editor.selected = self.notes.iter().map(|note| note.id).collect();
        editor.track = self.track;
        match self.target {
            Target::Existing {
                reference,
                group,
                end,
            } => {
                editor
                    .project
                    .group_mut(&group)
                    .unwrap()
                    .notes
                    .extend(self.notes);
                let track = editor
                    .project
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == self.track)
                    .unwrap();
                if let Some(end) = end {
                    let target = std::iter::once(&mut track.main_ref)
                        .chain(&mut track.groups)
                        .find(|target| target.id == reference)
                        .unwrap();
                    target.blick_absolute_end = end;
                }
                editor.reference = Some(reference);
            }
            Target::New(target) => {
                let (mut group, reference) = *target;
                group.notes = self.notes;
                editor.reference = Some(reference.id);
                editor.project.library.push(group);
                editor
                    .project
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == self.track)
                    .unwrap()
                    .groups
                    .push(reference);
            }
        }
        count
    }
}
