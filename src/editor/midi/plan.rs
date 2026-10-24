use super::ImportSummary;
use crate::{
    midi::{MidiFile, MidiNote},
    model::{Note, Project},
};
use std::collections::BTreeSet;

const MAX_TOTAL_TRACKS: usize = 1024;
const TRACK_LIMIT_ERROR: &str =
    "导入后最多支持 1024 条音轨（含重叠音符拆分的声部），请减少所选音轨或拆分 MIDI 文件";

pub(super) struct ImportPlan {
    pub lanes: Vec<Lane>,
    pub summary: ImportSummary,
}

pub(super) struct Lane {
    pub name: String,
    pub start: i64,
    pub end: i64,
    pub notes: Vec<Note>,
}

struct PlannedLane<'a> {
    name: String,
    start: i64,
    end: i64,
    notes: Vec<&'a MidiNote>,
}

impl ImportPlan {
    pub fn prepare(midi: &MidiFile, selected: &[usize], project: &Project) -> Result<Self, String> {
        if selected.is_empty() {
            return Err("请选择至少一条包含音符的 MIDI 音轨".into());
        }
        let budget = MAX_TOTAL_TRACKS
            .checked_sub(project.tracks.len())
            .ok_or(TRACK_LIMIT_ERROR)?;
        let mut names: BTreeSet<_> = project.tracks.iter().map(|t| t.name.clone()).collect();
        let mut planned = Vec::new();
        let mut summary = ImportSummary {
            tracks: 0,
            notes: 0,
            split_tracks: 0,
        };
        for index in selected.iter().copied().collect::<BTreeSet<_>>() {
            let track = midi
                .tracks
                .get(index)
                .ok_or("所选 MIDI 音轨不存在，请重新打开导入窗口")?;
            if track.notes.is_empty() {
                continue;
            }
            let lanes = partition(&track.notes, budget - planned.len())?;
            summary.notes += track.notes.len();
            summary.split_tracks += lanes.len() - 1;
            let polyphonic = lanes.len() > 1;
            let name = if track.name.trim().is_empty() {
                format!("MIDI 音轨 {}", track.source_index + 1)
            } else {
                track.name.trim().to_owned()
            };
            for (index, mut lane) in lanes.into_iter().enumerate() {
                let name = if polyphonic {
                    format!("{name} · 声部 {}", index + 1)
                } else {
                    name.clone()
                };
                lane.name = unique_name(name, &mut names);
                planned.push(lane);
            }
        }
        if planned.is_empty() {
            return Err("所选 MIDI 音轨没有可导入的音符".into());
        }
        summary.tracks = planned.len();

        Ok(Self {
            lanes: planned.into_iter().map(PlannedLane::materialize).collect(),
            summary,
        })
    }
}

fn partition(notes: &[MidiNote], budget: usize) -> Result<Vec<PlannedLane<'_>>, String> {
    if budget == 0 {
        return Err(TRACK_LIMIT_ERROR.into());
    }
    let mut ordered: Vec<_> = notes.iter().collect();
    ordered.sort_by_key(|note| (note.onset, note.pitch, note.duration));
    let mut active = BTreeSet::<(i64, usize)>::new();
    let mut available = BTreeSet::new();
    let mut lanes = Vec::<PlannedLane<'_>>::new();
    for note in ordered {
        let end = note
            .onset
            .checked_add(note.duration)
            .filter(|_| note.onset >= 0 && note.duration > 0 && (0..=127).contains(&note.pitch))
            .ok_or("MIDI 音符的时间或音高超出支持范围")?;
        while active.first().is_some_and(|(end, _)| *end <= note.onset) {
            let (_, index) = active.pop_first().unwrap();
            available.insert(index);
        }
        let index = if let Some(index) = available.pop_first() {
            index
        } else {
            if lanes.len() >= budget {
                return Err(TRACK_LIMIT_ERROR.into());
            }
            lanes.push(PlannedLane {
                name: String::new(),
                start: note.onset,
                end,
                notes: Vec::new(),
            });
            lanes.len() - 1
        };
        let lane = &mut lanes[index];
        lane.notes.push(note);
        lane.end = end;
        active.insert((end, index));
    }
    Ok(lanes)
}

impl PlannedLane<'_> {
    fn materialize(self) -> Lane {
        let notes = self
            .notes
            .into_iter()
            .map(|note| {
                let mut imported = Note::new(note.onset - self.start, note.duration, note.pitch);
                if let Some(lyrics) = &note.lyrics
                    && !lyrics.trim().is_empty()
                {
                    imported.lyrics = lyrics.trim().to_owned();
                }
                imported
            })
            .collect();
        Lane {
            name: self.name,
            start: self.start,
            end: self.end,
            notes,
        }
    }
}

fn unique_name(base: String, names: &mut BTreeSet<String>) -> String {
    if names.insert(base.clone()) {
        return base;
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base} ({suffix})");
        if names.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}
