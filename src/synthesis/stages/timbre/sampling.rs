//! Two noise-conditioned acoustic heads in the `d84bf44a` module.
//!
//! Evaluate each head once. Gaussian samples and the two distribution controls
//! are explicit inputs; this program has no diffusion-step loop.

use crate::synthesis::voice::{PreparedTree, Result, VoiceError};

const TYPE_ID: [u8; 16] = [
    0xd8, 0x4b, 0xf4, 0x4a, 0xd0, 0x47, 0xa3, 0xb2, 0x81, 0x0a, 0x61, 0xcc, 0x45, 0x1e, 0x08, 0x9b,
];

#[derive(Clone, Copy, Debug)]
pub struct SamplerConfig {
    pub ppg_noise: usize,
    pub timbre_noise: usize,
    pub ppg_prior: usize,
    pub timbre_prior: usize,
    pub controls: usize,
    pub embedding: usize,
    pub embedding_scale: f32,
    pub ppg_hidden: usize,
    pub timbre_hidden: usize,
}

impl SamplerConfig {
    /// Payload only, excluding the 16-byte module type prefix.
    pub fn parse(payload: &[u8]) -> Result<Self> {
        if payload.len() != 20 {
            return Err(VoiceError::new(
                "Acoustic sampler configuration must have 20 bytes",
            ));
        }
        let word = |offset| usize::from(u16::from_le_bytes([payload[offset], payload[offset + 1]]));
        let config = Self {
            ppg_noise: word(0),
            timbre_noise: word(2),
            ppg_prior: word(4),
            timbre_prior: word(6),
            controls: word(8),
            embedding: word(10),
            embedding_scale: f32::from_le_bytes(payload[12..16].try_into().unwrap()),
            ppg_hidden: word(16),
            timbre_hidden: word(18),
        };
        if [
            config.ppg_noise,
            config.timbre_noise,
            config.ppg_prior,
            config.timbre_prior,
            config.controls,
            config.embedding,
            config.ppg_hidden,
            config.timbre_hidden,
        ]
        .contains(&0)
            || !config.embedding_scale.is_finite()
            || config.controls != 2
        {
            return Err(VoiceError::new(
                "Invalid acoustic sampler dimensions or embedding scale",
            ));
        }
        Ok(config)
    }

    pub(super) fn embedding(&self, value: f32, target: &mut [f32]) {
        let half = self.embedding / 2;
        let value = value * self.embedding_scale;
        for index in 0..half {
            let divisor = 10000.0_f32.powf((index * 2) as f32 / self.embedding as f32);
            let (sin, cos) = (value / divisor).sin_cos();
            target[index] = sin;
            target[index + half] = cos;
        }
    }
}

pub(super) struct AcousticSampler {
    pub config: SamplerConfig,
    pub(super) children: [usize; 6],
}

impl AcousticSampler {
    pub fn prepare(tree: &PreparedTree, node: usize) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("timbre.sampler.prepare");
        let source = tree.node(node)?;
        if source.type_id != TYPE_ID {
            return Err(VoiceError::new(
                "The supplied node is not the two-head acoustic sampler",
            ));
        }
        let children = source
            .children
            .as_slice()
            .try_into()
            .map_err(|_| VoiceError::new("Acoustic sampler requires six model children"))?;
        let config = SamplerConfig::parse(&tree.payload(node)?)?;
        Ok(Self { config, children })
    }
}
