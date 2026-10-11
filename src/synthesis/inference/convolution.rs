//! Temporal convolution with ordered matrix taps, stride and zero padding.

mod kernel;
mod packing;
mod stream;
mod transpose;

pub use stream::ConvolutionStream;
pub use transpose::{
    TRANSPOSED_CONVOLUTION_TYPE_ID, TransposedConvolution, TransposedConvolutionStream,
};

use std::{fmt, sync::Arc};

use super::matrix::{DenseMatrix, MatrixError, check_length};
use packing::PackedWeights;

pub const CONVOLUTION_TYPE_ID: [u8; 16] = [
    0x6d, 0xf2, 0xee, 0x48, 0x5c, 0xad, 0xe1, 0x27, 0xdf, 0x03, 0x0b, 0x97, 0x91, 0x87, 0xcc, 0x1a,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvolutionConfig {
    pub kernel: usize,
    pub stride: usize,
    pub padding: usize,
    pub dilation: usize,
}

impl ConvolutionConfig {
    /// The payload excludes the resource's 16-byte module type prefix.
    pub fn parse(payload: &[u8]) -> Result<Self, ConvolutionError> {
        if payload.len() != 20 {
            return Err(ConvolutionError::InvalidConfig("expected five u32 words"));
        }
        let mut words = [0; 5];
        for (word, bytes) in words.iter_mut().zip(payload.as_chunks::<4>().0.iter()) {
            *word = u32::from_le_bytes(*bytes) as usize;
        }
        if words[4] != 1 {
            return Err(ConvolutionError::InvalidConfig("last word must be one"));
        }
        let config = Self {
            kernel: words[0],
            stride: words[1],
            padding: words[2],
            dilation: words[3],
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(self) -> Result<(), ConvolutionError> {
        if self.kernel == 0 || self.stride == 0 || self.dilation == 0 {
            return Err(ConvolutionError::InvalidConfig(
                "zero kernel, stride or dilation",
            ));
        }
        // The queue consumer needs overlapping or adjacent windows.
        if self.stride > self.receptive_frames()? {
            return Err(ConvolutionError::InvalidConfig(
                "stride exceeds receptive window",
            ));
        }
        self.padding
            .checked_mul(2)
            .ok_or(MatrixError::InvalidShape)?;
        Ok(())
    }

    pub fn receptive_frames(self) -> Result<usize, ConvolutionError> {
        self.kernel
            .checked_sub(1)
            .and_then(|n| n.checked_mul(self.dilation))
            .and_then(|n| n.checked_add(1))
            .ok_or(MatrixError::InvalidShape.into())
    }

    fn available_outputs(self, frames: usize) -> Result<usize, ConvolutionError> {
        Ok(frames
            .checked_sub(self.receptive_frames()?)
            .map_or(0, |n| n / self.stride + 1))
    }
}

#[derive(Debug)]
pub enum ConvolutionError {
    InvalidConfig(&'static str),
    Matrix(MatrixError),
    Finished,
    Cancelled,
    Parallel(String),
}

impl From<MatrixError> for ConvolutionError {
    fn from(value: MatrixError) -> Self {
        Self::Matrix(value)
    }
}

impl fmt::Display for ConvolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "Invalid convolution: {message}"),
            Self::Matrix(error) => error.fmt(f),
            Self::Finished => f.write_str("Convolution stream has already been finalized"),
            Self::Cancelled => f.write_str("Synthesis cancelled"),
            Self::Parallel(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ConvolutionError {}

/// Immutable, shared CPU weights, packed once across independent output channels.
/// Every output accumulates taps in order, then input channels within each tap.
#[derive(Clone)]
pub struct Convolution {
    config: ConvolutionConfig,
    weights: PackedWeights,
    bias: Option<Arc<[f32]>>,
}

impl Convolution {
    pub fn new(
        config: ConvolutionConfig,
        taps: Vec<DenseMatrix>,
        bias: Option<Arc<[f32]>>,
    ) -> Result<Self, ConvolutionError> {
        config.validate()?;
        check_length("convolution taps", config.kernel, taps.len())?;
        let rows = taps[0].rows();
        let columns = taps[0].columns();
        if taps
            .iter()
            .any(|tap| tap.rows() != rows || tap.columns() != columns)
        {
            return Err(ConvolutionError::InvalidConfig("tap matrix shapes differ"));
        }
        if let Some(bias) = &bias {
            check_length("convolution bias", rows, bias.len())?;
        }
        Ok(Self {
            config,
            weights: PackedWeights::new(&taps)?,
            bias,
        })
    }

    pub fn config(&self) -> ConvolutionConfig {
        self.config
    }

    pub fn input_channels(&self) -> usize {
        self.weights.input_channels()
    }

    pub fn output_channels(&self) -> usize {
        self.weights.output_channels()
    }

    pub fn has_bias(&self) -> bool {
        self.bias.is_some()
    }

    pub fn output_frames(&self, input_frames: usize) -> Result<usize, ConvolutionError> {
        let padded = input_frames
            .checked_add(self.config.padding * 2)
            .ok_or(MatrixError::InvalidShape)?;
        self.config.available_outputs(padded)
    }

    pub fn stream(&self) -> Result<ConvolutionStream, ConvolutionError> {
        ConvolutionStream::new(self.clone())
    }

    /// Execute one complete sequence. Input/output are contiguous frames.
    pub fn execute(&self, input: &[f32]) -> Result<Vec<f32>, ConvolutionError> {
        self.stream()?.process(input, true)
    }

    fn input_frames(&self, input: &[f32]) -> Result<usize, ConvolutionError> {
        if !input.len().is_multiple_of(self.input_channels()) {
            return Err(ConvolutionError::InvalidConfig("partial input frame"));
        }
        Ok(input.len() / self.input_channels())
    }

    /// Queue already contains the left padding. Missing trailing rows are zero.
    fn execute_pending(
        &self,
        pending: &[f32],
        frames: usize,
    ) -> Result<Vec<f32>, ConvolutionError> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("kernel.convolution");
        let output_channels = self.output_channels();
        let length = frames
            .checked_mul(output_channels)
            .ok_or(MatrixError::InvalidShape)?;
        // Establish the source-address bound before the packed kernel derives
        // any per-frame slices, including logical trailing zero-padding rows.
        if frames != 0 {
            (frames - 1)
                .checked_mul(self.config.stride)
                .and_then(|frame| frame.checked_add(self.config.receptive_frames().ok()? - 1))
                .and_then(|frame| frame.checked_add(1))
                .and_then(|frames| frames.checked_mul(self.input_channels()))
                .ok_or(MatrixError::InvalidShape)?;
        }
        let mut output = vec![0.0; length];
        self.weights
            .execute_pending(self.config, pending, &mut output, self.bias.as_deref())?;
        Ok(output)
    }
}
