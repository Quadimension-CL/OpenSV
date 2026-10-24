use std::fmt;

pub const POOL_TYPE_ID: [u8; 16] = [
    0xc5, 0x6d, 0x41, 0x22, 0x35, 0x84, 0xca, 0xc3, 0x32, 0xe0, 0x84, 0x06, 0xab, 0xcf, 0x1d, 0xda,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolConfig {
    pub kernel: usize,
    pub stride: usize,
    pub dilation: usize,
    pub left_padding: usize,
    pub right_padding: usize,
}

impl PoolConfig {
    pub fn parse(payload: &[u8]) -> Result<Self, PoolError> {
        if payload.len() != 22 {
            return Err(PoolError("configuration must have 22 bytes"));
        }
        if payload[20] != 0 || payload[21] == 0 {
            return Err(PoolError("unsupported pooling flags"));
        }
        let words = payload[..20].as_chunks::<4>().0;
        let config = Self {
            kernel: u32::from_le_bytes(words[0]) as usize,
            stride: u32::from_le_bytes(words[1]) as usize,
            dilation: u32::from_le_bytes(words[2]) as usize,
            left_padding: u32::from_le_bytes(words[3]) as usize,
            right_padding: u32::from_le_bytes(words[4]) as usize,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(self) -> Result<(), PoolError> {
        if self.kernel == 0 || self.stride == 0 || self.dilation == 0 {
            return Err(PoolError("kernel, stride and dilation must be positive"));
        }
        if self.stride > self.kernel {
            return Err(PoolError(
                "stride exceeds kernel (original constructor restriction)",
            ));
        }
        self.receptive_frames()?;
        Ok(())
    }

    fn receptive_frames(self) -> Result<usize, PoolError> {
        self.kernel
            .checked_sub(1)
            .and_then(|n| n.checked_mul(self.dilation))
            .and_then(|n| n.checked_add(1))
            .ok_or(PoolError("receptive window overflows"))
    }

    fn available_outputs(self, frames: usize) -> Result<usize, PoolError> {
        Ok(frames
            .checked_sub(self.receptive_frames()?)
            .map_or(0, |n| n / self.stride + 1))
    }

    pub fn output_frames(self, frames: usize) -> Result<usize, PoolError> {
        self.validate()?;
        let padded = frames
            .checked_add(self.left_padding)
            .and_then(|n| n.checked_add(self.right_padding))
            .ok_or(PoolError("padded input length overflows"))?;
        self.available_outputs(padded)
    }

    pub fn stream(self, channels: usize) -> Result<PoolStream, PoolError> {
        self.validate()?;
        if channels == 0 {
            return Err(PoolError("input channels must be positive"));
        }
        let length = self
            .left_padding
            .checked_mul(channels)
            .ok_or(PoolError("left padding size overflows"))?;
        Ok(PoolStream {
            config: self,
            channels,
            pending: vec![0.0; length],
            finished: false,
        })
    }

    pub fn execute(self, input: &[f32], channels: usize) -> Result<Vec<f32>, PoolError> {
        self.stream(channels)?.process(input, true)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolError(&'static str);

impl fmt::Display for PoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid temporal pooling: {}", self.0)
    }
}

impl std::error::Error for PoolError {}

#[derive(Clone)]
pub struct PoolStream {
    config: PoolConfig,
    channels: usize,
    pending: Vec<f32>,
    finished: bool,
}

impl PoolStream {
    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn reset(&mut self) {
        self.pending.clear();
        self.pending
            .resize(self.config.left_padding * self.channels, 0.0);
        self.finished = false;
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn pending_frames(&self) -> usize {
        self.pending.len() / self.channels
    }

    pub fn process(&mut self, input: &[f32], final_chunk: bool) -> Result<Vec<f32>, PoolError> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("op.pooling");
        if self.finished {
            return Err(PoolError("stream has already been finalized"));
        }
        if !input.len().is_multiple_of(self.channels) {
            return Err(PoolError("partial input frame"));
        }
        let right = if final_chunk {
            self.config.right_padding
        } else {
            0
        };
        let length = right
            .checked_mul(self.channels)
            .and_then(|n| n.checked_add(input.len()))
            .and_then(|n| n.checked_add(self.pending.len()))
            .ok_or(PoolError("input size overflows"))?;
        let frames = self.config.available_outputs(length / self.channels)?;
        let out_length = frames
            .checked_mul(self.channels)
            .ok_or(PoolError("output size overflows"))?;
        self.pending.extend_from_slice(input);
        self.pending.resize(length, 0.0);
        let mut output = vec![0.0; out_length];
        for (frame, row) in output.chunks_exact_mut(self.channels).enumerate() {
            for tap in 0..self.config.kernel {
                let offset =
                    (frame * self.config.stride + tap * self.config.dilation) * self.channels;
                for (value, &sample) in row
                    .iter_mut()
                    .zip(&self.pending[offset..offset + self.channels])
                {
                    *value += sample;
                }
            }
            let scale = 1.0 / self.config.kernel as f32;
            for value in row {
                *value *= scale;
            }
        }
        let consumed = frames * self.config.stride * self.channels;
        self.pending.drain(..consumed);

        self.finished = final_chunk;
        Ok(output)
    }
}
