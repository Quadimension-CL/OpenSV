use super::RenderVisuals;
use crate::synthesis::plan::OwnerInput;
use std::{ops::Range, sync::Arc};

mod flash;
mod timeline;

pub use flash::VisualFlash;
pub use timeline::VisualTimeline;

/// Render data and the immutable musical geometry that produced it. Keeping
/// them together prevents retained regions from moving with later note edits.
#[derive(Clone, Debug)]
pub struct VisualSnapshot {
    pub data: Arc<RenderVisuals>,
    pub input: Arc<OwnerInput>,
}

/// An atomically published pitch/waveform interval in global musical time.
/// A ready silent interval has no data and removes any old display beneath it.
#[derive(Clone, Debug)]
pub struct VisualPatch {
    pub input: Arc<OwnerInput>,
    pub range: Range<i64>,
    pub data: Option<Arc<RenderVisuals>>,
    pub highlight: bool,
}

/// A visible half-open region backed by one immutable render snapshot.
#[derive(Clone, Debug)]
pub struct VisualPiece {
    pub range: Range<i64>,
    pub snapshot: VisualSnapshot,
}

fn intersection(first: &Range<i64>, second: &Range<i64>) -> Option<Range<i64>> {
    let range = first.start.max(second.start)..first.end.min(second.end);
    (range.start < range.end).then_some(range)
}

fn matching_input(first: &Arc<OwnerInput>, second: &Arc<OwnerInput>) -> bool {
    Arc::ptr_eq(first, second) || first == second
}
