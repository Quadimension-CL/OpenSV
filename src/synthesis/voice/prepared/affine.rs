use std::sync::Arc;

use crate::synthesis::inference::{affine::Affine, matrix::DenseMatrix};

use super::{PreparedModule, PreparedNode, Result, TensorStorage, VoiceError};

pub struct PreparedAffine {
    pub weights: Arc<TensorStorage>,
    pub bias: Option<Arc<TensorStorage>>,
    pub input_features: usize,
    pub output_features: usize,
    operator: Affine,
}

impl PreparedAffine {
    pub fn operator(&self) -> &Affine {
        &self.operator
    }
    pub(super) fn prepare(nodes: &[PreparedNode], children: &[usize]) -> Result<Self> {
        if !(1..=2).contains(&children.len()) {
            return Err(VoiceError::new(
                "Affine requires a matrix and optional bias child",
            ));
        }
        let weights = child_tensor(nodes, children[0])?;
        let matrix = match &*weights {
            TensorStorage::FloatMatrix {
                rows,
                columns,
                values,
                ..
            } => DenseMatrix::from_float(*rows, *columns, Arc::clone(values)),
            TensorStorage::QuantizedMatrix {
                legacy_integer_execution,
                rows,
                columns,
                bits,
                input_exponent,
                scales,
                values,
            } => {
                DenseMatrix::from_quantized_execution(*rows, *columns, *bits, *input_exponent, scales, values,*legacy_integer_execution)
            }
            TensorStorage::FloatVector { .. } => {
                return Err(VoiceError::new("Affine first child is not a matrix"));
            }
        }
        .map_err(|error| VoiceError::new(error.to_string()))?;
        let bias = children
            .get(1)
            .map(|index| child_tensor(nodes, *index))
            .transpose()?;
        let bias_values = bias
            .as_ref()
            .map(|tensor| match &**tensor {
                TensorStorage::FloatVector { values } => Ok(Arc::clone(values)),
                _ => Err(VoiceError::new("Affine second child is not a float vector")),
            })
            .transpose()?;
        let operator =
            Affine::new(matrix, bias_values).map_err(|error| VoiceError::new(error.to_string()))?;
        Ok(Self {
            input_features: operator.input_channels(),
            output_features: operator.output_channels(),
            weights,
            bias,
            operator,
        })
    }

    pub fn execute(&self, input: &[f32]) -> Result<Vec<f32>> {
        if !input.len().is_multiple_of(self.input_features) {
            return Err(VoiceError::new(format!(
                "Affine requires complete {}-channel input frames, got {} values",
                self.input_features,
                input.len()
            )));
        }
        let frames = input.len() / self.input_features;
        let length = frames
            .checked_mul(self.output_features)
            .ok_or_else(|| VoiceError::new("Affine output size overflows"))?;
        let mut output = vec![0.0; length];
        self.operator
            .execute_batch(input, &mut output, frames)
            .map_err(|error| VoiceError::new(error.to_string()))?;
        Ok(output)
    }
}

fn child_tensor(nodes: &[PreparedNode], index: usize) -> Result<Arc<TensorStorage>> {
    match nodes.get(index).map(|node| &node.module) {
        Some(PreparedModule::Tensor(storage)) => Ok(Arc::clone(storage)),
        _ => Err(VoiceError::new(
            "Affine child must be a prepared tensor storage",
        )),
    }
}
