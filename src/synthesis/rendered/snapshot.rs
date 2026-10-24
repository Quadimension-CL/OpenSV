use super::RenderVisuals;
use crate::synthesis::plan::OwnerInput;
use std::{ops::Range, sync::Arc};

mod flash;
mod timeline;

pub use flash::VisualFlash;
pub use timeline::VisualTimeline;

#[derive(Clone, Debug)]
pub struct VisualSnapshot {
    pub data: Arc<RenderVisuals>,
    pub input: Arc<OwnerInput>,
}

#[derive(Clone, Debug)]
pub struct VisualPatch {
    pub input: Arc<OwnerInput>,
    pub range: Range<i64>,
    pub data: Option<Arc<RenderVisuals>>,
    pub highlight: bool,
}

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
