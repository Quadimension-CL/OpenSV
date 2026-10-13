use crate::synthesis::{
    inference::Sequence,
    stages::timbre::{TimbreRequest, TimbreStyles},
    voice::{Result, VoiceError},
};

pub(super) fn prepare(
    styles: &TimbreStyles,
    request: &TimbreRequest<'_>,
    frames: usize,
) -> Result<Option<Sequence>> {
    for weights in [
        request.primary_style_weights,
        request.secondary_style_weights,
    ]
    .into_iter()
    .flatten()
    {
        weights.validate()?;
        if weights.frames != frames
            || weights.channels != styles.variants.len()
            || weights.values.iter().any(|value| !value.is_finite())
        {
            return Err(VoiceError::new(
                "DDS style weights do not match the acoustic timeline or voice styles",
            ));
        }
    }
    if request
        .primary_style_weights
        .is_some_and(|weights| weights.values.iter().any(|&weight| weight != 0.0))
    {
        return Err(VoiceError::new(
            "DDS voices do not support pronunciation style weights",
        ));
    }
    let Some(weights) = request.secondary_style_weights else {
        return Ok(None);
    };
    if weights.values.iter().all(|&weight| weight == 0.0) {
        return Ok(None);
    }
    let mut output = Sequence::zeros(frames, styles.neutral.len())?;
    for (row, weights) in output
        .values
        .chunks_exact_mut(styles.neutral.len())
        .zip(weights.values.chunks_exact(weights.channels))
    {
        row.copy_from_slice(&styles.blend(weights)?);
    }
    Ok(Some(output))
}
