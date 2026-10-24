use super::{
    FeatureConfiguration, FeatureNetwork,
    phones::PhoneEncoder,
    prepare::{affine_shape, is_empty_slot, prepare_embedding},
};
use crate::synthesis::{
    stages::f0::frontend::PHONETIC_NOTE_FRONTEND_TYPE_ID,
    voice::{PhoneticMap, PreparedModule, PreparedTree, Result, VoiceError},
};

pub(super) fn prepare<'a>(
    tree: &'a PreparedTree,
    children: [usize; 10],
    words: &[usize],
) -> Result<FeatureNetwork<'a>> {
    let root = tree.node(tree.root_index())?;
    let frontend = *root
        .children
        .first()
        .ok_or_else(|| VoiceError::new("Missing phonetic frontend"))?;
    let frontend = tree.node(frontend)?;
    if frontend.type_id != PHONETIC_NOTE_FRONTEND_TYPE_ID || frontend.children.len() != 3 {
        return Err(VoiceError::new(
            "Phonetic feature program requires its named linguistic frontend",
        ));
    }
    let map = PhoneticMap::parse(&tree.resource_bytes(frontend.children[0])?)?;
    let config = FeatureConfiguration {
        style_channels: words[0],
        condition_channels: words[1],
        phone_embedding_channels: words[2],
        language_embedding_channels: words[3],
        note_channels: words[4],
        phone_channels: words[7],
        position_channels: words[6],
        frame_channels: words[5],
    };
    if affine_shape(tree, children[0])? != (73, config.phone_embedding_channels) {
        return Err(VoiceError::new(
            "Phonetic phone projection dimensions disagree",
        ));
    }
    let language_embedding = prepare_embedding(tree, children[1])?;
    if language_embedding.features() != config.language_embedding_channels {
        return Err(VoiceError::new(
            "Phonetic language projection dimensions disagree",
        ));
    }
    let (note_input_channels, note_output) = affine_shape(tree, children[2])?;
    if note_output != config.note_channels
        || affine_shape(tree, children[4])?
            != (
                config.phone_embedding_channels
                    + config.language_embedding_channels
                    + config.note_channels,
                config.phone_channels,
            )
    {
        return Err(VoiceError::new(
            "Phonetic note/phone context dimensions disagree",
        ));
    }
    if config.style_channels == 0 {
        if !is_empty_slot(tree, children[3])? {
            return Err(VoiceError::new(
                "Absent phonetic style requires an empty slot",
            ));
        }
    } else if affine_shape(tree, children[3])? != (config.style_channels, config.note_channels)
        || config.note_channels != config.phone_channels
    {
        return Err(VoiceError::new(
            "Phonetic style projection dimensions disagree",
        ));
    }
    if !is_empty_slot(tree, children[7])? || !is_empty_slot(tree, children[8])? {
        return Err(VoiceError::new(
            "Unexpected phonetic frame projection ownership",
        ));
    }
    let expanded = note_input_channels
        + config.note_channels
        + config.phone_channels
        + 2 * config.position_channels
        + config.style_channels
        + config.condition_channels;
    match &tree.node(children[9])?.module {
        PreparedModule::Conditional(head)
            if head.input_channels == expanded
                && head.output_channels == config.frame_channels
                && head.condition_channels == 0 => {}
        _ => {
            return Err(VoiceError::new(
                "Phonetic expanded frame program dimensions disagree",
            ));
        }
    }
    Ok(FeatureNetwork {
        tree,
        children,
        configuration: config,
        note_input_channels,
        phone_encoder: PhoneEncoder::Phonetic(map),
        language_embedding,
    })
}
