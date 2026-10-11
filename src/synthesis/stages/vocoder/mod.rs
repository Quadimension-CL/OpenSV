//! Voice-specific spectral vocoder and its streaming CPU signal processors.

pub mod condition;
pub mod configuration;
pub mod excitation;
mod excitation_fourier;
mod excitation_spectrum;
mod filter_kernel;
pub mod filterbank;
mod fourier;
pub mod gain_filter;
pub mod interpolation;
pub mod metadata;
pub mod output;
pub mod postfilter;
mod render;
pub mod sampling;
pub mod source;
pub mod spectral_heads;
mod stage;
mod stream;
mod stream_parameters;
mod stream_queue;
mod stream_signals;
pub use render::VocoderOutput;
pub use stage::{VocoderParameters, VocoderStage};
pub use stream::VocoderStream;
