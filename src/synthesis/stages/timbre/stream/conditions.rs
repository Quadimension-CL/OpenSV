use super::super::sampling::AcousticSampler;
use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

pub(super) fn ppg(
    sampler: &AcousticSampler,
    prior: &Sequence,
    noise: &Sequence,
    controls: &Sequence,
    shape: &[f32],
    intensity: &[f32],
) -> Result<Sequence> {
    let c = sampler.config;
    let frames = prior.frames;
    check(prior, frames, c.ppg_prior, "PPG prior")?;
    check(noise, frames, c.ppg_noise, "PPG noise")?;
    check(controls, frames, c.controls, "Downsampled PPG controls")?;
    check_scalar(shape, frames, "PPG shape")?;
    check_scalar(intensity, frames, "PPG intensity")?;
    let width = c.ppg_prior + 2 * c.embedding + c.ppg_noise + c.controls;
    let mut condition = Sequence::zeros(frames, width)?;
    for (index, frame) in condition.values.chunks_exact_mut(width).enumerate() {
        let mut offset = 0;
        copy_frame(frame, &mut offset, prior, index);
        c.embedding(shape[index], &mut frame[offset..offset + c.embedding]);
        offset += c.embedding;
        c.embedding(intensity[index], &mut frame[offset..offset + c.embedding]);
        offset += c.embedding;
        copy_frame(frame, &mut offset, noise, index);
        copy_frame(frame, &mut offset, controls, index);
    }
    Ok(condition)
}

pub(super) fn timbre(
    sampler: &AcousticSampler,
    prior: &Sequence,
    noise: &Sequence,
    ppg_hidden: &Sequence,
    intensity: &[f32],
) -> Result<(Sequence, Sequence)> {
    let c = sampler.config;
    let frames = prior.frames;
    check(prior, frames, c.timbre_prior, "Timbre prior")?;
    check(noise, frames, c.timbre_noise, "Timbre noise")?;
    check(ppg_hidden, frames, c.ppg_hidden, "Timbre PPG state")?;
    check_scalar(intensity, frames, "Timbre intensity")?;
    let input_width = c.timbre_noise + c.ppg_hidden;
    let condition_width = c.timbre_prior + c.embedding + c.timbre_noise;
    let mut data = Sequence::zeros(frames, input_width)?;
    let mut condition = Sequence::zeros(frames, condition_width)?;
    for (index, (frame, condition_frame)) in data
        .values
        .chunks_exact_mut(input_width)
        .zip(condition.values.chunks_exact_mut(condition_width))
        .enumerate()
    {
        let mut offset = 0;
        copy_frame(frame, &mut offset, noise, index);
        copy_frame(frame, &mut offset, ppg_hidden, index);
        offset = 0;
        copy_frame(condition_frame, &mut offset, prior, index);
        c.embedding(
            intensity[index],
            &mut condition_frame[offset..offset + c.embedding],
        );
        offset += c.embedding;
        copy_frame(condition_frame, &mut offset, noise, index);
    }
    Ok((data, condition))
}

fn check(input: &Sequence, frames: usize, channels: usize, name: &str) -> Result<()> {
    input.validate()?;
    if input.frames != frames
        || input.channels != channels
        || input.values.iter().any(|value| !value.is_finite())
    {
        return Err(VoiceError::new(format!(
            "{name} must contain finite acoustic condition frames of the configured width"
        )));
    }
    Ok(())
}

fn check_scalar(values: &[f32], frames: usize, name: &str) -> Result<()> {
    if values.len() != frames || values.iter().any(|value| !value.is_finite()) {
        return Err(VoiceError::new(format!(
            "{name} must contain one finite value per acoustic condition frame"
        )));
    }
    Ok(())
}

fn copy_frame(target: &mut [f32], offset: &mut usize, source: &Sequence, index: usize) {
    let width = source.channels;
    target[*offset..*offset + width]
        .copy_from_slice(&source.values[index * width..(index + 1) * width]);
    *offset += width;
}
