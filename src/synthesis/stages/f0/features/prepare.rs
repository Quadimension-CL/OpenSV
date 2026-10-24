use std::sync::Arc;

use crate::synthesis::{
    inference::{
        matrix::DenseMatrix,
        ops::embedding::{EMBEDDING_TYPE_ID, Embedding},
    },
    voice::{PreparedModule, PreparedTree, Result, TensorStorage, VoiceError},
};

use super::{EMPTY_SLOT_TYPE_ID, FEATURE_TYPE_ID, PHONETIC_FEATURE_TYPE_ID, FeatureConfiguration, FeatureNetwork,phones::PhoneEncoder};

impl<'a> FeatureNetwork<'a> {
    pub fn prepare(tree: &'a PreparedTree, index: usize) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("features.prepare");
        let node = tree.node(index)?;
        if node.type_id != FEATURE_TYPE_ID && node.type_id!=PHONETIC_FEATURE_TYPE_ID {
            return Err(VoiceError::new(
                "Expected the shared note/phone feature module",
            ));
        }
        let children: [usize; 10] = node
            .children
            .as_slice()
            .try_into()
            .map_err(|_| VoiceError::new("Feature module requires ten children"))?;
        let payload = tree.payload(index)?;
        if payload.len() != 16 {
            return Err(VoiceError::new(
                "Feature module requires eight uint16 configuration fields",
            ));
        }
        let words: Vec<usize> = payload
            .as_chunks::<2>()
            .0
            .iter()
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) as usize)
            .collect();
        if words[2..].contains(&0) {
            return Err(VoiceError::new("Feature dimensions must be nonzero"));
        }
        if node.type_id==PHONETIC_FEATURE_TYPE_ID {
            return super::phonetic_prepare::prepare(tree,children,&words);
        }
        let configuration = FeatureConfiguration {
            style_channels: words[0],
            condition_channels: words[1],
            phone_embedding_channels: words[2],
            language_embedding_channels: words[3],
            note_channels: words[4],
            phone_channels: words[5],
            position_channels: words[6],
            frame_channels: words[7],
        };
        let phone_embedding = prepare_embedding(tree, children[0])?;
        let language_embedding = prepare_embedding(tree, children[1])?;
        if phone_embedding.features() != words[2] || language_embedding.features() != words[3] {
            return Err(VoiceError::new(
                "Feature embeddings disagree with configured dimensions",
            ));
        }
        let (note_input_channels, note_output_channels) = affine_shape(tree, children[2])?;
        if note_output_channels != words[4] {
            return Err(VoiceError::new(
                "Feature note projection width disagrees with configuration",
            ));
        }
        if words[0] == 0 {
            if !is_empty_slot(tree, children[3])? {
                return Err(VoiceError::new(
                    "Zero-width style input requires an empty ownership slot",
                ));
            }
        } else if affine_shape(tree, children[3])? != (words[0], words[4]) {
            return Err(VoiceError::new(
                "Feature style projection dimensions disagree",
            ));
        }
        if affine_shape(tree, children[4])? != (words[2] + words[3] + words[4], words[5]) {
            return Err(VoiceError::new(
                "Feature phone projection dimensions disagree",
            ));
        }
        let expanded_width =
            note_input_channels + words[4] + words[5] + 2 * words[6] + words[0] + words[1];
        if affine_shape(tree, children[7])? != (expanded_width, words[7]) {
            return Err(VoiceError::new(
                "Feature frame projection dimensions disagree",
            ));
        }
        Ok(Self {
            tree,
            children,
            configuration,
            note_input_channels,
            phone_encoder:PhoneEncoder::Categorical(phone_embedding),
            language_embedding,
        })
    }
}

pub(super) fn is_empty_slot(tree: &PreparedTree, index: usize) -> Result<bool> {
    let node = tree.node(index)?;
    if node.type_id != EMPTY_SLOT_TYPE_ID {
        return Ok(false);
    }
    if !node.children.is_empty() || node.payload_bytes != 0 {
        return Err(VoiceError::new(
            "Empty ownership slot contains unexpected data",
        ));
    }
    Ok(true)
}

pub(super) fn affine_shape(tree: &PreparedTree, index: usize) -> Result<(usize, usize)> {
    if let PreparedModule::Affine(affine) = &tree.node(index)?.module {
        Ok((affine.input_features, affine.output_features))
    } else {
        Err(VoiceError::new(
            "Feature projection is not a prepared affine layer",
        ))
    }
}

pub(super) fn prepare_embedding(tree: &PreparedTree, index: usize) -> Result<Embedding> {
    let node = tree.node(index)?;
    if node.type_id != EMBEDDING_TYPE_ID || node.children.len() != 1 {
        return Err(VoiceError::new(
            "Feature vocabulary requires one embedding matrix",
        ));
    }
    let matrix = match tree.tensor(node.children[0])? {
        TensorStorage::FloatMatrix {
            rows,
            columns,
            values,
            ..
        } => DenseMatrix::from_float(*rows, *columns, Arc::clone(values)),
        TensorStorage::QuantizedMatrix {
            rows,
            columns,
            bits,
            input_exponent,
            scales,
            values,
            ..
        } => DenseMatrix::from_quantized(*rows, *columns, *bits, *input_exponent, scales, values),
        _ => return Err(VoiceError::new("Feature vocabulary weight is not a matrix")),
    }
    .map_err(|error| VoiceError::new(error.to_string()))?;
    Ok(Embedding::new(&matrix))
}
