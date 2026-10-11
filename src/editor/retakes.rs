use super::Editor;
use crate::model::{EntityId, RetakeComponent, TakeSet};
use std::collections::{BTreeMap, BTreeSet};

pub struct RetakeChoice {
    pub id: u32,
}

pub struct RetakeSelection {
    pub note_count: usize,
    pub active_take_id: Option<u32>,
    pub takes: Vec<RetakeChoice>,
    pub can_prune: bool,
    pub previous_take_id: Option<u32>,
    pub next_take_id: Option<u32>,
}

struct SelectedTakes {
    group: String,
    sets: Vec<(EntityId, TakeSet)>,
}

impl RetakeSelection {
    pub fn from_set(set: &TakeSet) -> Self {
        Self::from_sets(&[set])
    }

    pub fn adjacent_id(&self, direction: i32) -> Option<u32> {
        match direction.cmp(&0) {
            std::cmp::Ordering::Less => self.previous_take_id,
            std::cmp::Ordering::Equal => self.active_take_id,
            std::cmp::Ordering::Greater => self.next_take_id,
        }
    }

    fn from_sets(sets: &[&TakeSet]) -> Self {
        let first = sets[0];
        let common_ids = sets.iter().skip(1).fold(
            first
                .takes
                .iter()
                .map(|take| take.id)
                .collect::<BTreeSet<_>>(),
            |ids, set| {
                ids.intersection(&set.takes.iter().map(|take| take.id).collect())
                    .copied()
                    .collect()
            },
        );
        let min_active = sets.iter().map(|set| set.active_take_id).min().unwrap();
        let max_active = sets.iter().map(|set| set.active_take_id).max().unwrap();
        let previous_take_id = common_ids.range(..max_active).next_back().copied();
        let next_take_id = common_ids
            .range((
                std::ops::Bound::Excluded(min_active),
                std::ops::Bound::Unbounded,
            ))
            .next()
            .copied();
        let takes = common_ids
            .into_iter()
            .map(|id| RetakeChoice { id })
            .collect();
        Self {
            note_count: sets.len(),
            active_take_id: sets
                .iter()
                .all(|set| set.active_take_id == first.active_take_id)
                .then_some(first.active_take_id),
            takes,
            can_prune: sets.iter().any(|set| {
                set.takes
                    .iter()
                    .any(|take| take.id != 0 && take.id != set.active_take_id)
            }),
            previous_take_id,
            next_take_id,
        }
    }
}

impl Editor {
    pub fn retake_selection(&self) -> Result<Option<RetakeSelection>, String> {
        let Some(SelectedTakes { sets, .. }) = self.selected_take_sets()? else {
            return Ok(None);
        };
        Ok(Some(RetakeSelection::from_sets(
            &sets.iter().map(|(_, set)| set).collect::<Vec<_>>(),
        )))
    }

    pub fn create_retake(&mut self, component: RetakeComponent) -> Result<usize, String> {
        let sets = self.selected_take_sets()?;
        self.create_retake_from_sets(sets, component)
    }

    pub fn create_note_retake(
        &mut self,
        group_id: &str,
        note_id: EntityId,
        component: RetakeComponent,
    ) -> Result<usize, String> {
        let sets = self.note_take_set(group_id, note_id)?;
        self.create_retake_from_sets(sets, component)
    }

    fn create_retake_from_sets(
        &mut self,
        sets: Option<SelectedTakes>,
        component: RetakeComponent,
    ) -> Result<usize, String> {
        let Some(SelectedTakes { group, mut sets }) = sets else {
            return Ok(0);
        };
        let id = sets
            .iter()
            .flat_map(|(_, set)| set.takes.iter().map(|take| take.id))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("重录采样编号已用尽")?;
        for (_, set) in &mut sets {
            set.append(id, component)?;
        }
        let label = match component {
            RetakeComponent::All => "重录新的采样",
            RetakeComponent::PitchAndTimbre => "重录音高和音色采样",
            RetakeComponent::Duration => "重录时值采样",
            RetakeComponent::Pitch => "重录音高采样",
            RetakeComponent::Timbre => "重录音色采样",
        };
        Ok(self.write_take_sets(label, &group, sets))
    }

