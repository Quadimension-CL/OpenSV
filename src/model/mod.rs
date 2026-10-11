mod automation;
mod dictionary;
mod instrumental;
mod language;
pub mod pitch;
mod project;
mod time;
mod track_color;

pub use automation::*;
pub use dictionary::*;
pub use instrumental::*;
pub use language::*;
pub use pitch::{PitchControl, PitchCurve, PitchPoint};
pub use project::*;
pub use time::*;
pub use track_color::*;

use std::sync::atomic::{AtomicU64, Ordering};
pub type EntityId = u64;
pub fn new_id() -> EntityId {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
pub type Extra = std::collections::BTreeMap<String, serde_json::Value>;
mod singer;
pub use singer::singer_database;
