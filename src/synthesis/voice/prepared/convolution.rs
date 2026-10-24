use std::sync::Arc;

use crate::synthesis::inference::{
    convolution::{Convolution, ConvolutionConfig, ConvolutionStream},
    matrix::DenseMatrix,
};

use super::{PreparedModule, PreparedNode, Result, TensorStorage, VoiceError};

pub struct PreparedConvolution {
    pub weights: Vec<Arc<TensorStorage>>,
    pub bias: Option<Arc<TensorStorage>>,
    pub input_features: usize,
    pub output_features: usize,
    pub config: ConvolutionConfig,
    pub(super) operator: Convolution,
}

impl PreparedConvolution {
    pub(super) fn prepare(
        payload: &[u8],
        nodes: &[PreparedNode],
        children: &[usize],
    ) -> Result<Self> {
        let config = ConvolutionConfig::parse(payload).map_err(voice_error)?;
        if children.len() != config.kernel && children.len() != config.kernel + 1 {
            return Err(VoiceError::new(
                "Convolution needs K matrix children and optional bias",
            ));
        }
        let mut weights = Vec::with_capacity(config.kernel);
        let mut matrices = Vec::with_capacity(config.kernel);
        let mut bias = None;
        let mut bias_values = None;
        for &index in children {
            let Some(PreparedModule::Tensor(tensor)) = nodes.get(index).map(|node| &node.module)
            else {
                return Err(VoiceError::new("Convolution child must be a tensor"));
            };
            let matrix = match &**tensor {
                TensorStorage::FloatMatrix {
                    rows,
                    columns,
                    values,
                    ..
                } => DenseMatrix::from_float(*rows, *columns, Arc::clone(values))
                    .map_err(voice_error)?,
                TensorStorage::QuantizedMatrix {
                    legacy_integer_execution,
                    rows,
                    columns,
                    bits,
                    input_exponent,
                    scales,
                    values,
                } => DenseMatrix::from_quantized_execution(
                    *rows,
                    *columns,
                    *bits,
                    *input_exponent,
                    scales,
                    values,
                    *legacy_integer_execution,
                )
                .map_err(voice_error)?,
                TensorStorage::FloatVector { values } => {
                    if bias.is_some() {
                        return Err(VoiceError::new("Convolution has multiple bias vectors"));
                    }
                    bias = Some(Arc::clone(tensor));
                    bias_values = Some(Arc::clone(values));
                    continue;
                }
            };
            weights.push(Arc::clone(tensor));
            matrices.push(matrix);
        }
        let operator = Convolution::new(config, matrices, bias_values).map_err(voice_error)?;
        Ok(Self {
            weights,
            bias,
            input_features: operator.input_channels(),
            output_features: operator.output_channels(),
            config,
            operator,
        })
    }

    pub fn stream(&self) -> Result<ConvolutionStream> {
        self.operator.stream().map_err(voice_error)
    }

    pub fn output_frames(&self, frames: usize) -> Result<usize> {
        self.operator.output_frames(frames).map_err(voice_error)
    }

    pub fn execute(&self, input: &[f32]) -> Result<Vec<f32>> {
        self.operator.execute(input).map_err(voice_error)
    }
}

fn voice_error(error: impl std::fmt::Display) -> VoiceError {
    VoiceError::new(error.to_string())
}