    pub fn select_retake(&mut self, id: u32) -> Result<usize, String> {
        self.edit_retakes("切换重录采样", |set| set.select(id))
    }

    pub fn select_note_retake(
        &mut self,
        group_id: &str,
        note_id: EntityId,
        id: u32,
    ) -> Result<usize, String> {
        let sets = self.note_take_set(group_id, note_id)?;
        self.edit_take_sets("切换重录采样", sets, |set| set.select(id))
    }

    pub fn step_retake(&mut self, direction: i32) -> Result<usize, String> {
        let Some(id) = self
            .retake_selection()?
            .and_then(|selection| selection.adjacent_id(direction))
        else {
            return Ok(0);
        };
        self.select_retake(id)
    }

    pub fn prune_retakes(&mut self) -> Result<usize, String> {
        self.edit_retakes("裁剪保留使用采样", |set| {
            set.retain_current();
            Ok(())
        })
    }

    pub fn prune_note_retakes(
        &mut self,
        group_id: &str,
        note_id: EntityId,
    ) -> Result<usize, String> {
        let sets = self.note_take_set(group_id, note_id)?;
        self.edit_take_sets("裁剪保留使用采样", sets, |set| {
            set.retain_current();
            Ok(())
        })
    }

    fn edit_retakes(
        &mut self,
        label: &str,
        edit: impl FnMut(&mut TakeSet) -> Result<(), String>,
    ) -> Result<usize, String> {
        let sets = self.selected_take_sets()?;
        self.edit_take_sets(label, sets, edit)
    }

    fn edit_take_sets(
        &mut self,
        label: &str,
        sets: Option<SelectedTakes>,
        mut edit: impl FnMut(&mut TakeSet) -> Result<(), String>,
    ) -> Result<usize, String> {
        let Some(SelectedTakes { group, mut sets }) = sets else {
            return Ok(0);
        };
        for (_, set) in &mut sets {
            edit(set)?;
        }
        Ok(self.write_take_sets(label, &group, sets))
    }

    fn note_take_set(
        &self,
        group_id: &str,
        note_id: EntityId,
    ) -> Result<Option<SelectedTakes>, String> {
        let Some(note) = self
            .project
            .group(group_id)
            .and_then(|group| group.notes.iter().find(|note| note.id == note_id))
        else {
            return Ok(None);
        };
        Ok(Some(SelectedTakes {
            group: group_id.into(),
            sets: vec![(note_id, TakeSet::from_extra(&note.extra)?)],
        }))
    }

    fn selected_take_sets(&self) -> Result<Option<SelectedTakes>, String> {
        let Some(reference) = self.active_ref() else {
            return Ok(None);
        };
        let mut notes = self
            .notes()
            .iter()
            .filter(|note| self.selected.contains(&note.id) && reference.note_range(note).is_some())
            .collect::<Vec<_>>();
        notes.sort_by_key(|note| (note.onset, note.id));
        if notes.is_empty() {
            return Ok(None);
        }
        let sets = notes
            .iter()
            .map(|note| Ok((note.id, TakeSet::from_extra(&note.extra)?)))
            .collect::<Result<_, String>>()?;
        Ok(Some(SelectedTakes {
            group: reference.group_id.clone(),
            sets,
        }))
    }

    fn write_take_sets(
        &mut self,
        label: &str,
        group_id: &str,
        sets: Vec<(EntityId, TakeSet)>,
    ) -> usize {
        let values: BTreeMap<_, _> = sets
            .into_iter()
            .map(|(id, set)| (id, set.to_value()))
            .collect();
        self.begin(label);
        let mut changed = 0;
        if let Some(group) = self.project.group_mut(group_id) {
            for note in &mut group.notes {
                if let Some(value) = values.get(&note.id)
                    && note.extra.get("takes") != Some(value)
                {
                    note.extra.insert("takes".into(), value.clone());
                    changed += 1;
                }
            }
        }
        self.finish();
        changed
    }
}
