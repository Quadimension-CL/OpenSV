//! Root configuration and child layouts of the SV1 and SV2 spectral vocoders.
//!
//! Both configurations occupy 18 bytes. SV1 has no positional sampler;
//! SV2 may bind one.

use crate::synthesis::voice::{PreparedTree, Result, VoiceError, embedded::types::VOCODER_TYPE_ID};

pub const ROOT_TYPE: [u8; 16] = [
    0xc6, 0xbc, 0xba, 0x5d, 0x8c, 0xa1, 0xf6, 0xce, 0xd6, 0x49, 0x89, 0x7b, 0x4b, 0xfc, 0x23, 0x81,
];

#[derive(Clone, Copy, Debug)]
pub struct Configuration {
    pub bands: usize,
    pub excitation_log_gain: f32,
    pub noise_log_gain: f32,
    pub use_input_projection: bool,
    pub gate_postfilter_noise: bool,
    /// Copied without conversion, independently of the exponentiated source gains.
    pub trailing_scalar: f32,
}

impl Configuration {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        if payload.len() != 18 {
            return Err(VoiceError::new(
                "Vocoder root requires an 18-byte configuration",
            ));
        }
        let word = |at: usize| {
            u32::from_le_bytes(
                payload[at..at + 4]
                    .try_into()
                    .expect("checked root payload"),
            )
        };
        let config = Self {
            bands: word(0) as usize,
            excitation_log_gain: f32::from_bits(word(4)),
            noise_log_gain: f32::from_bits(word(8)),
            use_input_projection: payload[12] & 1 != 0,
            gate_postfilter_noise: payload[13] & 1 != 0,
            trailing_scalar: f32::from_bits(word(14)),
        };
        if config.bands == 0
            || !config.excitation_log_gain.is_finite()
            || !config.noise_log_gain.is_finite()
            || !config.trailing_scalar.is_finite()
        {
            return Err(VoiceError::new(
                "Vocoder configuration contains invalid dimensions or gains",
            ));
        }
        Ok(config)
    }
}

/// Ownership edges explicitly bound by the root's prepare method.
/// Their order is not a sequential neural network execution order.
#[derive(Clone, Debug)]
pub struct Modules {
    pub conditions: usize,
    pub input_projection: Option<usize>,
    pub conditioned_network: usize,
    pub spectral_heads: usize,
    pub coefficients: usize,
    pub gain_network: usize,
    pub harmonic_filter: usize,
    pub noise_filter: usize,
    pub third_filter: usize,
    pub analysis_filterbank: usize,
    pub synthesis_filterbank: usize,
    pub postfilter_network: usize,
    pub postfilter_projection: usize,
    pub recurrent_sampler: Option<usize>,
}

impl Modules {
    pub fn prepare(tree: &PreparedTree, root: usize, config: Configuration) -> Result<Self> {
        let node = tree.node(root)?;
        let positional_allowed = match node.type_id {
            VOCODER_TYPE_ID => false,
            ROOT_TYPE => true,
            _ => {
                return Err(VoiceError::new(
                    "Selected tree is not a recovered vocoder root",
                ));
            }
        };
        let base = 1 + usize::from(config.use_input_projection);
        // This root always enables the third-filter branch.
        let required = base + 11;
        let maximum = required + usize::from(positional_allowed);
        if !(required..=maximum).contains(&node.children.len()) {
            return Err(VoiceError::new(
                "Vocoder root has an unexpected child layout",
            ));
        }
        let at = |i| node.children[i];
        Ok(Self {
            conditions: at(0),
            input_projection: config.use_input_projection.then(|| at(1)),
            conditioned_network: at(base),
            spectral_heads: at(base + 1),
            coefficients: at(base + 2),
            gain_network: at(base + 3),
            harmonic_filter: at(base + 4),
            noise_filter: at(base + 5),
            third_filter: at(base + 6),
            synthesis_filterbank: at(base + 7),
            analysis_filterbank: at(base + 8),
            postfilter_network: at(base + 9),
            postfilter_projection: at(base + 10),
            recurrent_sampler: node.children.get(required).copied(),
        })
    }
}
