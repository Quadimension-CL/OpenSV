use super::super::{
    audio::RenderedAudio,
    plan::{Coverage, OwnerId, OwnerInput, OwnerOutputControls, OwnerRequest},
    rendered::{RenderVisuals, RenderedClip},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// An immutable, complete owner result and the exact input that produced it.
/// Its Arc identity is the commit identity used by a replacement render.
#[derive(Debug)]
pub struct CompletedOwner {
    pub input: Arc<OwnerInput>,
    pub clip: Arc<RenderedClip>,
}

/// A captured run, not a cache key. Execution checks its shared cancellation
/// flag within expensive stages; `finish` always checks identity again.
#[derive(Clone, Debug)]
pub struct RenderJob {
    pub request: Arc<OwnerRequest>,
    /// Last completed owner, independent of intervening cancelled requests.
    pub base: Option<Arc<CompletedOwner>>,
    pub(super) generation: u64,
    pub(super) serial: u64,
    pub(super) cancelled: Arc<AtomicBool>,
}

impl RenderJob {
    pub(crate) fn cancellation(&self) -> super::super::execution::Cancellation {
        super::super::execution::Cancellation::from_flag(Arc::clone(&self.cancelled))
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Immutable publication descriptor. The playback preparer must apply coverage
/// clipping in project time and these latest mixer controls. Native PCM may use
/// a different sample rate and include warmup/tail beyond the visible interval.
#[derive(Clone, Debug)]
pub struct PublishedOwner {
    pub owner: OwnerId,
    pub coverage: Coverage,
    pub controls: OwnerOutputControls,
    pub audio: Arc<RenderedAudio>,
    pub visuals: Arc<RenderVisuals>,
}

#[derive(Default, Debug)]
pub struct CacheUpdate {
    /// Immediately remove old placements from any prepared playback snapshot.
    /// Includes removal, changed input/placement and instance muting.
    pub withdrawn: BTreeMap<OwnerId, Coverage>,
    pub publication_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completion {
    /// The current owner now has usable audio and visuals. It can still be muted.
    Published,
    Failed(String),
    /// The run was superseded, reset, removed, or already completed.
    Discarded,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct CacheStats {
    pub queued: usize,
    pub running: usize,
    pub ready: usize,
    pub failed: usize,
    /// Unique completed audio and visual buffers held by current owners,
    /// including bases retained during queued, running or failed replacement.
    pub current_render_bytes: usize,
    pub discarded_completions: u64,
}
