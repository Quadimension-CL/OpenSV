//! Explicit, validated score conditioning for each supported voice family.
//!
//! Neural frames use the voice's native 48 kHz / 240-sample grid; native SV1
//! retains continuous score seconds until its stage-specific quantizers.
//! Export rate and track mixing belong to the output layer. An owner remains
//! one context across rests; its visible bounds only restrict publication.

mod context;
mod lyrics;
mod options;
mod owner;
mod pitch;
mod pronunciation;
mod records;
mod types;
mod voice_binding;

pub use lyrics::LyricFrontend;
pub use owner::prepare_owner;
pub use pitch::PreparedPitchControls;
pub use types::{ExplicitScore, PreparedPhrase, ScoreNote, ScorePhone};
pub use voice_binding::validate_voice;

pub const SAMPLE_RATE: u32 = 48_000;
pub const HOP_SAMPLES: usize = 240;
pub const FRAME_SECONDS: f64 = HOP_SAMPLES as f64 / SAMPLE_RATE as f64;
