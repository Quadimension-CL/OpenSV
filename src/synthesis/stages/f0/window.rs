use crate::synthesis::voice::{Result, VoiceError};
use std::ops::Range;

pub(super) fn matrix<'a>(
    values: &'a [f32],
    frames: usize,
    range: &Range<usize>,
) -> Result<&'a [f32]> {
    if frames == 0
        || !values.len().is_multiple_of(frames)
        || range.start > range.end
        || range.end > frames
    {
        return Err(VoiceError::new("Invalid F0 work-region feature matrix"));
    }
    let channels = values.len() / frames;
    Ok(&values[range.start * channels..range.end * channels])
}
