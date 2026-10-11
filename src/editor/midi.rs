mod plan;

use super::Editor;
use crate::{
    midi::MidiFile,
    model::{Group, GroupRef, Project, Track},
};
use plan::ImportPlan;

#[derive(Clone, Copy, Debug)]
pub struct ImportSummary {
    /// Total number of vocal tracks created, including polyphonic lanes.
    pub tracks: usize,
    pub notes: usize,
    /// Additional tracks needed to preserve overlapping notes.
    pub split_tracks: usize,
}

impl Editor {
    /// Adds the chosen MIDI entries at their original beat positions. The
    /// current project's tempo and meter are deliberately left unchanged.
    pub fn import_midi_tracks(
        &mut self,
        midi: &MidiFile,
        selected: &[usize],
    ) -> Result<ImportSummary, String> {
        let plan = ImportPlan::prepare(midi, selected, &self.project)?;
        self.finish();
        self.begin("导入 MIDI 音轨");
        let summary = self.append_midi(plan);
        self.finish();
        Ok(summary)
    }

    /// A full import starts a new unsaved project, including the MIDI time
    /// map. Undo returns to a valid blank project rather than zero tracks.
    pub fn from_midi(midi: &MidiFile, selected: &[usize]) -> Result<(Self, ImportSummary), String> {
        let mut blank = Project::default();
        blank.tracks.clear();
        let plan = ImportPlan::prepare(midi, selected, &blank)?;
        let mut editor = Self::new(Project::default());
        editor.begin("导入 MIDI");
        editor.project.tracks.clear();
        editor.project.time = midi.time.clone();
        let summary = editor.append_midi(plan);
        editor.finish();
        Ok((editor, summary))
    }

    fn append_midi(&mut self, plan: ImportPlan) -> ImportSummary {
        let mut first = None;
        for lane in plan.lanes {
            let mut track = Track::new(self.project.tracks.len());
            track.name = lane.name;
            track.disp_color = self.project.next_track_color();
            let mut group = Group::new(&track.name);
            group.notes = lane.notes;
            let mut reference = GroupRef::new(&group, lane.end);
            reference.blick_absolute_begin = lane.start;
            reference.blick_offset = lane.start;
            first.get_or_insert((track.id, reference.id));
            track.groups.push(reference);
            self.project.library.push(group);
            self.project.tracks.push(track);
        }
        let (track, reference) = first.expect("import plan has at least one nonempty lane");
        self.track = track;
        self.reference = Some(reference);
        self.selected.clear();
        plan.summary
    }
}
