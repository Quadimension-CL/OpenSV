mod controls;
mod inputs;

use crate::synthesis::{
    inference::{ModelRuntime, Sequence, ops::standard_normalizer::StandardNormalizer},
    stages::f0::{frontend::FrontendNote, statistics::SamplerStatistics},
    voice::{PreparedTree, Result, VoiceError, embedded::types::F0_SAMPLER_TYPE_ID},
};

pub(super) struct SampleResult {
    pub pitch: Vec<f32>,
    pub predicted_controls: SamplerStatistics,
}

pub(super) struct Request<'a> {
    pub source: &'a Sequence,
    pub context: &'a Sequence,
    pub positions: &'a [[f32; 3]],
    pub noise: &'a [f32],
    pub expression: &'a [f32],
    pub notes: &'a [FrontendNote],
    pub note_frames: &'a [usize],
    pub vowel_mask: &'a [bool],
    pub normalizer: &'a StandardNormalizer,
    pub transpose: f32,
    pub contract: super::Contract,
}

pub(super) fn sample(
    tree: &PreparedTree,
    index: usize,
    request: Request<'_>,
) -> Result<SampleResult> {
    let Request {
        source,
        context,
        positions,
        noise,
        expression,
        notes,
        note_frames,
        vowel_mask,
        normalizer,
        transpose,
        contract,
    } = request;
    let config = Configuration::prepare(tree, index)?;
    source.validate()?;
    context.validate()?;
    if source.frames == 0
        || context.frames != source.frames
        || positions.len() != source.frames
        || noise.len() != source.frames
        || expression.len() != source.frames
    {
        return Err(VoiceError::new(
            "SV1 pitch sampler inputs have incompatible frame counts",
        ));
    }
    let children = &tree.node(index)?.children;
    let mut runtime = ModelRuntime::new(tree);
    let scalar = runtime.execute(children[0], source, true)?;
    require_shape(&scalar, source.frames, 1, "source projection")?;
    let packed = inputs::pack_scalar(&scalar, config.group_frames)?;
    let base = inputs::group_mean(context, config.group_frames)?;
    #[cfg(feature="acoustic-diagnostic")]
    for (label,value) in [("compatible_f0_scalar",&scalar),("compatible_f0_packed",&packed),("compatible_f0_base",&base)] {
        crate::synthesis::stages::f0::diagnostic::capture(label.into(),value)?;
    }
    let mut condition = inputs::condition(
        &base,
        positions,
        noise,
        config.group_frames,
        config.positional_width,
        config.positional_scale,
        tree.legacy_numeric(),
    )?;
    #[cfg(feature="acoustic-diagnostic")]
    crate::synthesis::stages::f0::diagnostic::capture("compatible_f0_first_condition".into(),&condition)?;
    let first = runtime.execute_conditioned(children[5], &packed, &condition, true)?;
    #[cfg(feature="acoustic-diagnostic")]
    crate::synthesis::stages::f0::diagnostic::capture("compatible_f0_first_hidden".into(),&first)?;
    let preliminary = runtime.execute(
        children[7],
        &inputs::repeat_frames(&first, source.frames, config.group_frames)?,
        true,
    )?;
    require_shape(&preliminary, source.frames, 1, "preliminary pitch head")?;
    #[cfg(feature="acoustic-diagnostic")]
    crate::synthesis::stages::f0::diagnostic::capture("compatible_f0_preliminary".into(),&preliminary)?;
    let preliminary = preliminary
        .values
        .iter()
        .map(|&value| {
            normalizer
                .denormalize_scalar(2, value)
                .map_err(|error| VoiceError::new(error.to_string()))
        })
        .collect::<Result<Vec<_>>>()?;
    if preliminary.iter().any(|value| !value.is_finite()) {
        return Err(VoiceError::new("DDS preliminary pitch is not finite"));
    }
    let predicted_controls = super::statistics::estimate(
        &preliminary,
        vowel_mask,
        notes,
        note_frames,
        transpose,
        contract,
    )?;

    let mut expression = inputs::group_mean(
        &Sequence::new(source.frames, 1, expression.to_vec())?,
        config.group_frames,
    )?;
    for value in &mut expression.values {
        *value = value.clamp(-0.999, 0.999);
    }
    let mut embedding = runtime.execute(children[1], &expression, true)?;
    require_shape(
        &embedding,
        base.frames,
        base.channels,
        "expression projection",
    )?;
    if let Some(controls) = controls::prepare(notes, note_frames, &predicted_controls, contract)? {
        for (index, values, weights) in [
            (2, controls.slope, controls.slope_weight),
            (3, controls.offset, controls.offset_weight),
        ] {
            let values = inputs::group_mean(&values, config.group_frames)?;
            let weights = inputs::group_mean(&weights, config.group_frames)?;
            let projected = runtime.execute(children[index], &values, true)?;
            require_shape(
                &projected,
                base.frames,
                base.channels,
                "pitch control projection",
            )?;
            for (frame, &weight) in weights.values.iter().enumerate() {
                let start = frame * embedding.channels;
                for (value, &adjustment) in embedding.values[start..start + embedding.channels]
                    .iter_mut()
                    .zip(projected.frame(frame)?)
                {
                    *value += adjustment * weight;
                }
            }
        }
    }
    let adjustment_input = Sequence::concat_channels(&[&base, &embedding])?;
    let mut adjustment = runtime.execute(children[4], &adjustment_input, true)?;
    require_shape(
        &adjustment,
        base.frames,
        base.channels,
        "context adjustment",
    )?;
    for (adjusted, original) in adjustment.values.iter_mut().zip(&base.values) {
        *adjusted += original;
    }
    for frame in 0..condition.frames {
        let start = frame * condition.channels;
        condition.values[start..start + base.channels].copy_from_slice(adjustment.frame(frame)?);
    }

    let final_hidden = runtime.execute_conditioned(children[6], &first, &condition, true)?;
    #[cfg(feature="acoustic-diagnostic")]
    for (label,value) in [("compatible_f0_final_condition",&condition),("compatible_f0_final_hidden",&final_hidden)] {
        crate::synthesis::stages::f0::diagnostic::capture(label.into(),value)?;
    }
    let pitch = runtime.execute(
        children[8],
        &inputs::repeat_frames(&final_hidden, source.frames, config.group_frames)?,
        true,
    )?;
    require_shape(&pitch, source.frames, 1, "final pitch head")?;
    #[cfg(feature="acoustic-diagnostic")]
    crate::synthesis::stages::f0::diagnostic::capture("compatible_normalized_head".into(), &pitch)?;
    Ok(SampleResult {
        pitch: pitch.values,
        predicted_controls,
    })
}

