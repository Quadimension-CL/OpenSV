use crate::synthesis::voice::{PreparedTree, Result, VoiceError};

pub const TIMBRE_ROOT_TYPE_ID: [u8; 16] = [
    0x60, 0x26, 0xa2, 0x7c, 0xf0, 0x7d, 0xaf, 0xef, 0x12, 0x96, 0xe6, 0xa2, 0x45, 0x6d, 0x84, 0x0c,
];
pub const TIMBRE16_ROOT_TYPE_ID: [u8; 16] = [
    0x3d, 0xe8, 0x38, 0x06, 0x8c, 0x36, 0xd8, 0xa9, 0x93, 0xdc, 0x8e, 0x8e, 0xd6, 0x67, 0xe9, 0xbe,
];

#[derive(Clone, Copy, Debug)]
pub enum TimbreArchitecture {
    Sixteen,
    Nineteen {
        encoder: usize,
        encoded_affine: usize,
        decoder: usize,
        decoded_affine: usize,
    },
}

#[derive(Clone, Debug)]
pub struct TimbreLayout {
    pub ppg_noise_channels: usize,
    pub timbre_noise_channels: usize,
    pub pitch_embedding_channels: usize,
    pub stride: usize,
    pub frontend: usize,
    pub styles: usize,
    pub log_pitch_normalizer: usize,
    pub acoustic_normalizer: usize,
    pub default_style: usize,
    pub default_secondary_style: usize,
    pub feature_network: usize,
    pub pitch_projection: usize,
    pub pulse_projection: usize,
    pub architecture: TimbreArchitecture,
    pub refinement: usize,
    pub spectral_network: usize,
    pub spectral_head: usize,
    pub auxiliary_network: usize,
    pub auxiliary_head: usize,
    pub sampler: usize,
}

impl TimbreLayout {
    pub fn prepare(tree: &PreparedTree, speaker: Option<usize>) -> Result<Self> {
        let root = tree.node(tree.root_index())?;
        let sixteen = match (root.type_id, root.children.len()) {
            (TIMBRE16_ROOT_TYPE_ID, 16) => true,
            (TIMBRE_ROOT_TYPE_ID, 19) => false,
            _ => {
                return Err(VoiceError::new(format!(
                    "Unsupported SV2 timbre root type or child count ({})",
                    root.children.len()
                )));
            }
        };
        let payload = tree.payload(tree.root_index())?;
        if payload.len() != if sixteen { 6 } else { 8 } {
            return Err(VoiceError::new(
                "Timbre root dimensions do not match its architecture",
            ));
        }
        let dimensions: Vec<_> = payload
            .as_chunks::<2>()
            .0
            .iter()
            .map(|bytes| usize::from(u16::from_le_bytes([bytes[0], bytes[1]])))
            .collect();
        if dimensions.contains(&0) {
            return Err(VoiceError::new(
                "Timbre dimensions and stride must be nonzero",
            ));
        }
        let c = &root.children;
        for &index in &[c[4], c[5]] {
            crate::synthesis::voice::speakers::vector(tree, index, speaker)?;
        }
        let architecture = if sixteen {
            let reserved = tree.node(c[6])?;
            if reserved.type_id
                != [
                    0xd8, 0x9e, 0x1b, 0x8a, 0x6c, 0x01, 0xd9, 0x3d, 0x18, 0xa8, 0xd1, 0xa7, 0x40,
                    0x1e, 0x1c, 0x0e,
                ]
                || reserved.payload_bytes != 0
                || !reserved.children.is_empty()
            {
                return Err(VoiceError::new(
                    "Unsupported 16-child acoustic auxiliary module",
                ));
            }
            TimbreArchitecture::Sixteen
        } else {
            TimbreArchitecture::Nineteen {
                encoder: c[9],
                encoded_affine: c[10],
                decoder: c[11],
                decoded_affine: c[12],
            }
        };
        let (feature, pitch, pulse, refinement) = if sixteen {
            (7, 8, 9, 10)
        } else {
            (6, 7, 8, 13)
        };
        Ok(Self {
            ppg_noise_channels: dimensions[0],
            timbre_noise_channels: dimensions[1],
            pitch_embedding_channels: dimensions[2],
            stride: if sixteen { 1 } else { dimensions[3] },
            frontend: c[0],
            styles: c[1],
            log_pitch_normalizer: c[2],
            acoustic_normalizer: c[3],
            default_style: c[4],
            default_secondary_style: c[5],
            feature_network: c[feature],
            pitch_projection: c[pitch],
            pulse_projection: c[pulse],
            architecture,
            refinement: c[refinement],
            spectral_network: c[refinement + 1],
            spectral_head: c[refinement + 2],
            auxiliary_network: c[refinement + 3],
            auxiliary_head: c[refinement + 4],
            sampler: c[refinement + 5],
        })
    }
}
