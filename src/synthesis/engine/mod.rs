//! One voice's offline neural program. Model weights are shared, execution
//! state belongs to a render. Scoped cancellation reaches model nodes and
//! numerical blocks as well as the four stage publication checkpoints.

mod acoustics;
mod components;
pub mod incremental;
mod models;
mod pitch;
mod prepared;
pub mod progressive;
mod render;

pub use acoustics::{
    AcousticRender, PreparedRenderAcoustics, StageTimings, prepare_acoustics, synthesize_acoustics,
};
pub use components::PcmComponents;
pub use incremental::synthesize_owner;
pub use prepared::{OwnerSynthesis, PreparedAcoustics, PreparedModelFeatures};
pub use render::{RenderResult, synthesize};

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum RenderStage {
    Duration,
    Pitch,
    Timbre,
    Vocoder,
}

impl RenderStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Duration => "音素时长",
            Self::Pitch => "音高",
            Self::Timbre => "音色",
            Self::Vocoder => "声码器",
        }
    }
}
