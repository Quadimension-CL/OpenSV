use super::output::FrameMetadata;
use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

pub fn from_timbre(
    controls: Option<&Sequence>,
    physical_frames: usize,
) -> Result<Vec<FrameMetadata>> {

    let Some(controls) = controls else {
        let mut metadata = Vec::new();
        metadata
            .try_reserve_exact(physical_frames)
            .map_err(|_| VoiceError::new("Could not allocate vocoder frame metadata"))?;
        metadata.resize(physical_frames, FrameMetadata::default());
        return Ok(metadata);
    };
    controls.validate()?;
    if controls.channels != 2 || controls.frames != physical_frames {
        return Err(VoiceError::new(
            "Timbre metadata must contain two values for each physical vocoder input frame",
        ));
    }
    let mut metadata = Vec::new();
    metadata
        .try_reserve_exact(physical_frames)
        .map_err(|_| VoiceError::new("Could not allocate vocoder frame metadata"))?;
    metadata.extend(
        controls
            .values
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&values| FrameMetadata { values }),
    );
    Ok(metadata)
}

pub fn source_storage_value(metadata: FrameMetadata) -> f32 {
    let first = metadata.values[0];
    if first.is_nan() { 0.0 } else { first }
}
