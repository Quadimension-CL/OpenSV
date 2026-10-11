//! Demand-driven PCM production over one immutable prepared owner revision.
//!
//! Neural execution cursors may be replaced after a seek. Prepared duration,
//! pitch/features and already published PCM remain owned by this revision.
mod cursor;
mod network;
mod pcm;
mod pipeline;
mod prepare;

pub use cursor::Cursor;
pub use network::Network;
pub use prepare::prepare;

use super::{OwnerSynthesis, PreparedRenderAcoustics, RenderResult};
use crate::synthesis::{
    audio::RenderedAudio,
    rendered::{PitchTrack, RenderVisuals, Waveform},
    voice::Result,
};
use network::PreparedFrames;
use pcm::PcmStore;
use std::{ops::Range, sync::OnceLock, time::Instant};

pub struct PreparedRender {
    acoustic: PreparedRenderAcoustics,
    pcm: PcmStore,
    pitch: PitchTrack,
    timbre_frames: OnceLock<PreparedFrames>,
}

impl PreparedRender {
    /// Immutable pitch geometry for progressive display. The worker retains
    /// this once and shares its values across ready PCM interval snapshots.
    pub fn pitch_track(&self) -> Result<PitchTrack> {
        Ok(self.pitch.clone())
    }

    /// Existing unchanged regions are available before any new cursor runs.
    /// Publication still applies the owner instance's visible coverage.
    pub fn ready_audio(&self) -> Result<Vec<RenderedAudio>> {
        self.pcm.ready_audio()
    }

    /// Exact native model clock. Playback intersects its own integer coverage
    /// with this range to distinguish outside silence from pending PCM.
    pub fn model_samples(&self) -> Range<i64> {
        self.pcm.model_samples()
    }

    pub fn needs_audio(&self, seconds: f64) -> bool {
        self.pcm.needs(seconds)
    }

    pub fn is_complete(&self) -> bool {
        self.pcm.is_complete()
    }

    /// Only a complete owner creates a history snapshot. Progressive display
    /// patches never become synthesis baselines for a later source revision.
    pub fn finish(self) -> Result<RenderResult> {
        let components = self.pcm.into_components()?;
        let phrase = &self.acoustic.prepared.phrase;
        let audio = components.publish(phrase.visible_begin_seconds, phrase.visible_end_seconds)?;
        let started = Instant::now();
        let visuals = RenderVisuals {
            pitch: self.pitch,
            waveform: Waveform::new(&audio),
        };
        let mut timings = self.acoustic.timings;
        timings.visuals_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(RenderResult {
            audio,
            visuals,
            timings,
            state: OwnerSynthesis {
                prepared: self.acoustic.prepared,
                duration: self.acoustic.duration,
                pitch: self.acoustic.pitch,
                components,
            },
        })
    }
}
