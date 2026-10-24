use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

pub(super) fn pack_scalar(input: &Sequence, group: usize) -> Result<Sequence> {
    let mut output = Sequence::zeros(input.frames.div_ceil(group), group)?;
    for (index, value) in output.values.iter_mut().enumerate() {
        *value = input.values[index.min(input.frames - 1)];
    }
    Ok(output)
}

pub(super) fn group_mean(input: &Sequence, group: usize) -> Result<Sequence> {
    let mut output = Sequence::zeros(input.frames.div_ceil(group), input.channels)?;
    for frame in 0..output.frames {
        let row = &mut output.values[frame * input.channels..(frame + 1) * input.channels];
        for offset in 0..group {
            for (value, input) in row
                .iter_mut()
                .zip(input.frame((frame * group + offset).min(input.frames - 1))?)
            {
                *value += input;
            }
        }
        for value in row {
            *value /= group as f32;
        }
    }
    Ok(output)
}

pub(super) fn condition(
    context: &Sequence,
    positions: &[[f32; 3]],
    noise: &[f32],
    group: usize,
    width: usize,
    scale: f32,
    legacy_numeric:bool,
) -> Result<Sequence> {
    let channels = width
        .checked_mul(3)
        .and_then(|value| value.checked_add(context.channels))
        .and_then(|value| value.checked_add(group))
        .ok_or_else(|| VoiceError::new("SV1 pitch condition width overflows"))?;
    let mut output = Sequence::zeros(context.frames, channels)?;
    let frequency_step = f32::from_bits(0xc1d4_9a78) / width as f32;
    let frequencies: Vec<f32> = (0..width / 2)
        .map(|index| if legacy_numeric {10000.0_f32.powf((index as f32+index as f32)/width as f32)}
            else {(index as f32 * frequency_step).exp2()})
        .collect();
    for frame in 0..context.frames {
        let row = &mut output.values[frame * channels..(frame + 1) * channels];
        row[..context.channels].copy_from_slice(context.frame(frame)?);
        let mut position_sum = [0.0; 3];
        for offset in 0..group {
            let source = (frame * group + offset).min(positions.len() - 1);
            for (sum, value) in position_sum.iter_mut().zip(positions[source]) {
                *sum += value;
            }
            row[context.channels + 3 * width + offset] = noise[source];
        }
        for (position_index, sum) in position_sum.into_iter().enumerate() {
            let angle = if legacy_numeric {(sum / group as f32)*scale}else{sum * scale / group as f32};
            let start = context.channels + position_index * width;
            for (index, frequency) in frequencies.iter().enumerate() {
                let phase=if legacy_numeric {angle / frequency}else{frequency * angle};
                let (sin, cos) = if legacy_numeric {(phase.sin(),phase.cos())}else{phase.sin_cos()};
                row[start + index] = sin;
                row[start + width / 2 + index] = cos;
            }
        }
    }
    Ok(output)
}

pub(super) fn repeat_frames(input: &Sequence, frames: usize, group: usize) -> Result<Sequence> {
    if input.frames != frames.div_ceil(group) {
        return Err(VoiceError::new(
            "SV1 pitch hidden state has an invalid grouped frame count",
        ));
    }
    let mut output = Sequence::zeros(frames, input.channels)?;
    for frame in 0..frames {
        let start = frame * input.channels;
        output.values[start..start + input.channels].copy_from_slice(input.frame(frame / group)?);
    }
    Ok(output)
}
