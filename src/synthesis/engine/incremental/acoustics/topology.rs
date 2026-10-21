use crate::synthesis::{
    engine::incremental::IncrementalEdit,
    score::PreparedPhrase,
    stages::{
        duration::DurationRecord,
        f0::frontend::{FrontendNote, frame_count},
    },
    voice::{Result, VoiceError},
};

/// New rows outside the calculation interval map to immutable baseline rows.
/// Work rows remain None, so changed phone/parts counts need no index identity.
pub(super) struct Mapping {
    pub note_rows: Vec<Option<usize>>,
    pub record_frames: Vec<usize>,
}

pub(super) fn validate(
    current: &PreparedPhrase,
    base: &PreparedPhrase,
    edit: &IncrementalEdit,
) -> Result<Mapping> {
    let work = &edit.work_records;
    let previous = &edit.previous_records;
    if current.notes.is_empty()
        || current.records.len() != current.notes.len()
        || base.records.len() != base.notes.len()
        || current.source_note_ids.len() != current.notes.len()
        || base.source_note_ids.len() != base.notes.len()
        || current.retake_seeds.len() != current.notes.len()
        || base.retake_seeds.len() != base.notes.len()
        || current.start_seconds != base.start_seconds
        || current.visible_begin_seconds != base.visible_begin_seconds
        || current.visible_end_seconds != base.visible_end_seconds
        || work.start >= work.end
        || previous.start >= previous.end
        || work.end > current.notes.len()
        || previous.end > base.notes.len()
        || work.start != previous.start
        || current.notes.len() - work.end != base.notes.len() - previous.end
    {
        return Err(error());
    }
    let record_frames = boundaries(&current.notes)?;
    let old_frames = boundaries(&base.notes)?;
    if record_frames[work.start] != old_frames[previous.start]
        || record_frames[work.end] != old_frames[previous.end]
        || record_frames.last() != old_frames.last()
    {
        return Err(error());
    }
    let mut note_rows = vec![None; current.notes.len()];
    for (index, mapped) in note_rows.iter_mut().enumerate() {
        let old_index = if index < work.start {
            index
        } else if index >= work.end {
            previous.end + (index - work.end)
        } else {
            continue;
        };
        if record_frames[index] != old_frames[old_index]
            || record_frames[index + 1] != old_frames[old_index + 1]
            || current.source_note_ids[index] != base.source_note_ids[old_index]
            || current.retake_seeds[index] != base.retake_seeds[old_index]
            || !same_note(&current.notes[index], &base.notes[old_index])
            || !same_record(&current.records[index], &base.records[old_index])
        {
            return Err(error());
        }
        *mapped = Some(old_index);
    }
    Ok(Mapping {
        note_rows,
        record_frames,
    })
}

fn boundaries(notes: &[FrontendNote]) -> Result<Vec<usize>> {
    let mut offsets = Vec::with_capacity(notes.len() + 1);
    offsets.push(0_usize);
    for note in notes {
        let end = offsets
            .last()
            .expect("initial boundary")
            .checked_add(frame_count(note)?)
            .ok_or_else(|| VoiceError::new("Local acoustic timeline exceeds addressable memory"))?;
        offsets.push(end);
    }
    Ok(offsets)
}

fn same_note(now: &FrontendNote, old: &FrontendNote) -> bool {
    now.phones == old.phones
        && now.pitch == old.pitch
        && now.pitch_offset == old.pitch_offset
        && now.duration_seconds == old.duration_seconds
        && now.frame_seconds == old.frame_seconds
        && now.kind == old.kind
        && now.flag == old.flag
        && now.tag == old.tag
        && now.parts == old.parts
        && now.part_index == old.part_index
        && now.appended_durations == old.appended_durations
        && now.performance == old.performance
}

fn same_record(now: &DurationRecord, old: &DurationRecord) -> bool {
    now.phones.len() == old.phones.len()
        && now.parts == old.parts
        && now.part_index == old.part_index
        && now.linked_durations_seconds == old.linked_durations_seconds
        && now.pitch_offset == old.pitch_offset
        && now.note.sample_rate == old.note.sample_rate
        && now.note.hop_samples == old.note.hop_samples
        && now.note.duration_seconds == old.note.duration_seconds
        && now.note.pitch == old.note.pitch
        && now.phones.iter().zip(&old.phones).all(|(a, b)| {
            a.language == b.language
                && a.token == b.token
                && a.source == b.source
                && a.attributes == b.attributes
                && a.source_note_onset_seconds == b.source_note_onset_seconds
        })
}

fn error() -> VoiceError {
    VoiceError::new(
        "Local acoustic reuse requires equal frame boundaries and unchanged outside records",
    )
}
