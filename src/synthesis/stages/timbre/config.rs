use crate::synthesis::voice::{PreparedTree, Result, VoiceError};

pub const TIMBRE_ROOT_TYPE_ID: [u8; 16] = [
    0x60, 0x26, 0xa2, 0x7c, 0xf0, 0x7d, 0xaf, 0xef, 0x12, 0x96, 0xe6, 0xa2, 0x45, 0x6d, 0x84, 0x0c,
];

/// Four u16 dimensions and the 19 child bindings of the acoustic model.
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
    pub encoder: usize,
    pub encoded_affine: usize,
    pub decoder: usize,
    pub decoded_affine: usize,
    pub refinement: usize,
    pub spectral_network: usize,
    pub spectral_head: usize,
    pub auxiliary_network: usize,
    pub auxiliary_head: usize,
    pub sampler: usize,
}

impl TimbreLayout {
    pub fn prepare(tree: &PreparedTree) -> Result<Self> {
        let root = tree.node(tree.root_index())?;
        if root.type_id != TIMBRE_ROOT_TYPE_ID || root.children.len() != 19 {
            return Err(VoiceError::new(
                "Timbre root requires the recovered 19-child model",
            ));
        }
        let payload = tree.payload(tree.root_index())?;
        if payload.len() != 8 {
            return Err(VoiceError::new(
                "Timbre root configuration must contain four u16 values",
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
            if tree.tensor(index)?.vector().is_none() {
                return Err(VoiceError::new(
                    "Timbre default style must be a float vector",
                ));
            }
        }
        Ok(Self {
            ppg_noise_channels: dimensions[0],
            timbre_noise_channels: dimensions[1],
            pitch_embedding_channels: dimensions[2],
            stride: dimensions[3],
            frontend: c[0],
            styles: c[1],
            log_pitch_normalizer: c[2],
            acoustic_normalizer: c[3],
            default_style: c[4],
            default_secondary_style: c[5],
            feature_network: c[6],
            pitch_projection: c[7],
            pulse_projection: c[8],
            encoder: c[9],
            encoded_affine: c[10],
            decoder: c[11],
            decoded_affine: c[12],
            refinement: c[13],
            spectral_network: c[14],
            spectral_head: c[15],
            auxiliary_network: c[16],
            auxiliary_head: c[17],
            sampler: c[18],
        })
    }
}
