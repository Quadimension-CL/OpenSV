use crate::synthesis::voice::{
    DurationConfiguration, PreparedTree, Result, TensorStorage, VoiceError,
};

use super::frontend::PhoneFeatures;

pub(super) fn encode(
    tree: &PreparedTree,
    children: &[usize],
    configuration: DurationConfiguration,
    phones: &[PhoneFeatures],
    constant: &[f32],
) -> Result<Vec<f32>> {
    let width = configuration.feature_width(constant.len())?;
    let capacity = width
        .checked_mul(phones.len())
        .ok_or_else(|| VoiceError::new("Duration feature buffer size overflows"))?;
    let mut output = Vec::with_capacity(capacity);
    for phone in phones {
        embedding(
            tree,
            children[3],
            phone.unified_phone_index,
            configuration.phone_embedding_width as usize,
            &mut output,
        )?;
        embedding(
            tree,
            children[2],
            phone.language_index,
            configuration.language_embedding_width as usize,
            &mut output,
        )?;
        embedding(
            tree,
            children[4],
            phone.left_index.min(7),
            configuration.position_embedding_width as usize,
            &mut output,
        )?;
        embedding(
            tree,
            children[4],
            phone.right_index.min(7),
            configuration.position_embedding_width as usize,
            &mut output,
        )?;
        sinusoidal(
            phone.duration,
            configuration.duration_encoding_width as usize,
            &mut output,
        )?;
        sinusoidal(
            phone.pitch,
            configuration.pitch_encoding_width as usize,
            &mut output,
        )?;
        output.extend_from_slice(constant);
    }
    Ok(output)
}

pub(crate) fn embedding(
    tree: &PreparedTree,
    node: usize,
    index: usize,
    width: usize,
    output: &mut Vec<f32>,
) -> Result<()> {
    let module = tree.node(node)?;
    let [matrix] = module.children.as_slice() else {
        return Err(VoiceError::new("Duration embedding requires one matrix"));
    };
    let TensorStorage::FloatMatrix {
        rows,
        columns,
        values,
        ..
    } = tree.tensor(*matrix)?
    else {
        return Err(VoiceError::new("Duration embedding is not a float matrix"));
    };
    if *rows != width || *columns == 0 {
        return Err(VoiceError::new(
            "Duration embedding dimensions do not match root configuration",
        ));
    }

    let index = index.min(columns - 1);
    output.extend((0..width).map(|row| values[row * columns + index]));
    Ok(())
}

pub(crate) fn sinusoidal(value: f32, width: usize, output: &mut Vec<f32>) -> Result<()> {
    if !value.is_finite() || width < 2 || !width.is_multiple_of(2) {
        return Err(VoiceError::new("Invalid duration sinusoidal feature"));
    }
    let start = output.len();
    output.resize(start + width, 0.0);
    let half = width / 2;
    for index in 0..half {

        let divisor = 10000.0_f32.powf((2 * index) as f32 / width as f32);
        let (sin, cos) = (value / divisor).sin_cos();
        output[start + index] = sin;
        output[start + half + index] = cos;
    }
    Ok(())
}
