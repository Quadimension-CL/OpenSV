//! Native voice preparation, neural synthesis, render scheduling and audio output.
pub mod audio;
pub mod engine;
pub mod execution;
pub mod inference;
pub mod instrumental;
mod mixing_options;
pub mod plan;
#[cfg(feature = "synth-profile")]
pub mod profiling;
pub mod render_cache;
pub mod rendered;
pub mod runtime;
pub mod score;
pub mod settings;
pub mod stages;
pub mod voice;
