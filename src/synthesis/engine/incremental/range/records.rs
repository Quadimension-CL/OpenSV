use crate::synthesis::{
    plan::NoteInput,
    score::PreparedPhrase,
    stages::f0::frontend::frame_count,
    voice::{Result, VoiceError},
};
use std::{collections::BTreeSet, ops::Range};

pub(super) struct RecordLayout {
    sources: Vec<Range<usize>>,
    boundaries: Vec<usize>,
}

impl RecordLayout {
    pub fn new(phrase: &PreparedPhrase, sources: &[NoteInput]) -> Result<Self> {
        if phrase.notes.len() != phrase.source_note_ids.len()
            || phrase.notes.len() != phrase.records.len()
            || sources.is_empty()
        {
            return Err(invalid());
        }
        let mut boundaries = vec![0_usize];
        for note in &phrase.notes {
            boundaries.push(
                boundaries
                    .last()
                    .copied()
                    .unwrap()
                    .checked_add(frame_count(note)?)
                    .ok_or_else(invalid)?,
            );
        }
        let ids = &phrase.source_note_ids;
        let mut cursor = ids.iter().take_while(|id| id.is_none()).count();
        let mut groups = Vec::with_capacity(sources.len());
        let mut seen = BTreeSet::new();
        for source in sources {
            if !seen.insert(source.id) || ids.get(cursor) != Some(&Some(source.id)) {
                return Err(invalid());
            }
            let start = cursor;
            while ids.get(cursor) == Some(&Some(source.id)) {
                cursor += 1;
            }
            let count = cursor - start;
            let first = &phrase.notes[start];
            if phrase.notes[start..cursor]
                .iter()
                .enumerate()
                .any(|(part, note)| {
                    note.kind != 0
                        || note.parts as usize != count
                        || note.part_index as usize != part
                        || note.duration_seconds != first.duration_seconds
                        || note.pitch != first.pitch
                        || note.pitch_offset != first.pitch_offset
                })
            {
                return Err(invalid());
            }
            groups.push(start..cursor);
        }
        if ids[cursor..].iter().any(Option::is_some) {
            return Err(invalid());
        }
        Ok(Self {
            sources: groups,
            boundaries,
        })
    }

    /// Include generated onset/tail records when the source region touches an
    /// edge: their phones and inherited pitch can also change after an edit.
    pub fn records(&self, sources: Range<usize>) -> Range<usize> {
        let first = if sources.start == 0 {
            0
        } else {
            self.sources[sources.start].start
        };
        let end = if sources.end == self.sources.len() {
            self.boundaries.len() - 1
        } else {
            self.sources[sources.end - 1].end
        };
        first..end
    }

    pub fn frames(&self, records: &Range<usize>) -> Range<usize> {
        self.boundaries[records.start]..self.boundaries[records.end]
    }

    pub fn total_frames(&self) -> usize {
        *self.boundaries.last().unwrap()
    }
}

fn invalid() -> VoiceError {
    VoiceError::new("Prepared records do not match their source note identities and partitions")
}
