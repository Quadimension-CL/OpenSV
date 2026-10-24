use std::sync::Arc;

use crate::synthesis::inference::convolution::{
    ConvolutionConfig, TransposedConvolution, TransposedConvolutionStream,
};

use super::{PreparedNode, Result, TensorStorage, VoiceError, convolution::PreparedConvolution};

pub struct PreparedTransposedConvolution {
    pub weights: Vec<Arc<TensorStorage>>,
    pub bias: Option<Arc<TensorStorage>>,
    pub input_features: usize,
    pub output_features: usize,
    pub config: ConvolutionConfig,
    operator: TransposedConvolution,
}

impl PreparedTransposedConvolution {
    pub(super) fn prepare(
        payload: &[u8],
        nodes: &[PreparedNode],
        children: &[usize],
    ) -> Result<Self> {
        let prepared = PreparedConvolution::prepare(payload, nodes, children)?;
        Ok(Self {
            weights: prepared.weights,
            bias: prepared.bias,
            input_features: prepared.input_features,
            output_features: prepared.output_features,
            config: prepared.config,
            operator: TransposedConvolution::from_convolution(prepared.operator),
        })
    }

    pub fn stream(&self) -> TransposedConvolutionStream {
        self.operator.stream()
    }

    pub fn output_frames(&self, frames: usize) -> Result<usize> {
        self.operator
            .output_frames(frames)
            .map_err(|error| VoiceError::new(error.to_string()))
    }

    pub fn execute(&self, input: &[f32]) -> Result<Vec<f32>> {
        self.operator
            .execute(input)
            .map_err(|error| VoiceError::new(error.to_string()))
    }
}
