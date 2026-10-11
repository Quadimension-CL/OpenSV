//! Temporal refinement topology for acoustic child 13.
//!
//! Children are a conditional stack, temporal upsampling sequence, then a
//! channel affine. The acoustic stream retains the queues joining these stages.

use crate::synthesis::{
    inference::ops::{
        SEQUENTIAL_TYPE_ID,
        conditional::{CONDITIONAL_TYPE_ID, ConditionalConfig},
        normalization::CHANNEL_AFFINE_TYPE_ID,
    },
    voice::{PreparedTree, Result, TensorStorage, VoiceError},
};

pub const REFINEMENT_TYPE_ID: [u8; 16] = [
    0x03, 0x4c, 0x11, 0x4e, 0x23, 0x87, 0x8d, 0x3f, 0x57, 0x7f, 0x57, 0xeb, 0x57, 0x66, 0xb7, 0xb7,
];

pub struct Refinement {
    pub(super) conditional: usize,
    pub(super) upsample: usize,
    pub(super) channel_affine: usize,
    input_channels: usize,
    latent_channels: usize,
    output_channels: usize,
}

impl Refinement {
    pub fn prepare(tree: &PreparedTree, index: usize) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("timbre.refinement.prepare");
        let node = tree.node(index)?;
        if node.type_id != REFINEMENT_TYPE_ID || node.payload_bytes != 0 {
            return Err(VoiceError::new("Invalid timbre refinement module"));
        }
        let [conditional, upsample, channel_affine] = node.children.as_slice() else {
            return Err(VoiceError::new("Timbre refinement requires three children"));
        };
        if tree.node(*conditional)?.type_id != CONDITIONAL_TYPE_ID
            || tree.node(*upsample)?.type_id != SEQUENTIAL_TYPE_ID
            || tree.node(*channel_affine)?.type_id != CHANNEL_AFFINE_TYPE_ID
        {
            return Err(VoiceError::new("Unexpected timbre refinement child types"));
        }
        let config =
            ConditionalConfig::parse(&tree.payload(*conditional)?).map_err(VoiceError::new)?;
        if config.input_channels != config.condition_channels {
            return Err(VoiceError::new(
                "Timbre refinement uses the same features for input and condition",
            ));
        }
        let affine = tree.node(*channel_affine)?;
        let [scale, bias] = affine.children.as_slice() else {
            return Err(VoiceError::new(
                "Refinement channel affine needs scale and bias",
            ));
        };
        let output_channels = match (tree.tensor(*scale)?, tree.tensor(*bias)?) {
            (
                TensorStorage::FloatVector { values: scale },
                TensorStorage::FloatVector { values: bias },
            ) if !scale.is_empty() && scale.len() == bias.len() => scale.len(),
            _ => return Err(VoiceError::new("Invalid refinement channel affine vectors")),
        };
        Ok(Self {
            conditional: *conditional,
            upsample: *upsample,
            channel_affine: *channel_affine,
            input_channels: config.input_channels,
            latent_channels: config.output_channels,
            output_channels,
        })
    }

    pub fn input_channels(&self) -> usize {
        self.input_channels
    }

    pub fn latent_channels(&self) -> usize {
        self.latent_channels
    }

    pub fn output_channels(&self) -> usize {
        self.output_channels
    }
}
