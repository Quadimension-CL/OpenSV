use crate::synthesis::voice::{PreparedModule, PreparedTree, Result, VoiceError};

const TYPE_ID: [u8; 16] = [
    0xd8, 0x4b, 0xf4, 0x4a, 0xd0, 0x47, 0xa3, 0xb2, 0x81, 0x0a, 0x61, 0xcc, 0x45, 0x1e, 0x08, 0x9b,
];
const SIXTEEN_TYPE_ID: [u8; 16] = [
    0x93, 0x6a, 0x61, 0x42, 0x05, 0x6f, 0x2d, 0x72, 0x0b, 0xd3, 0x18, 0xe8, 0x65, 0x67, 0x8c, 0xa9,
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
    pub(super) children: SamplerChildren,
    pub(super) ppg_mean: Option<usize>,
}

pub(super) enum SamplerChildren {
    Sixteen([usize; 7]),
    Nineteen([usize; 6]),
}

impl AcousticSampler {
    pub fn prepare(tree: &PreparedTree, node: usize) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("timbre.sampler.prepare");
        let source = tree.node(node)?;
        let children = match source.type_id {
            TYPE_ID => {
                SamplerChildren::Nineteen(source.children.as_slice().try_into().map_err(|_| {
                    VoiceError::new("19-child acoustic sampler requires six children")
                })?)
            }
            SIXTEEN_TYPE_ID => {
                SamplerChildren::Sixteen(source.children.as_slice().try_into().map_err(|_| {
                    VoiceError::new("16-child acoustic sampler requires seven children")
                })?)
            }
            _ => {
                return Err(VoiceError::new(
                    "Unsupported two-head acoustic sampler type",
                ));
            }
        };
        let config = SamplerConfig::parse(&tree.payload(node)?)?;
        let ppg_mean = match &children {
            SamplerChildren::Sixteen(children) => {
                let mean = tree.node(children[0])?;
                if mean.type_id == crate::synthesis::stages::f0::features::EMPTY_SLOT_TYPE_ID
                    && mean.children.is_empty()
                    && mean.payload_bytes == 0
                {
                    None
                } else if matches!(mean.module, PreparedModule::Affine(_)) {
                    Some(children[0])
                } else {
                    return Err(VoiceError::new("Unsupported 16-child PPG mean projection"));
                }
            }
            SamplerChildren::Nineteen(_) => None,
        };
        Ok(Self {
            config,
            children,
            ppg_mean,
        })
    }
}
