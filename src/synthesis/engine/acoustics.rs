mod conditioning;
mod prepare;

pub use prepare::prepare_acoustics;

use super::{
    PreparedAcoustics, PreparedModelFeatures, RenderStage, models, voice_controls::AcousticControls,
};
use crate::synthesis::{
    score::PreparedPhrase,
    stages::{
        duration::DurationResult,
        f0::PitchResult,
        timbre::{TimbreNetwork, TimbreOutput, TimbreRequest, records::RecordsConfig},
    },
    voice::{PreparedVoice, Result, VoiceError, VoiceFamily},
};
use serde::Serialize;
use std::time::Instant;

#[derive(Clone, Debug, Default, Serialize)]
/// Accumulated stage wall times. Progressive timbre/vocoder execution may
/// overlap on separate threads, so these fields must not be summed to obtain
/// render latency; measure the complete call separately.
pub struct StageTimings {
    pub duration_ms: f64,
    pub pitch_ms: f64,
    pub timbre_ms: f64,
    pub vocoder_ms: f64,
    pub visuals_ms: f64,
}

/// Complete contextual conditions shared by full and bounded acoustic renders.
/// No frame-level timbre network has run yet.
pub struct PreparedRenderAcoustics {
    pub prepared: PreparedAcoustics,
    pub duration: DurationResult,
    pub pitch: PitchResult,
    pub timings: StageTimings,
}

pub struct AcousticRender {
    pub duration: DurationResult,
    pub pitch: PitchResult,
    pub timbre: TimbreOutput,
    pub timings: StageTimings,
    pub prepared: PreparedAcoustics,
}

/// `duration_identity` is explicit model conditioning, distinct from both the
/// archive content hash and the deterministic retake seeds. Callers must supply
/// the intended session identity for SV2 containers. Native SV1 supplies None.
pub fn synthesize_acoustics(
    voice: &PreparedVoice,
    duration_identity: Option<u64>,
    phrase: &PreparedPhrase,
    checkpoint: impl FnMut(RenderStage) -> Result<()>,
) -> Result<AcousticRender> {
    let mut render = prepare_acoustics(voice, duration_identity, phrase, checkpoint)?;
    let started = Instant::now();
    #[cfg(feature = "synth-profile")]
    let _profile = crate::synthesis::profiling::span("stage.timbre");
    let modulation = render.modulation();
    let impulses = render.impulses();
    let request = TimbreRequest {
        notes: &render.prepared.phrase.notes,
        phones: &render.prepared.phones,
        midi_pitch: &render.pitch.pitch,
        note_modulation: &modulation,
        phone_impulses: (voice.family == VoiceFamily::Sv2).then_some(impulses.as_slice()),
        frame_controls: None,
        primary_style_weights: None,
        secondary_style_weights: None,
        requested_frame: 0,
    };
    let timbre = match &render.prepared.features {
        PreparedModelFeatures::Sv1Local => {
            let stage = models::dds_timbre(voice)?;
            let controls = AcousticControls::sample(
                &render.prepared.phrase,
                &voice.style_names,
                stage.frame_seconds(),
                render
                    .prepared
                    .phones
                    .iter()
                    .map(|phone| phone.frames)
                    .sum(),
            )?;
            stage.render(
                &TimbreRequest {
                    frame_controls: controls.frames.as_deref(),
                    primary_style_weights: controls.pronunciation.as_ref(),
                    secondary_style_weights: controls.timbre.as_ref(),
                    ..request
                },
                &render.pitch,
            )?
        }
        PreparedModelFeatures::Sv2 { timbre, .. } => {
            let controls = AcousticControls::sample(
                &render.prepared.phrase,
                &voice.style_names,
                crate::synthesis::score::FRAME_SECONDS,
                render.pitch.pitch.len(),
            )?;
            TimbreNetwork::prepare(voice.model("timbre")?)?.render_prepared(
                &TimbreRequest {
                    frame_controls: controls.frames.as_deref(),
                    primary_style_weights: controls.pronunciation.as_ref(),
                    secondary_style_weights: controls.timbre.as_ref(),
                    ..request
                },
                &RecordsConfig::default(),
                timbre,
            )?
        }
    };
    if timbre
        .acoustics
        .values
        .iter()
        .chain(timbre.controls.iter().flat_map(|controls| &controls.values))
        .any(|value| !value.is_finite())
    {
        return Err(VoiceError::new(
            "Acoustic synthesis returned a nonfinite value",
        ));
    }
    render.timings.timbre_ms += started.elapsed().as_secs_f64() * 1000.0;
    Ok(AcousticRender {
        duration: render.duration,
        pitch: render.pitch,
        timbre,
        timings: render.timings,
        prepared: render.prepared,
    })
}
