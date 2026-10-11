use super::{
    OwnerSynthesis, PcmComponents, RenderStage, StageTimings, models, synthesize_acoustics,
};
use crate::synthesis::{
    audio::RenderedAudio,
    rendered::{RenderVisuals, Waveform},
    score::PreparedPhrase,
    stages::vocoder::VocoderStage,
    voice::{PreparedVoice, Result},
};
use std::time::Instant;

pub struct RenderResult {
    pub audio: RenderedAudio,
    pub visuals: RenderVisuals,
    pub timings: StageTimings,
    pub state: OwnerSynthesis,
}

/// Render a complete owner context, then restrict publication to its visible
/// interval. Hidden notes remain available to all neural stages.
pub fn synthesize(
    voice: &PreparedVoice,
    duration_identity: Option<u64>,
    phrase: &PreparedPhrase,
    mut checkpoint: impl FnMut(RenderStage) -> Result<()>,
) -> Result<RenderResult> {
    let acoustic = synthesize_acoustics(voice, duration_identity, phrase, &mut checkpoint)?;
    checkpoint(RenderStage::Vocoder)?;
    let started = Instant::now();
    #[cfg(feature = "synth-profile")]
    let profile = crate::synthesis::profiling::span("stage.vocoder");
    let mut vocoder = VocoderStage::prepare(voice.model("vocoder")?)?;
    let output = vocoder.render_components(
        &acoustic.timbre.acoustics,
        acoustic.timbre.controls.as_ref(),
    )?;
    let mut timings = acoustic.timings;
    timings.vocoder_ms = started.elapsed().as_secs_f64() * 1000.0;
    #[cfg(feature = "synth-profile")]
    drop(profile);
    #[cfg(feature = "synth-profile")]
    let profile = crate::synthesis::profiling::span("stage.pcm_publication");
    let origin = phrase.start_seconds
        + acoustic.timbre.first_frame as f64 * crate::synthesis::score::FRAME_SECONDS;
    let components = PcmComponents::new(origin, output)?;
    let audio = components.publish(phrase.visible_begin_seconds, phrase.visible_end_seconds)?;
    #[cfg(feature = "synth-profile")]
    drop(profile);
    let started = Instant::now();
    #[cfg(feature = "synth-profile")]
    let _profile = crate::synthesis::profiling::span("stage.visuals");
    let pitch = models::pitch_track(voice, &acoustic.prepared.phrase, acoustic.pitch.clone())?;
    let visuals = RenderVisuals {
        pitch,
        waveform: Waveform::new(&audio),
    };
    timings.visuals_ms = started.elapsed().as_secs_f64() * 1000.0;
    Ok(RenderResult {
        audio,
        visuals,
        timings,
        state: OwnerSynthesis {
            prepared: acoustic.prepared,
            duration: acoustic.duration,
            pitch: acoustic.pitch,
            components,
        },
    })
}
