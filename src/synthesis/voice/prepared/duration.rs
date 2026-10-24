use super::{PreparedModule, PreparedNode};
use crate::synthesis::voice::{DurationConfiguration, Result, VoiceError};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct DurationInputLayout {
    pub configuration: DurationConfiguration,
    pub constant_node: usize,
    pub constant_width: usize,
    pub input_projection_node: usize,
    pub feature_width: usize,
    pub projected_width: usize,
}

impl DurationInputLayout {
    pub(super) fn prepare(
        configuration: DurationConfiguration,
        root: &PreparedNode,
        nodes: &[PreparedNode],
    ) -> Result<Self> {
        if root.children.len() != 14 {
            return Err(VoiceError::new(
                "Observed duration root requires fourteen children",
            ));
        }
        let constant_node = root.children[1];
        let constant_width = match &nodes[constant_node].module {
            PreparedModule::Tensor(storage) => match storage.as_ref() {
                super::TensorStorage::FloatVector { values } => Some(values.len()),
                super::TensorStorage::FloatMatrix { columns, rows, .. } if *rows > 0 => {
                    Some(*columns)
                }
                _ => None,
            },
            _ => None,
        }
        .ok_or_else(|| {
            VoiceError::new("Duration conditioning must be a float vector or speaker matrix")
        })?;
        let input_projection_node = root.children[5];
        let PreparedModule::Affine(projection) = &nodes[input_projection_node].module else {
            return Err(VoiceError::new(
                "Duration root child 5 must be its input projection",
            ));
        };
        configuration.validate_input_projection(
            projection.output_features,
            projection.input_features,
            constant_width,
        )?;
        Ok(Self {
            configuration,
            constant_node,
            constant_width,
            input_projection_node,
            feature_width: configuration.feature_width(constant_width)?,
            projected_width: projection.output_features,
        })
    }
}
