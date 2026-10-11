//! The local default-duration path of the Mo Xu fourth model tree.
//!
//! Phones without a nonzero retake seed use child 8's scalar prediction.
//! This evaluates that deterministic branch directly; the containment tree
//! is not executed as a list.

mod attention;
pub(crate) mod context;
pub(crate) mod encoding;
pub mod frontend;
mod heads;

use crate::synthesis::voice::{
    DURATION_ROOT_TYPE_ID, DurationConfiguration, PreparedModule, PreparedTree, Result, VoiceError,
};

use serde::Serialize;

pub use frontend::{DurationNote, DurationRecord, ExplicitPhone, PhoneDuration, PhoneFeatures};

pub struct DurationStage<'a> {
    tree: &'a PreparedTree,
    children: [usize; 14],
    configuration: DurationConfiguration,
    identity_bias: Vec<f32>,
}

/// Model outputs before the frontend redistributes frames within each record.
#[derive(Debug, Serialize)]
pub struct DurationPrediction {
    pub phones: Vec<PhoneFeatures>,
    pub scalars: Vec<f32>,
    /// Discrete head index / (bins - 1). The timbre
    /// record consumer uses this as an impulse value; zero suppresses writeback.
    pub impulse_values: Vec<f64>,
    /// Softmax-weighted index / (bins - 1), used by timbre
    /// to place that impulse within the phoneme's frames.
    pub impulse_positions: Vec<f32>,
}

#[derive(Debug, Serialize)]
pub struct DurationResult {
    pub durations: Vec<PhoneDuration>,
    pub prediction: DurationPrediction,
}

impl<'a> DurationStage<'a> {
    /// `identity` is the 64-bit bank/session model condition, not a random
    /// synthesis seed.
    pub fn prepare(tree: &'a PreparedTree, identity: u64) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("duration.prepare");
        let root = tree.node(tree.root_index())?;
        if root.type_id != DURATION_ROOT_TYPE_ID {
            return Err(VoiceError::new(
                "The supplied tree is not the local duration model",
            ));
        }
        let children: [usize; 14] = root.children.as_slice().try_into().map_err(|_| {
            VoiceError::new("Duration model requires its fourteen observed children")
        })?;
        let configuration = DurationConfiguration::parse(&tree.resource_bytes(tree.root_index())?)?;
        let PreparedModule::Affine(input) = &tree.node(children[5])?.module else {
            return Err(VoiceError::new("Duration input projection is not prepared"));
        };
        let constant_width = tree
            .tensor(children[1])?
            .vector()
            .ok_or_else(|| VoiceError::new("Duration fixed conditioning is not a float vector"))?
            .len();
        configuration.validate_input_projection(
            input.output_features,
            input.input_features,
            constant_width,
        )?;
        // Expand the identity from most significant to least significant bit.
        let identity_bits: Vec<f32> = (0..64)
            .map(|index| ((identity >> (63 - index)) & 1) as f32)
            .collect();
        let identity_bias = tree.execute_node(children[6], &identity_bits)?;
        if identity_bias.len() != input.output_features {
            return Err(VoiceError::new(
                "Duration identity conditioning width differs from its projection",
            ));
        }
        Ok(Self {
            tree,
            children,
            configuration,
            identity_bias,
        })
    }

    pub fn predict_explicit(&self, records: &[DurationRecord]) -> Result<DurationResult> {
        let phones = frontend::prepare_explicit(self.tree, records)?;
        let prediction = self.predict(phones)?;
        let durations = frontend::postprocess(self.tree, records, &prediction.scalars)?;
        Ok(DurationResult {
            durations,
            prediction,
        })
    }

    /// Execute real explicit-phone conditions on the default, non-retake path.
    pub fn predict(&self, phones: Vec<PhoneFeatures>) -> Result<DurationPrediction> {
        if phones.is_empty() {
            return Ok(DurationPrediction {
                phones,
                scalars: Vec::new(),
                impulse_values: Vec::new(),
                impulse_positions: Vec::new(),
            });
        }
        let features = encoding::encode(self.tree, &self.children, self.configuration, &phones)?;
        let width = self.identity_bias.len();
        let mut encoded = self.tree.execute_node(self.children[5], &features)?;
        if encoded.len() != phones.len() * width {
            return Err(VoiceError::new("Duration projection frame count changed"));
        }
        for frame in encoded.chunks_exact_mut(width) {
            for (value, bias) in frame.iter_mut().zip(&self.identity_bias) {
                *value += bias;
            }
        }
        for node in &self.tree.node(self.children[7])?.children {
            encoded = context::encode(self.tree, *node, &encoded, width)?;
        }
        // A zero retake seed selects child 8 instead of the sampled path.
        let scalars = self.tree.execute_node(self.children[8], &encoded)?;
        if scalars.len() != phones.len() || scalars.iter().any(|v| !v.is_finite()) {
            return Err(VoiceError::new(
                "Duration prediction is not one finite scalar per phone",
            ));
        }
        // Project the scalar back into context space before both heads.
        let mut head_input = self.tree.execute_node(self.children[11], &scalars)?;
        if head_input.len() != encoded.len() {
            return Err(VoiceError::new(
                "Duration scalar projection width differs from context",
            ));
        }
        for (value, context) in head_input.iter_mut().zip(encoded) {
            *value += context;
        }
        let boundary = self.tree.execute_node(self.children[12], &head_input)?;
        let transition = self.tree.execute_node(self.children[13], &head_input)?;
        let impulse_values = heads::maximum_index(
            &boundary,
            self.configuration.unassigned_words[1] as usize,
            phones.len(),
        )?;
        let impulse_positions = heads::expected_index(
            &transition,
            self.configuration.unassigned_words[2] as usize,
            phones.len(),
        )?;
        Ok(DurationPrediction {
            phones,
            scalars,
            impulse_values,
            impulse_positions,
        })
    }
}
