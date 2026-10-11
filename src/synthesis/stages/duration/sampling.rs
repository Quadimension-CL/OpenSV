//! Select the deterministic or noise-conditioned duration head per record.

use super::PhoneFeatures;
use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    voice::{PreparedTree, Result, VoiceError},
};

mod noise;

pub(super) fn predict(
    tree: &PreparedTree,
    children: &[usize; 14],
    encoded: &[f32],
    width: usize,
    phones: &[PhoneFeatures],
    seeds: Option<&[u32]>,
) -> Result<Vec<f32>> {
    let defaults = tree.execute_node(children[8], encoded)?;
    if defaults.len() != phones.len() {
        return Err(VoiceError::new(
            "Default duration prediction frame count changed",
        ));
    }
    let Some(seeds) = seeds else {
        return Ok(defaults);
    };
    if phones.iter().any(|phone| phone.record_index >= seeds.len()) {
        return Err(VoiceError::new(
            "Duration phoneme references a missing seed record",
        ));
    }
    if phones.iter().all(|phone| seeds[phone.record_index] == 0) {
        return Ok(defaults);
    }

    // All phones participate in the sampled context, even when their final
    // output comes from the default head.
    let noise = Sequence::new(phones.len(), 1, noise::generate(phones, seeds)?)?;
    let context = Sequence::new(phones.len(), width, encoded.to_vec())?;
    let mut runtime = ModelRuntime::new(tree);
    let sampled = runtime.execute_conditioned(children[10], &noise, &context, true)?;
    let mut sampled = runtime.execute(children[9], &sampled, true)?;
    if sampled.frames != phones.len() || sampled.channels != 1 {
        return Err(VoiceError::new(
            "Sampled duration prediction is not one scalar per phone",
        ));
    }
    for ((value, default), phone) in sampled.values.iter_mut().zip(defaults).zip(phones) {
        if seeds[phone.record_index] == 0 {
            *value = default;
        }
    }
    Ok(sampled.values)
}
