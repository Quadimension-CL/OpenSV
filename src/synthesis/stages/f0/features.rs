mod encode;
mod expand;
mod prepare;
mod phones;
mod phonetic_prepare;
mod project;
mod rows;
pub mod phonetic_context;
#[cfg(feature="acoustic-diagnostic")]
pub mod diagnostic;

pub use expand::{ExpandedFeatures, ExpansionInput, FeatureRows};
pub use phones::FeaturePhones;
use phones::PhoneEncoder;
use std::sync::Arc;

use crate::synthesis::{
    inference::ops::embedding::Embedding,
    voice::{PreparedTree, Result, VoiceError},
};

pub const FEATURE_TYPE_ID: [u8; 16] = [
    0xc9, 0x73, 0x5a, 0xd3, 0xe2, 0x78, 0x74, 0xcf, 0x5f, 0x28, 0x2c, 0x5f, 0xdf, 0x8e, 0xff, 0x6a,
];
pub const PHONETIC_FEATURE_TYPE_ID:[u8;16]=[
    0xa1,0xd1,0x0a,0x74,0x1d,0x35,0x42,0x47,
    0x38,0x26,0x6a,0x20,0x0f,0x92,0x44,0x4a,
];

pub(crate) const EMPTY_SLOT_TYPE_ID: [u8; 16] = [
    0xd8, 0x9e, 0x1b, 0x8a, 0x6c, 0x01, 0xd9, 0x3d, 0x18, 0xa8, 0xd1, 0xa7, 0x40, 0x1e, 0x1c, 0x0e,
];

#[derive(Clone, Copy, Debug)]
pub struct FeatureConfiguration {
    pub style_channels: usize,
    pub condition_channels: usize,
    pub phone_embedding_channels: usize,
    pub language_embedding_channels: usize,
    pub note_channels: usize,
    pub phone_channels: usize,
    pub position_channels: usize,
    pub frame_channels: usize,
}

pub struct FeatureNetwork<'a> {
    tree: &'a PreparedTree,
    children: [usize; 10],
    configuration: FeatureConfiguration,
    note_input_channels: usize,
    phone_encoder: PhoneEncoder,
    language_embedding: Embedding,
}

pub struct FeatureInputs<'a> {

    pub notes: &'a [f32],
    pub style: Option<&'a [f32]>,
    pub phone_to_note: &'a [usize],
    pub phones: &'a FeaturePhones,
}

#[derive(Clone, Debug)]
pub struct FeatureEncoding {
    pub note_count: usize,
    pub note_channels: usize,
    pub notes: Arc<[f32]>,
    pub phone_count: usize,
    pub phone_channels: usize,
    pub phones: Arc<[f32]>,
}

pub struct FrameFeatureInputs<'a> {
    pub notes: &'a [f32],
    pub style: Option<&'a [f32]>,
    pub encoding: &'a FeatureEncoding,
    pub note_durations: &'a [u32],
    pub note_positions: &'a [u32],
    pub phone_durations: &'a [u32],
}

pub struct FrameConditioning {
    pub frames: usize,

    pub pointwise: Option<Vec<f32>>,

    pub convolutional: Vec<f32>,
}

impl FeatureNetwork<'_> {

    pub(super) fn shared_frame_source(&self) -> bool {
        self.phonetic()
    }

    pub fn configuration(&self) -> FeatureConfiguration {
        self.configuration
    }

    pub fn expand_encoding(
        &self,
        input: &FrameFeatureInputs<'_>,
        condition: Option<FeatureRows<'_>>,
    ) -> Result<ExpandedFeatures> {
        let config = self.configuration;
        input
            .encoding
            .validate(config.note_channels, config.phone_channels)?;
        self.expand(
            &ExpansionInput {
                first_primary: FeatureRows {
                    rows: input.encoding.note_count,
                    width: config.note_channels,
                    values: &input.encoding.notes,
                },
                first_context: FeatureRows {
                    rows: input.encoding.note_count,
                    width: self.note_input_channels,
                    values: input.notes,
                },
                optional: input.style.map(|values| FeatureRows {
                    rows: input.encoding.note_count,
                    width: config.style_channels,
                    values,
                }),
                first_durations: input.note_durations,
                first_positions: input.note_positions,
                second: FeatureRows {
                    rows: input.encoding.phone_count,
                    width: config.phone_channels,
                    values: &input.encoding.phones,
                },
                second_durations: input.phone_durations,
            },
            condition,
        )
    }

    pub fn expand(
        &self,
        input: &ExpansionInput<'_>,
        condition: Option<FeatureRows<'_>>,
    ) -> Result<ExpandedFeatures> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("features.expand");
        let mut output = expand::expand(
            expand::ExpansionSpec {
                position_channels: self.configuration.position_channels,
                optional_channels: self.configuration.style_channels,
                condition_channels: self.configuration.condition_channels,
            },
            input,
        )?;
        let width = self.configuration.condition_channels;
        match condition {
            None if width == 0 => {}
            Some(condition) if width != 0 => {
                if condition.rows != output.frames
                    || condition.width != width
                    || condition.rows.checked_mul(width) != Some(condition.values.len())
                {
                    return Err(VoiceError::new(
                        "Feature frame condition has an incompatible shape",
                    ));
                }
                for (row, values) in output
                    .full
                    .chunks_exact_mut(output.full_width)
                    .zip(condition.values.chunks_exact(width))
                {
                    row[output.full_width - width..].copy_from_slice(values);
                }
            }
            _ => {
                return Err(VoiceError::new(
                    "Feature frame condition presence differs from its model",
                ));
            }
        }
        #[cfg(feature="acoustic-diagnostic")]
        diagnostic::capture_frames(self.tree,&output)?;
        Ok(output)
    }
}
