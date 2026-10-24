use std::sync::Arc;

use crate::synthesis::voice::{PreparedTree, Result, VoiceError};

pub struct TimbreStyles {
    pub neutral: Arc<[f32]>,
    pub variants: Vec<Arc<[f32]>>,
}

pub(super) struct AcousticStyles {
    pub primary: crate::synthesis::inference::Sequence,
    pub secondary: crate::synthesis::inference::Sequence,
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

#[cfg(test)]
mod mode_arithmetic_tests {
    use super::*;

    fn basis(base: Vec<f32>, variants: Vec<Vec<f32>>) -> TimbreStyles {
        TimbreStyles {
            neutral: base.into(),
            variants: variants.into_iter().map(Arc::from).collect(),
        }
    }

    #[test]
    fn serialized_mode_order_is_observable() {
        let model = basis(vec![1.0e20], vec![vec![-1.0e20], vec![3.0]]);
        assert_eq!(model.blend(&[1.0, 1.0]).unwrap(), vec![3.0]);
        let reversed = basis(vec![1.0e20], vec![vec![3.0], vec![-1.0e20]]);
        assert_eq!(reversed.blend(&[1.0, 1.0]).unwrap(), vec![0.0]);
    }

    #[test]
    fn no_fused_multiply_add_or_nonlinear_weight_transform() {
        let a = std::hint::black_box(1.0_f32 + f32::EPSILON);
        let b = std::hint::black_box(1.0_f32 - f32::EPSILON);
        let actual = basis(vec![-1.0], vec![vec![a]]).blend(&[b]).unwrap()[0];
        assert_eq!(actual.to_bits(), 0.0_f32.to_bits());
        assert_ne!(actual.to_bits(), a.mul_add(b, -1.0).to_bits());
        assert_eq!(basis(vec![1.0], vec![vec![2.0]]).blend(&[1.5]).unwrap(), vec![4.0]);
    }

    #[test]
    fn output_is_independent_and_basis_remains_unchanged() {
        let model = basis(vec![0.25, -0.5], vec![vec![1.0, 2.0]]);
        let mut first = model.blend(&[0.5]).unwrap();
        first.fill(99.0);
        assert_eq!(model.blend(&[0.5]).unwrap(), vec![0.75, 0.5]);
        assert_eq!(&*model.neutral, &[0.25, -0.5]);
        assert_eq!(&*model.variants[0], &[1.0, 2.0]);
    }

    #[test]
    fn mismatched_or_nonfinite_coefficients_reject() {
        let model = basis(vec![0.0], vec![vec![1.0]]);
        assert!(model.blend(&[]).is_err());
        assert!(model.blend(&[f32::NAN]).is_err());
        assert!(model.blend(&[f32::INFINITY]).is_err());
    }
}
