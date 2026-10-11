//! Immutable synthesis output and the display data prepared alongside its PCM.
//!
//! Waveform queries share the rendered samples and use a peak pyramid. Neither
//! painting nor owner-cache publication needs to scan or copy an entire clip.

pub mod patch;
mod pitch;
mod snapshot;
mod waveform;

pub use pitch::PitchTrack;
pub use snapshot::{VisualFlash, VisualPatch, VisualPiece, VisualSnapshot, VisualTimeline};
pub use waveform::Waveform;

use super::{audio::RenderedAudio, engine::OwnerSynthesis};
use std::sync::Arc;

#[derive(Debug)]
pub struct RenderedClip {
    pub audio: Arc<RenderedAudio>,
    pub visuals: Arc<RenderVisuals>,
    pub state: Arc<OwnerSynthesis>,
}

impl RenderedClip {
    /// Shared PCM is counted once, even though the waveform also retains it.
    /// Prepared synthesis context is retained for the next completed edit.
    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of_val(self.audio.samples())
            .saturating_add(self.visuals.memory_bytes())
            .saturating_add(self.state.memory_bytes())
    }
}

#[derive(Debug)]
pub struct RenderVisuals {
    pub pitch: PitchTrack,
    pub waveform: Waveform,
}

impl RenderVisuals {
    /// Additional display storage; the clip owns the shared PCM allocation.
    pub fn memory_bytes(&self) -> usize {
        self.pitch
            .memory_bytes()
            .saturating_add(self.waveform.memory_bytes())
    }
}