struct Configuration {
    positional_width: usize,
    positional_scale: f32,
    group_frames: usize,
}

impl Configuration {
    fn prepare(tree: &PreparedTree, index: usize) -> Result<Self> {
        let node = tree.node(index)?;
        if node.type_id != F0_SAMPLER_TYPE_ID || node.children.len() != 9 {
            return Err(VoiceError::new(
                "Unsupported SV1 grouped pitch sampler topology",
            ));
        }
        let payload = tree.payload(index)?;
        if payload.len() != 12 {
            return Err(VoiceError::new(
                "Invalid SV1 pitch sampler configuration size",
            ));
        }
        let positional_width = u32::from_le_bytes(payload[0..4].try_into().unwrap()) as usize;
        let positional_scale = f32::from_le_bytes(payload[4..8].try_into().unwrap());
        let group_frames = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
        if positional_width < 2
            || !positional_width.is_multiple_of(2)
            || !positional_scale.is_finite()
            || group_frames == 0
        {
            return Err(VoiceError::new("Invalid SV1 pitch sampler dimensions"));
        }
        Ok(Self {
            positional_width,
            positional_scale,
            group_frames,
        })
    }
}

fn require_shape(sequence: &Sequence, frames: usize, channels: usize, stage: &str) -> Result<()> {
    if sequence.frames != frames || sequence.channels != channels {
        return Err(VoiceError::new(format!(
            "SV1 pitch {stage} returned {} x {}, expected {frames} x {channels}",
            sequence.frames, sequence.channels,
        )));
    }
    Ok(())
}
