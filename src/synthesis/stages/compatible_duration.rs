mod configuration;
mod encoding;

use crate::synthesis::voice::{PreparedTree, Result, VoiceError};

use super::duration::{
    DurationPrediction, DurationRecord, DurationResult, PhoneFeatures, context, frontend,
};
use configuration::Configuration;

pub const ROOT_TYPE_ID: [u8; 16] = [
    0x58, 0x3c, 0xe1, 0x32, 0x9c, 0xb9, 0x16, 0x54, 0x0a, 0xa0, 0x16, 0xe5, 0xcc, 0x96, 0xc1, 0x62,
];
const FRONTEND_TYPE_ID: [u8; 16] = [
    0x21, 0x11, 0x61, 0x7b, 0x6d, 0x9f, 0x86, 0x56, 0x1d, 0x5a, 0xa0, 0xbb, 0x63, 0x3b, 0x37, 0x10,
];

pub struct CompatibleDurationStage<'a> {
    tree: &'a PreparedTree,
    children: [usize; 9],
    configuration: Configuration,
    identity_bias: Vec<f32>,
}

impl<'a> CompatibleDurationStage<'a> {

    pub fn prepare(tree: &'a PreparedTree, identity: u64) -> Result<Self> {
        let (configuration, children) = configuration::validate(tree)?;

        let bits: Vec<_> = (0..64)
            .map(|index| ((identity >> (63 - index)) & 1) as f32)
            .collect();
        let identity_bias = tree.execute_node(children[6], &bits)?;
        #[cfg(feature="acoustic-diagnostic")]
        super::duration::diagnostic::capture("compatible_identity_bits",1,&bits)?;
        #[cfg(feature="acoustic-diagnostic")]
        super::duration::diagnostic::capture("compatible_identity_projection",1,&identity_bias)?;
        if identity_bias.len() != configuration.context
            || identity_bias.iter().any(|value| !value.is_finite())
        {
            return Err(VoiceError::new(
                "Invalid compatibility duration identity projection",
            ));
        }
        Ok(Self {
            tree,
            children,
            configuration,
            identity_bias,
        })
    }

    pub fn validate(tree: &PreparedTree) -> Result<()> {
        configuration::validate(tree).map(|_| ())
    }

    pub fn predict_explicit(&self, records: &[DurationRecord]) -> Result<DurationResult> {
        let phones = frontend::prepare_explicit_typed(self.tree, records, FRONTEND_TYPE_ID)?;
        let prediction = self.predict(phones)?;
        let durations =
            frontend::postprocess_typed(self.tree, records, &prediction.scalars, FRONTEND_TYPE_ID)?;
        Ok(DurationResult {
            natural_frames: durations.iter().map(|phone|phone.frames).collect(),
            durations,
            prediction,
        })
    }

    fn predict(&self, phones: Vec<PhoneFeatures>) -> Result<DurationPrediction> {
        let scalars = if phones.is_empty() {
            Vec::new()
        } else {
            let features =
                encoding::encode(self.tree, &self.children, self.configuration, &phones)?;
            #[cfg(feature="acoustic-diagnostic")]
            super::duration::diagnostic::capture("compatible_input_features",phones.len(),&features)?;
            let width = self.configuration.context;
            let mut encoded = self.tree.execute_node(self.children[5], &features)?;
            #[cfg(feature="acoustic-diagnostic")]
            super::duration::diagnostic::capture("compatible_input_projection",phones.len(),&encoded)?;
            if encoded.len()
                != phones.len().checked_mul(width).ok_or_else(|| {
                    VoiceError::new("Compatibility duration feature dimensions overflow")
                })?
            {
                return Err(VoiceError::new(
                    "Compatibility duration projection changed frame count",
                ));
            }
            for row in encoded.chunks_exact_mut(width) {
                for (value, bias) in row.iter_mut().zip(&self.identity_bias) {
                    *value += bias;
                }
            }
            for &node in &self.tree.node(self.children[7])?.children {
                encoded = context::encode(self.tree, node, &encoded, width)?;
            }
            #[cfg(feature="acoustic-diagnostic")]
            super::duration::diagnostic::capture("compatible_context_to_scalar_head",phones.len(),&encoded)?;
            let scalars = self.tree.execute_node(self.children[8], &encoded)?;
            #[cfg(feature="acoustic-diagnostic")]
            super::duration::diagnostic::capture("compatible_scalar_head",phones.len(),&scalars)?;
            if scalars.len() != phones.len() || scalars.iter().any(|value| !value.is_finite()) {
                return Err(VoiceError::new(
                    "Compatibility duration did not produce one finite scalar per phone",
                ));
            }
            scalars
        };
        Ok(DurationPrediction {
            phones,
            scalars,

            impulse_values: Vec::new(),
            impulse_positions: Vec::new(),
        })
    }
}
