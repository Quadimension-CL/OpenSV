use crate::synthesis::{
    stages::duration::{
        PhoneFeatures,
        encoding::{embedding, sinusoidal},
    },
    voice::{PreparedTree, Result, VoiceError},
};

use super::configuration::Configuration;

pub(super) fn encode(
    tree: &PreparedTree,
    children: &[usize; 9],
    configuration: Configuration,
    phones: &[PhoneFeatures],
) -> Result<Vec<f32>> {
    let constant = tree
        .tensor(children[1])?
        .vector()
        .ok_or_else(|| VoiceError::new("Compatibility duration constant is not a float vector"))?;
    let width = configuration.feature_width(constant.len())?;
    let capacity = width
        .checked_mul(phones.len())
        .ok_or_else(|| VoiceError::new("Compatibility duration feature buffer size overflows"))?;
    let mut result = Vec::with_capacity(capacity);
    for phone in phones {
        // Share the modern duration frontend's block order and powf-based
        // sinusoidal encoding, without its impulse heads.
        embedding(
            tree,
            children[3],
            phone.unified_phone_index,
            configuration.phone,
            &mut result,
        )?;
        embedding(
            tree,
            children[2],
            phone.language_index,
            configuration.language,
            &mut result,
        )?;
        embedding(
            tree,
            children[4],
            phone.left_index.min(7),
            configuration.position,
            &mut result,
        )?;
        embedding(
            tree,
            children[4],
            phone.right_index.min(7),
            configuration.position,
            &mut result,
        )?;
        sinusoidal(phone.duration, configuration.duration, &mut result)?;
        sinusoidal(phone.pitch, configuration.pitch, &mut result)?;
        result.extend_from_slice(constant);
    }
    Ok(result)
}
