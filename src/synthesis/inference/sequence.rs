//! Checked frame-major buffers passed between model operators.
use crate::synthesis::voice::{Result, VoiceError};
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct Sequence {
    pub frames: usize,
    pub channels: usize,
    pub values: Vec<f32>,
}

impl Sequence {
    pub fn new(frames: usize, channels: usize, values: Vec<f32>) -> Result<Self> {
        if channels == 0 || frames.checked_mul(channels) != Some(values.len()) {
            return Err(VoiceError::new("Invalid frame-major sequence dimensions"));
        }
        Ok(Self {
            frames,
            channels,
            values,
        })
    }

    pub fn from_values(channels: usize, values: Vec<f32>) -> Result<Self> {
        if channels == 0 || !values.len().is_multiple_of(channels) {
            return Err(VoiceError::new("Sequence contains an incomplete frame"));
        }
        Self::new(values.len() / channels, channels, values)
    }

    pub fn zeros(frames: usize, channels: usize) -> Result<Self> {
        let len = frames
            .checked_mul(channels)
            .ok_or_else(|| VoiceError::new("Sequence size overflows"))?;
        Self::new(frames, channels, vec![0.0; len])
    }

    pub fn frame(&self, index: usize) -> Result<&[f32]> {
        self.validate()?;
        if index >= self.frames {
            return Err(VoiceError::new("Sequence frame is out of range"));
        }
        Ok(&self.values[index * self.channels..(index + 1) * self.channels])
    }

    pub fn slice_frames(&self, range: Range<usize>) -> Result<Self> {
        self.validate()?;
        if range.start > range.end || range.end > self.frames {
            return Err(VoiceError::new("Sequence slice is out of range"));
        }
        Self::new(
            range.len(),
            self.channels,
            self.values[range.start * self.channels..range.end * self.channels].to_vec(),
        )
    }

    pub fn validate(&self) -> Result<()> {
        if self.channels == 0 || self.frames.checked_mul(self.channels) != Some(self.values.len()) {
            return Err(VoiceError::new("Sequence shape does not match its data"));
        }
        Ok(())
    }

    pub fn concat_channels(inputs: &[&Self]) -> Result<Self> {
        let frames = inputs
            .first()
            .ok_or_else(|| VoiceError::new("Concatenation needs an input"))?
            .frames;
        let mut channels = 0_usize;
        for input in inputs {
            input.validate()?;
            if input.frames != frames {
                return Err(VoiceError::new(
                    "Concatenated sequences have different frame counts",
                ));
            }
            channels = channels
                .checked_add(input.channels)
                .ok_or_else(|| VoiceError::new("Concatenated channels overflow"))?;
        }
        let mut output = Self::zeros(frames, channels)?;
        for frame in 0..frames {
            let mut offset = frame * channels;
            for input in inputs {
                output.values[offset..offset + input.channels].copy_from_slice(input.frame(frame)?);
                offset += input.channels;
            }
        }
        Ok(output)
    }
}
