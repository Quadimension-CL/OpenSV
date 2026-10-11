use std::sync::Arc;

use crate::synthesis::voice::{PreparedTree, Result, VoiceError};

/// Form each style vector as the neutral row plus weighted style rows.
pub struct TimbreStyles {
    pub neutral: Arc<[f32]>,
    pub variants: Vec<Arc<[f32]>>,
}

impl TimbreStyles {
    pub fn prepare(tree: &PreparedTree, node: usize) -> Result<Self> {
        let children = &tree.node(node)?.children;
        if children.len() != 2 {
            return Err(VoiceError::new(
                "Timbre style map requires the vector list and coefficients",
            ));
        }
        let vectors: Vec<Arc<[f32]>> = tree
            .node(children[0])?
            .children
            .iter()
            .map(|&index| {
                tree.tensor(index)?
                    .vector()
                    .map(Arc::from)
                    .ok_or_else(|| VoiceError::new("Timbre style row is not a float vector"))
            })
            .collect::<Result<_>>()?;
        let (neutral, variants) = vectors
            .split_first()
            .ok_or_else(|| VoiceError::new("Timbre style map has no neutral row"))?;
        if neutral.is_empty() || variants.iter().any(|row| row.len() != neutral.len()) {
            return Err(VoiceError::new(
                "Timbre style rows have incompatible dimensions",
            ));
        }
        Ok(Self {
            neutral: Arc::clone(neutral),
            variants: variants.to_vec(),
        })
    }

    /// Weights are the already resolved per-style coefficients for this branch.
    /// A primary and a secondary branch may use different weights.
    pub fn blend(&self, weights: &[f32]) -> Result<Vec<f32>> {
        if weights.len() != self.variants.len() || weights.iter().any(|x| !x.is_finite()) {
            return Err(VoiceError::new(
                "Timbre style coefficient count or value is invalid",
            ));
        }
        let mut output = self.neutral.to_vec();
        for (row, &weight) in self.variants.iter().zip(weights) {
            for (target, &value) in output.iter_mut().zip(row.iter()) {
                *target += value * weight;
            }
        }
        Ok(output)
    }
}
