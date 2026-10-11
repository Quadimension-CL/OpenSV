//! Per-channel min/max transforms used at acoustic stage boundaries.
//!
//! The inverse deliberately excludes the forward denominator epsilon.

use std::{fmt, sync::Arc};

pub const RANGE_NORMALIZER_TYPE_ID: [u8; 16] = [
    0xa7, 0x72, 0x9f, 0x8d, 0xad, 0x09, 0x0d, 0xd6, 0xbe, 0x21, 0xd1, 0x71, 0x8e, 0xe3, 0x4f, 0x97,
];

#[derive(Clone)]
pub struct RangeNormalizer {
    minimum: Arc<[f32]>,
    maximum: Arc<[f32]>,
    output_minimum: f32,
    output_maximum: f32,
}

#[derive(Clone, Debug)]
pub enum RangeError {
    InvalidBounds,
    InvalidChannel(usize),
    InvalidLength,
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBounds => f.write_str("Invalid range normalizer bounds"),
            Self::InvalidChannel(channel) => {
                write!(f, "Range normalizer channel {channel} is absent")
            }
            Self::InvalidLength => f.write_str("Range normalizer input/output shape mismatch"),
        }
    }
}

impl std::error::Error for RangeError {}

impl RangeNormalizer {
    /// Prepare three source vector children: minimum, maximum and two output bounds.
    pub fn new(
        minimum: Arc<[f32]>,
        maximum: Arc<[f32]>,
        output_bounds: &[f32],
    ) -> Result<Self, RangeError> {
        if minimum.is_empty()
            || minimum.len() != maximum.len()
            || output_bounds.len() != 2
            || minimum
                .iter()
                .chain(maximum.iter())
                .chain(output_bounds.iter())
                .any(|v| !v.is_finite())
            || output_bounds[0] == output_bounds[1]
        {
            return Err(RangeError::InvalidBounds);
        }
        Ok(Self {
            minimum,
            maximum,
            output_minimum: output_bounds[0],
            output_maximum: output_bounds[1],
        })
    }

    pub fn channels(&self) -> usize {
        self.minimum.len()
    }

    /// `clip` clamps the normalized ratio to [-1, 1], before the output mapping.
    pub fn normalize_scalar(
        &self,
        channel: usize,
        value: f32,
        clip: bool,
    ) -> Result<f32, RangeError> {
        self.check_channel(channel)?;
        Ok(self.normalize_value(channel, value, clip))
    }

    pub fn denormalize_scalar(&self, channel: usize, value: f32) -> Result<f32, RangeError> {
        self.check_channel(channel)?;
        Ok(self.denormalize_value(channel, value))
    }

    /// Process contiguous complete frames; use scalar methods for an individual
    /// channel slice such as the duration frontend's log-duration feature.
    pub fn normalize(
        &self,
        input: &[f32],
        output: &mut [f32],
        clip: bool,
    ) -> Result<(), RangeError> {
        self.check_buffers(input, output)?;
        for (position, (&value, target)) in input.iter().zip(output.iter_mut()).enumerate() {
            *target = self.normalize_value(position % self.channels(), value, clip);
        }
        Ok(())
    }

    pub fn denormalize(&self, input: &[f32], output: &mut [f32]) -> Result<(), RangeError> {
        self.check_buffers(input, output)?;
        for (position, (&value, target)) in input.iter().zip(output.iter_mut()).enumerate() {
            *target = self.denormalize_value(position % self.channels(), value);
        }
        Ok(())
    }

    fn normalize_value(&self, channel: usize, value: f32, clip: bool) -> f32 {
        let minimum = self.minimum[channel];
        let mut ratio = (value - minimum) / ((self.maximum[channel] - minimum) + 1.0e-5);
        if clip {
            // Preserve comparison order so NaN maps to the upper clamp bound.
            ratio = if ratio < -1.0 {
                -1.0
            } else if ratio <= 1.0 {
                ratio
            } else {
                1.0
            };
        }
        (self.output_maximum - self.output_minimum) * ratio + self.output_minimum
    }

    fn denormalize_value(&self, channel: usize, value: f32) -> f32 {
        let minimum = self.minimum[channel];
        ((self.maximum[channel] - minimum) * (value - self.output_minimum))
            / (self.output_maximum - self.output_minimum)
            + minimum
    }

    fn check_channel(&self, channel: usize) -> Result<(), RangeError> {
        if channel >= self.channels() {
            Err(RangeError::InvalidChannel(channel))
        } else {
            Ok(())
        }
    }

    fn check_buffers(&self, input: &[f32], output: &[f32]) -> Result<(), RangeError> {
        if input.len() != output.len() || !input.len().is_multiple_of(self.channels()) {
            Err(RangeError::InvalidLength)
        } else {
            Ok(())
        }
    }
}
