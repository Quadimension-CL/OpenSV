use std::collections::VecDeque;

use crate::synthesis::{
    execution::checkpoint,
    inference::{
        ModelRuntime, Sequence,
        ops::{MODULE_LIST_TYPE_ID, SEQUENTIAL_TYPE_ID},
    },
    voice::{PreparedTree, Result, VoiceError},
};

pub const SPECTRAL_HEADS_TYPE_ID: [u8; 16] = [
    0x2d, 0xae, 0xaa, 0xac, 0x38, 0xce, 0x69, 0x8d, 0xc6, 0x0c, 0xa1, 0xde, 0x32, 0x53, 0xcb, 0xd5,
];

#[derive(Debug)]
struct Band {
    node: usize,
    start: usize,
    end: usize,
    weights: Vec<f32>,
    pending: VecDeque<f32>,
}

#[derive(Debug)]
pub struct SpectralHeads {
    head_count: usize,
    channels: usize,
    bands: Vec<Band>,
}

impl SpectralHeads {
    pub fn prepare(tree: &PreparedTree, index: usize) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.spectral_heads.prepare");
        let node = tree.node(index)?;
        let [branches] = node.children.as_slice() else {
            return Err(VoiceError::new(
                "Vocoder spectral heads require one module list",
            ));
        };
        if node.type_id != SPECTRAL_HEADS_TYPE_ID {
            return Err(VoiceError::new("Invalid vocoder spectral-head type"));
        }
        let payload = tree.payload(index)?;
        if payload.len() < 8 || !payload.len().is_multiple_of(4) {
            return Err(VoiceError::new("Invalid spectral-head configuration size"));
        }
        let integers = payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|value| i32::from_le_bytes([value[0], value[1], value[2], value[3]]))
            .collect::<Vec<_>>();
        let head_count = integers[0];
        let band_count = integers[1];
        if !(1..=4).contains(&head_count)
            || band_count < 1
            || usize::try_from(band_count)
                .ok()
                .and_then(|n| n.checked_mul(4))
                .and_then(|n| n.checked_add(2))
                != Some(integers.len())
        {
            return Err(VoiceError::new("Invalid spectral-head or band count"));
        }
        let branches = tree.node(*branches)?;
        if branches.type_id != MODULE_LIST_TYPE_ID || branches.children.len() != band_count as usize
        {
            return Err(VoiceError::new(
                "Spectral-head branches do not match configuration",
            ));
        }
        let mut channels = 0;
        let mut bands = Vec::with_capacity(band_count as usize);
        for (&node, config) in branches
            .children
            .iter()
            .zip(integers[2..].as_chunks::<4>().0)
        {
            let [start, end, fade_in, fade_out] = [config[0], config[1], config[2], config[3]];
            if start < 0
                || end <= start
                || fade_in < 0
                || fade_out < 0
                || fade_in == 1
                || fade_out == 1
                || i64::from(fade_in) + i64::from(fade_out) > i64::from(end - start)
                || tree.node(node)?.type_id != SEQUENTIAL_TYPE_ID
            {
                return Err(VoiceError::new(
                    "Invalid spectral frequency band or branch type",
                ));
            }
            let start = start as usize;
            let end = end as usize;
            let width = end - start;
            let fade_in = fade_in as usize;
            let fade_out = fade_out as usize;
            let weights = (0..width)
                .map(|bin| {
                    if bin < fade_in {
                        bin as f32 / (fade_in - 1) as f32
                    } else if bin >= width - fade_out {
                        (width - 1 - bin) as f32 / (fade_out - 1) as f32
                    } else {
                        1.0
                    }
                })
                .collect();
            channels = channels.max(end);
            bands.push(Band {
                node,
                start,
                end,
                weights,
                pending: VecDeque::new(),
            });
        }
        Ok(Self {
            head_count: head_count as usize,
            channels,
            bands,
        })
    }

    pub fn head_count(&self) -> usize {
        self.head_count
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.pending.clear();
        }
    }

    pub fn execute(
        &mut self,
        runtime: &mut ModelRuntime<'_>,
        input: &Sequence,
        flush: bool,
    ) -> Result<Vec<Sequence>> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.spectral_heads.execute");
        input.validate()?;
        let mut frames = usize::MAX;
        for band in &mut self.bands {
            let result = runtime.execute(band.node, input, flush)?;
            result.validate()?;
            let expected = (band.end - band.start)
                .checked_mul(self.head_count)
                .ok_or_else(|| VoiceError::new("Spectral band channels overflow"))?;
            if result.channels != expected {
                return Err(VoiceError::new(
                    "Spectral branch output has the wrong channel count",
                ));
            }
            if result.values.iter().any(|value| !value.is_finite()) {
                return Err(VoiceError::new(
                    "Spectral branch produced non-finite values",
                ));
            }
            band.pending.extend(result.values);
            frames = frames.min(band.pending.len() / expected);
        }
        let mut output = (0..self.head_count)
            .map(|_| Sequence::zeros(frames, self.channels))
            .collect::<Result<Vec<_>>>()?;
        for frame in 0..frames {
            checkpoint()?;
            for band in &mut self.bands {
                for head in &mut output {
                    let offset = frame * self.channels + band.start;
                    for (bin, &weight) in band.weights.iter().enumerate() {
                        let value = band.pending.pop_front().ok_or_else(|| {
                            VoiceError::new("Spectral branch queue is incomplete")
                        })?;
                        head.values[offset + bin] += value * weight;
                    }
                }
            }
        }
        if flush && self.bands.iter().any(|band| !band.pending.is_empty()) {
            return Err(VoiceError::new(
                "Spectral branches ended with unaligned frame counts",
            ));
        }
        Ok(output)
    }
}
