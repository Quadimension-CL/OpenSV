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
    pub fn phoneme_track(&self)->crate::synthesis::rendered::PhonemeTrack {
        crate::synthesis::rendered::PhonemeTrack::new(&self.acoustic.prepared.phrase,&self.acoustic.duration)
    }

    pub fn pitch_track(&self) -> Result<PitchTrack> {
        Ok(self.pitch.clone())
    }

    pub fn ready_audio(&self) -> Result<Vec<RenderedAudio>> {
        self.pcm.ready_audio()
    }

    pub fn model_samples(&self) -> Range<i64> {
        self.pcm.model_samples()
    }

    pub fn needs_audio(&self, seconds: f64) -> bool {
        self.pcm.needs(seconds)
    }

    pub fn is_complete(&self) -> bool {
        self.pcm.is_complete()
    }

    pub fn finish(self) -> Result<RenderResult> {
        let components = self.pcm.into_components()?;
        let phrase = &self.acoustic.prepared.phrase;
        let audio = components.publish(phrase.visible_begin_seconds, phrase.visible_end_seconds)?;
        let started = Instant::now();
        let visuals = RenderVisuals {
            phonemes:crate::synthesis::rendered::PhonemeTrack::new(phrase,&self.acoustic.duration),
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
