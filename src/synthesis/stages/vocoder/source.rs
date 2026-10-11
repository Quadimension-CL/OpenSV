//! Physical LF excitation inputs, separate from learned spectral parameters.

use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

use super::{condition::CHANNELS, excitation::ExcitationFrame};

/// The root's optional one-channel network predicts the LF shape logit.
/// It is mapped to [0.1, 3] before applying the frame control.
/// F0 and voicing come directly from physical features, before normalization.
pub fn excitation_frames(
    physical: &Sequence,
    shape_logits: &Sequence,
    shape_controls: &[f32],
) -> Result<Vec<ExcitationFrame>> {
    #[cfg(feature = "synth-profile")]
    let _profile = crate::synthesis::profiling::span("vocoder.source.excitation_frames");
    physical.validate()?;
    shape_logits.validate()?;
    if physical.channels != CHANNELS
        || shape_logits.channels != 1
        || shape_logits.frames != physical.frames
        || shape_controls.len() != physical.frames
    {
        return Err(VoiceError::new(
            "Vocoder LF source inputs have different frame shapes",
        ));
    }
    physical
        .values
        .as_chunks::<CHANNELS>()
        .0
        .iter()
        .zip(&shape_logits.values)
        .zip(shape_controls)
        .map(|((frame, &logit), &control)| {
            if !logit.is_finite()
                || !control.is_finite()
                || !frame[0].is_finite()
                || !frame[1].is_finite()
                || frame[1] <= 0.0
            {
                return Err(VoiceError::new(
                    "Vocoder LF source inputs are not finite or F0 is nonpositive",
                ));
            }
            let mut shape = (0.1 + 2.9 / (1.0 + (-logit).exp())).clamp(0.1, 3.0);
            shape += control
                * if control > 0.0 {
                    3.0 - shape
                } else {
                    shape - 0.1
                };
            Ok(ExcitationFrame {
                f0_hz: frame[1],
                shape: shape.clamp(0.1, 3.0),
                voiced: frame[0] >= 0.5,
            })
        })
        .collect()
}
