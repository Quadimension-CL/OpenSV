use crate::synthesis::{
    stages::duration::{PhoneFeatures, encoding::embedding},
    voice::{PreparedTree, Result, VoiceError},
};

use super::configuration::Configuration;

pub(super) fn encode(
    tree: &PreparedTree,
    children: &[usize; 6],
    configuration: Configuration,
    phones: &[PhoneFeatures],
) -> Result<Vec<f32>> {
    let constant = tree
        .tensor(children[5])?
        .vector()
        .ok_or_else(|| VoiceError::new("SV1 duration constant is not a float vector"))?;
    let capacity = configuration
        .width
        .checked_mul(phones.len())
        .ok_or_else(|| VoiceError::new("SV1 duration feature buffer overflows"))?;
    let mut output = Vec::with_capacity(capacity);
    for phone in phones {
        embedding(
            tree,
            children[3],
            phone.unified_phone_index,
            configuration.phone,
            &mut output,
        )?;
        embedding(
            tree,
            children[2],
            phone.language_index,
            configuration.language,
            &mut output,
        )?;
        embedding(
            tree,
            children[4],
            phone.left_index.min(7),
            configuration.position,
            &mut output,
        )?;
        embedding(
            tree,
            children[4],
            phone.right_index.min(7),
            configuration.position,
            &mut output,
        )?;
        sinusoidal(phone.duration, configuration.duration, &mut output);
        sinusoidal(phone.pitch, configuration.pitch, &mut output);
        output.extend_from_slice(constant);
    }
    Ok(output)
}

fn sinusoidal(value: f32, width: usize, output: &mut Vec<f32>) {
    // SV1 computes this scale with exp2 and an f32 exponent coefficient.
    let scale = -26.575_424_f32 / width as f32;
    let start = output.len();
    output.resize(start + width, 0.0);
    let half = width / 2;
    for index in 0..half {
        let angle = value * (index as f32 * scale).exp2();
        let (sin, cos) = angle.sin_cos();
        output[start + index] = sin;
        output[start + half + index] = cos;
    }
}
