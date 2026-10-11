//! Local acoustic edits over an existing complete owner. The dense context is
//! evaluated with the complete note list; only explicitly selected rows and
//! frame results replace the previous snapshot.

mod duration;
mod topology;

use crate::synthesis::{
    engine::{OwnerSynthesis, PreparedAcoustics, PreparedModelFeatures, RenderStage, StageTimings},
    score::PreparedPhrase,
    stages::{
        f0::{F0Stage, PitchInputControls},
        timbre::TimbreNetwork,
    },
    voice::{PreparedVoice, Result, VoiceError},
};
use std::time::Instant;

pub struct RecomputedAcoustics {
    pub state: OwnerSynthesis,
    pub timings: StageTimings,
}

/// The caller supplies separate calculation-record and publication-frame
/// intervals. Both refer to the complete prepared owner, including its context.
pub fn recompute(
    voice: &PreparedVoice,
    duration_identity: u64,
    phrase: &PreparedPhrase,
    base: &OwnerSynthesis,
    edit: &super::IncrementalEdit,
    mut checkpoint: impl FnMut(RenderStage) -> Result<()>,
) -> Result<RecomputedAcoustics> {
    let (base_pitch_features, base_timbre_features) = base.prepared.sv2_features()?;
    let mapping = topology::validate(phrase, &base.prepared.phrase, edit)?;
    checkpoint(RenderStage::Duration)?;
    let started = Instant::now();
    // Both pitch and phoneme identity condition the duration model. Predict
    // the small model again, then reuse unchanged records using explicit maps.
    let duration::Prepared {
        duration,
        phones,
        phone_rows,
    } = duration::prepare(voice, duration_identity, phrase, base, &mapping)?;
    let duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    let work_frames = mapping.record_frames[edit.work_records.start]
        ..mapping.record_frames[edit.work_records.end];
    let total = *mapping
        .record_frames
        .last()
        .expect("validated nonempty records");
    let write_frames = edit.write_frames.clone();
    if write_frames.start >= write_frames.end
        || write_frames.start < work_frames.start
        || write_frames.end > work_frames.end
        || base.pitch.pitch.len() != total
        || base.pitch.predicted_controls.slope.len() != total
        || base.pitch.predicted_controls.offset.len() != total
    {
        return Err(VoiceError::new(
            "Pitch replacement bounds differ from the owner timeline",
        ));
    }
    checkpoint(RenderStage::Pitch)?;
    let started = Instant::now();
    let stage = F0Stage::prepare(voice.model("f0")?)?;
    let fresh = stage.prepare_features(&phrase.notes, &phones)?;
    let pitch_features =
        fresh.reuse_rows_from(base_pitch_features, &mapping.note_rows, &phone_rows)?;
    let controls = crate::synthesis::engine::pitch::sv2_controls(phrase)?;
    let expressiveness = crate::synthesis::engine::pitch::expressiveness(
        phrase,
        crate::synthesis::score::FRAME_SECONDS,
        total,
    )?;
    let pitch_seeds: Vec<_> = phrase
        .retake_seeds
        .iter()
        .map(|seeds| seeds.map(|seeds| seeds.pitch))
        .collect();
    let patch = stage.predict_singing_prepared(
        &phrase.notes,
        &phones,
        &pitch_features,
        PitchInputControls {
            pitch_seeds: Some(&pitch_seeds),
            absolute_pitch: controls.as_deref(),
            expressiveness: expressiveness.as_deref(),
        },
        Some(work_frames.clone()),
    )?;
    if patch.pitch.len() != work_frames.len()
        || patch.predicted_controls.slope.len() != work_frames.len()
        || patch.predicted_controls.offset.len() != work_frames.len()
        || patch.frame_seconds != base.pitch.frame_seconds
    {
        return Err(VoiceError::new(
            "Local pitch output differs from its requested frame interval",
        ));
    }
    let local = write_frames.start - work_frames.start..write_frames.end - work_frames.start;
    let mut pitch = base.pitch.clone();
    pitch.pitch[write_frames.clone()].copy_from_slice(&patch.pitch[local.clone()]);
    pitch.predicted_controls.slope[write_frames.clone()]
        .copy_from_slice(&patch.predicted_controls.slope[local.clone()]);
    pitch.predicted_controls.offset[write_frames]
        .copy_from_slice(&patch.predicted_controls.offset[local]);
    let pitch_ms = started.elapsed().as_secs_f64() * 1000.0;

    checkpoint(RenderStage::Timbre)?;
    let started = Instant::now();
    let stage = TimbreNetwork::prepare(voice.model("timbre")?)?;
    let fresh = stage.prepare_features(&phrase.notes, &phones)?;
    let timbre_features =
        fresh.reuse_rows_from(base_timbre_features, &mapping.note_rows, &phone_rows)?;
    let timbre_ms = started.elapsed().as_secs_f64() * 1000.0;
    Ok(RecomputedAcoustics {
        state: OwnerSynthesis {
            prepared: PreparedAcoustics {
                phrase: phrase.clone(),
                phones,
                features: PreparedModelFeatures::Sv2 {
                    pitch: pitch_features,
                    timbre: timbre_features,
                },
            },
            duration,
            pitch,
            components: base.components.clone(),
        },
        timings: StageTimings {
            duration_ms,
            pitch_ms,
            timbre_ms,
            ..StageTimings::default()
        },
    })
}
