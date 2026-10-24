use std::{fmt, sync::Arc};

pub const STANDARD_NORMALIZER_TYPE_ID: [u8; 16] = [
    0xb1, 0xa2, 0xa3, 0x42, 0x85, 0x77, 0xa9, 0xea, 0x68, 0x6d, 0x37, 0xfe, 0x96, 0x54, 0x9f, 0x51,
];

#[derive(Clone)]
pub struct StandardNormalizer {
    mean: Arc<[f32]>,
    standard_deviation: Arc<[f32]>,
}

#[derive(Clone, Debug)]
pub enum StandardError {
    InvalidStatistics,
    InvalidChannel(usize),
    InvalidLength,
}

impl fmt::Display for StandardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStatistics => f.write_str("Invalid standard normalizer statistics"),
            Self::InvalidChannel(channel) => {
                write!(f, "Standard normalizer channel {channel} is absent")
            }
            Self::InvalidLength => f.write_str("Standard normalizer input/output shape mismatch"),
        }
    }
}

impl std::error::Error for StandardError {}

impl StandardNormalizer {
    pub fn new(mean: Arc<[f32]>, standard_deviation: Arc<[f32]>) -> Result<Self, StandardError> {
        if mean.is_empty()
            || mean.len() != standard_deviation.len()
            || mean
                .iter()
                .chain(standard_deviation.iter())
                .any(|value| !value.is_finite())
        {
            return Err(StandardError::InvalidStatistics);
        }
        Ok(Self {
            mean,
            standard_deviation,
        })
    }

    pub fn channels(&self) -> usize {
        self.mean.len()
    }

    pub fn normalize_scalar(&self, channel: usize, value: f32) -> Result<f32, StandardError> {
        self.check_channel(channel)?;
        Ok(self.normalize_value(channel, value))
    }

    pub fn denormalize_scalar(&self, channel: usize, value: f32) -> Result<f32, StandardError> {
        self.check_channel(channel)?;
        Ok(self.denormalize_value(channel, value))
    }

    pub fn normalize(&self, input: &[f32], output: &mut [f32]) -> Result<(), StandardError> {
        self.check_buffers(input, output)?;
        for (index, (&value, target)) in input.iter().zip(output.iter_mut()).enumerate() {
            *target = self.normalize_value(index % self.channels(), value);
        }
        Ok(())
    }

    pub fn denormalize(&self, input: &[f32], output: &mut [f32]) -> Result<(), StandardError> {
        self.check_buffers(input, output)?;
        for (index, (&value, target)) in input.iter().zip(output.iter_mut()).enumerate() {
            *target = self.denormalize_value(index % self.channels(), value);
        }
        Ok(())
    }

    fn normalize_value(&self, channel: usize, value: f32) -> f32 {
        (value - self.mean[channel]) / self.standard_deviation[channel].max(1.0e-5)
    }

    fn denormalize_value(&self, channel: usize, value: f32) -> f32 {
        self.mean[channel] + self.standard_deviation[channel] * value
    }

    fn check_channel(&self, channel: usize) -> Result<(), StandardError> {
        if channel >= self.channels() {
            Err(StandardError::InvalidChannel(channel))
        } else {
            Ok(())
        }
    }

    fn check_buffers(&self, input: &[f32], output: &[f32]) -> Result<(), StandardError> {
        if input.len() != output.len() || !input.len().is_multiple_of(self.channels()) {
            Err(StandardError::InvalidLength)
        } else {
            Ok(())
        }
    }
}
