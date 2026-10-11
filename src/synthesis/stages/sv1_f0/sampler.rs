//! Grouped two-pass DDS pitch sampler with default expression controls.
//!
//! The final payload integer specifies temporal grouping, not a diffusion
//! iteration count. SV1 and SV2 compatibility models share this contract.
mod inputs;

use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    voice::{PreparedTree, Result, VoiceError, embedded::types::F0_SAMPLER_TYPE_ID},
};

pub(super) struct SampleResult {
    pub pitch: Vec<f32>,
    pub preliminary: Vec<f32>,
}

pub(super) fn sample(
    tree: &PreparedTree,
    index: usize,
    source: &Sequence,
    context: &Sequence,
    positions: &[[f32; 3]],
    noise: &[f32],
) -> Result<SampleResult> {
    let config = Configuration::prepare(tree, index)?;
    source.validate()?;
    context.validate()?;
    if source.frames == 0
        || context.frames != source.frames
        || positions.len() != source.frames
        || noise.len() != source.frames
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
    let mut condition = inputs::condition(
        &base,
        positions,
        noise,
        config.group_frames,
        config.positional_width,
        config.positional_scale,
    )?;
    let first = runtime.execute_conditioned(children[5], &packed, &condition, true)?;
    let preliminary = runtime.execute(
        children[7],
        &inputs::repeat_frames(&first, source.frames, config.group_frames)?,
        true,
    )?;
    require_shape(&preliminary, source.frames, 1, "preliminary pitch head")?;

    // Unset slope and offset controls contribute zero, regardless of their
    // predicted statistics. Default expressiveness is exactly zero;
    // its learned projection still contributes and must not be omitted.
    let default_expression = Sequence::zeros(base.frames, 1)?;
    let embedding = runtime.execute(children[1], &default_expression, true)?;
    require_shape(
        &embedding,
        base.frames,
        base.channels,
        "expression projection",
    )?;
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
    let pitch = runtime.execute(
        children[8],
        &inputs::repeat_frames(&final_hidden, source.frames, config.group_frames)?,
        true,
    )?;
    require_shape(&pitch, source.frames, 1, "final pitch head")?;
    Ok(SampleResult {
        pitch: pitch.values,
        preliminary: preliminary.values,
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
