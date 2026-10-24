use super::frontend::Encoded;
use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    voice::{PreparedTree, Result, VoiceError, embedded::types::F0_FEATURES_TYPE_ID},
};

pub(super) struct Features {
    pub source: Sequence,
    pub context: Sequence,
}

pub(super) fn execute(
    tree: &PreparedTree,
    index: usize,
    input: &Encoded,
    note_frames: &[usize],
    phone_frames: &[usize],
    style: &[f32],
) -> Result<Features> {
    let node = tree.node(index)?;
    let payload = tree.payload(index)?;
    if node.type_id != F0_FEATURES_TYPE_ID
        || node.children.len() != 7
        || payload.len() != 4
        || u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize != style.len()
        || note_frames.len() != input.notes.frames
        || phone_frames.len() != input.phone_ids.len()
    {
        return Err(VoiceError::new(
            "Invalid compatible F0 feature program dimensions",
        ));
    }
    let children = &node.children;
    let mut runtime = ModelRuntime::new(tree);
    let notes = runtime.execute(children[2], &input.notes, true)?;
    #[cfg(feature="acoustic-diagnostic")]
    {
        crate::synthesis::stages::f0::diagnostic::capture("compatible_feature_input_notes".into(), &input.notes)?;
        crate::synthesis::stages::f0::diagnostic::capture("compatible_feature_notes".into(), &notes)?;
        crate::synthesis::stages::f0::diagnostic::capture("compatible_feature_style".into(), &Sequence::new(1,style.len(),style.to_vec())?)?;
    }
    let phone_ids = Sequence::from_values(1, input.phone_ids.clone())?;
    let embedded_phones = runtime.execute(children[0], &phone_ids, true)?;
    let phones = runtime.execute(children[3], &embedded_phones, true)?;
    let languages = runtime.execute(
        children[1],
        &Sequence::from_values(1, input.language_ids.clone())?,
        true,
    )?;
    if notes.frames != note_frames.len()
        || phones.frames != phone_frames.len()
        || languages.frames != phone_frames.len()
    {
        return Err(VoiceError::new(
            "Compatible F0 context encoder changed record counts",
        ));
    }
    let note_total = total(note_frames)?;
    let phone_total = total(phone_frames)?;
    let frames = note_total.max(phone_total);
    if frames == 0 || languages.frames == 0 {
        return Err(VoiceError::new("Compatible F0 feature sequence is empty"));
    }
    let width = notes.channels + phones.channels + 8 + style.len() + languages.channels;
    let mut expanded = Sequence::zeros(frames, width)?;
    let mut frame = 0;
    for (index, &count) in note_frames.iter().enumerate() {
        for row in expanded.values[frame * width..(frame + count) * width].chunks_exact_mut(width) {
            row[..notes.channels].copy_from_slice(notes.frame(index)?);
        }
        frame += count;
    }
    let phone_offset = notes.channels;
    let position_offset = phone_offset + phones.channels;
    let style_offset = position_offset + 8;
    let language_offset = style_offset + style.len();
    frame = 0;
    let mut last_language = vec![0.0; languages.channels];
    for (index, &count) in phone_frames.iter().enumerate() {
        for relative in 0..count {
            let row =
                &mut expanded.values[(frame + relative) * width..(frame + relative + 1) * width];
            row[phone_offset..position_offset].copy_from_slice(phones.frame(index)?);
            let position = if count > 1 {
                relative as f32 / (count - 1) as f32
            } else {
                0.5
            };
            for channel in 0..8 {
                row[position_offset + channel] = 0.5
                    * (std::f32::consts::TAU * (position + (1.0 - channel as f32) / 8.0)).cos()
                    + 0.5;
            }
            last_language.copy_from_slice(languages.frame(index)?);
            row[language_offset..].copy_from_slice(&last_language);
        }
        frame += count;
    }
    for row in expanded.values[phone_total * width..].chunks_exact_mut(width) {
        row[language_offset..].copy_from_slice(&last_language);
    }
    for row in expanded.values.chunks_exact_mut(width) {
        row[style_offset..language_offset].copy_from_slice(style);
    }
    let shared = runtime.execute(children[4], &expanded, true)?;
    #[cfg(feature="acoustic-diagnostic")]
    {
        crate::synthesis::stages::f0::diagnostic::capture("compatible_feature_expanded".into(), &expanded)?;
        crate::synthesis::stages::f0::diagnostic::capture("compatible_feature_shared".into(), &shared)?;
    }
    let source = runtime.execute(children[5], &shared, true)?;
    let context = runtime.execute(children[6], &shared, true)?;
    #[cfg(feature="acoustic-diagnostic")]
    {
        crate::synthesis::stages::f0::diagnostic::capture("compatible_feature_source".into(), &source)?;
        crate::synthesis::stages::f0::diagnostic::capture("compatible_feature_context".into(), &context)?;
    }
    if source.frames != frames || context.frames != frames {
        return Err(VoiceError::new(
            "Compatible F0 feature branches changed frame counts",
        ));
    }
    Ok(Features { source, context })
}

fn total(values: &[usize]) -> Result<usize> {
    values
        .iter()
        .try_fold(0_usize, |total, count| total.checked_add(*count))
        .ok_or_else(|| VoiceError::new("Compatible F0 expanded frame count overflow"))
}
