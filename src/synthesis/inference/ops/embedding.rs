use std::{fmt, sync::Arc};

use crate::synthesis::inference::matrix::DenseMatrix;

pub const EMBEDDING_TYPE_ID: [u8; 16] = [
    0x8b, 0x8c, 0x95, 0x22, 0x55, 0xe2, 0x7b, 0xfa, 0xe4, 0xe7, 0x85, 0xaa, 0xae, 0x69, 0xb1, 0x98,
];

#[derive(Clone)]
pub struct Embedding {
    features: usize,
    entries: usize,
    values: Arc<[f32]>,
}

#[derive(Clone, Debug)]
pub enum EmbeddingError {
    SizeOverflow,
    OutputLength { expected: usize, actual: usize },
    NonFiniteIndex { position: usize },
}

impl fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeOverflow => f.write_str("Embedding output dimensions overflow"),
            Self::OutputLength { expected, actual } => {
                write!(
                    f,
                    "Embedding output length: expected {expected}, got {actual}"
                )
            }
            Self::NonFiniteIndex { position } => {
                write!(f, "Embedding index {position} is not finite")
            }
        }
    }
}

impl std::error::Error for EmbeddingError {}

impl Embedding {
    pub fn new(matrix: &DenseMatrix) -> Self {
        let features = matrix.rows();
        let entries = matrix.columns();
        let mut values = vec![0.0; features * entries];
        for feature in 0..features {
            let row = matrix.row(feature);
            for (entry, &value) in row.iter().enumerate() {
                values[entry * features + feature] = value;
            }
        }
        Self {
            features,
            entries,
            values: values.into(),
        }
    }

    pub fn features(&self) -> usize {
        self.features
    }

    pub fn entries(&self) -> usize {
        self.entries
    }

    pub fn execute(&self, indices: &[f32], output: &mut [f32]) -> Result<(), EmbeddingError> {
        let expected = indices
            .len()
            .checked_mul(self.features)
            .ok_or(EmbeddingError::SizeOverflow)?;
        if expected != output.len() {
            return Err(EmbeddingError::OutputLength {
                expected,
                actual: output.len(),
            });
        }
        if let Some(position) = indices.iter().position(|value| !value.is_finite()) {
            return Err(EmbeddingError::NonFiniteIndex { position });
        }
        for (&index, frame) in indices.iter().zip(output.chunks_exact_mut(self.features)) {
            let entry = index.round().clamp(0.0, (self.entries - 1) as f32) as usize;
            frame.copy_from_slice(&self.values[entry * self.features..(entry + 1) * self.features]);
        }
        Ok(())
    }
}
