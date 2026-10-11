//! Preparation of typed module schemas. Ownership lists never imply a
//! forward program; only the explicit Sequential type has sequential semantics.

use std::sync::Arc;

use crate::synthesis::inference::{
    affine::Affine,
    matrix::DenseMatrix,
    ops::{
        MODULE_LIST_TYPE_ID, SEQUENTIAL_TYPE_ID,
        attention::{AttentionConfig, MULTI_HEAD_ATTENTION_TYPE_ID, MultiHeadAttention},
        attention_relative::{RELATIVE_ATTENTION_TYPE_ID, RelativeAttention},
        bidirectional::{BIDIRECTIONAL_GRU_TYPE_ID, BidirectionalGru},
        conditional::{CONDITIONAL_TYPE_ID, ConditionalConfig},
        embedding::{EMBEDDING_TYPE_ID, Embedding},
        gated::{GATED_TYPE_ID, GatedConfig},
        normalization::{
            CHANNEL_AFFINE_TYPE_ID, ChannelAffine, LAYER_NORM_TYPE_ID, LayerNorm, LayerNormConfig,
        },
        parameterized::{LEAKY_RELU_TYPE_ID, LeakyRelu},
        range_normalizer::{RANGE_NORMALIZER_TYPE_ID, RangeNormalizer},
        sampler_network::SAMPLER_NETWORK_TYPE_ID,
        standard_normalizer::{STANDARD_NORMALIZER_TYPE_ID, StandardNormalizer},
    },
    pooling::{POOL_TYPE_ID, PoolConfig},
    recurrent::{GRU_TYPE_ID, Gru},
};

use super::{PreparedModule, PreparedNode, Result, TensorStorage, VoiceError};

pub(super) fn prepare_operator(
    type_id: [u8; 16],
    payload: &[u8],
    nodes: &[PreparedNode],
    children: &[usize],
) -> Result<Option<PreparedModule>> {
    let module = match type_id {
        POOL_TYPE_ID => {
            count(children, 0, "Temporal pooling")?;
            PreparedModule::Pool(PoolConfig::parse(payload).map_err(voice_error)?)
        }
        LEAKY_RELU_TYPE_ID => {
            count(children, 0, "Leaky ReLU")?;
            PreparedModule::LeakyRelu(LeakyRelu::from_payload(payload).map_err(voice_error)?)
        }
        LAYER_NORM_TYPE_ID => {
            let config = LayerNormConfig::parse(payload).map_err(voice_error)?;
            count(children, if config.affine { 2 } else { 0 }, "LayerNorm")?;
            let (gain, bias) = if config.affine {
                (
                    Some(vector(nodes, children[0])?),
                    Some(vector(nodes, children[1])?),
                )
            } else {
                (None, None)
            };
            PreparedModule::LayerNorm(
                LayerNorm::new(config.channels, config.epsilon, gain, bias).map_err(voice_error)?,
            )
        }
        CHANNEL_AFFINE_TYPE_ID => {
            empty(payload)?;
            count(children, 2, "Channel affine")?;
            PreparedModule::ChannelAffine(
                ChannelAffine::new(vector(nodes, children[0])?, vector(nodes, children[1])?)
                    .map_err(voice_error)?,
            )
        }
        RANGE_NORMALIZER_TYPE_ID => {
            empty(payload)?;
            count(children, 3, "Range normalizer")?;
            PreparedModule::RangeNormalizer(
                RangeNormalizer::new(
                    vector(nodes, children[0])?,
                    vector(nodes, children[1])?,
                    &vector(nodes, children[2])?,
                )
                .map_err(voice_error)?,
            )
        }
        STANDARD_NORMALIZER_TYPE_ID => {
            empty(payload)?;
            count(children, 2, "Standard normalizer")?;
            PreparedModule::StandardNormalizer(
                StandardNormalizer::new(vector(nodes, children[0])?, vector(nodes, children[1])?)
                    .map_err(voice_error)?,
            )
        }
        EMBEDDING_TYPE_ID => {
            empty(payload)?;
            count(children, 1, "Embedding")?;
            PreparedModule::Embedding(Embedding::new(&matrix(nodes, children[0])?))
        }
        RELATIVE_ATTENTION_TYPE_ID => {
            PreparedModule::RelativeAttention(relative_attention(payload, nodes, children)?)
        }
        MULTI_HEAD_ATTENTION_TYPE_ID => {
            let config = AttentionConfig::from_payload(payload).map_err(voice_error)?;
            count(children, 5, "Projected attention")?;
            let PreparedModule::RelativeAttention(attention) = &node(nodes, children[4])?.module
            else {
                return Err(VoiceError::new(
                    "Projected attention requires a relative attention core",
                ));
            };
            PreparedModule::Attention(
                MultiHeadAttention::new(
                    config,
                    affine(nodes, children[0])?,
                    affine(nodes, children[1])?,
                    affine(nodes, children[2])?,
                    affine(nodes, children[3])?,
                    attention.clone(),
                )
                .map_err(voice_error)?,
            )
        }
        GRU_TYPE_ID => {
            empty(payload)?;
            count(children, 12, "GRU")?;
            let layers = children
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    Affine::new(matrix(nodes, pair[0])?, Some(vector(nodes, pair[1])?))
                        .map_err(voice_error)
                })
                .collect::<Result<Vec<_>>>()?;
            let layers = layers
                .try_into()
                .map_err(|_| VoiceError::new("GRU needs six affine pairs"))?;
            PreparedModule::Gru(Gru::new(layers).map_err(voice_error)?)
        }
        BIDIRECTIONAL_GRU_TYPE_ID => {
            empty(payload)?;
            count(children, 2, "Bidirectional GRU")?;
            let gru = |index| match &node(nodes, index)?.module {
                PreparedModule::Gru(operator) => Ok(operator.clone()),
                _ => Err(VoiceError::new(
                    "Bidirectional GRU requires two GRU children",
                )),
            };
            PreparedModule::BidirectionalGru(Box::new(
                BidirectionalGru::new(gru(children[0])?, gru(children[1])?).map_err(voice_error)?,
            ))
        }
        SEQUENTIAL_TYPE_ID => {
            empty(payload)?;
            PreparedModule::Sequential
        }
        MODULE_LIST_TYPE_ID => {
            empty(payload)?;
            PreparedModule::ModuleList
        }
        CONDITIONAL_TYPE_ID => {
            let config = ConditionalConfig::parse(payload).map_err(voice_error)?;
            validate_conditional(config, nodes, children)?;
            PreparedModule::Conditional(config)
        }
        GATED_TYPE_ID => {
            let config = GatedConfig::parse(payload).map_err(voice_error)?;
            count(
                children,
                if config.condition_channels == 0 { 1 } else { 2 },
                "Gated block",
            )?;
            convolution_shape(
                nodes,
                children[0],
                config.input_channels,
                config.output_channels * 2,
            )?;
            if config.condition_channels != 0 {
                convolution_shape(
                    nodes,
                    children[1],
                    config.condition_channels,
                    config.output_channels * 2,
                )?;
            }
            PreparedModule::Gated(config)
        }
        SAMPLER_NETWORK_TYPE_ID => {
            if payload != [4, 0, 0, 1, 0, 1, 0, 1, 0, 1] {
                return Err(VoiceError::new("Unsupported sampler network configuration"));
            }
            count(children, 5, "Sampler network")?;
            for &index in &children[..3] {
                if !matches!(node(nodes, index)?.module, PreparedModule::Sequential) {
                    return Err(VoiceError::new(
                        "Sampler resampling child must be a Sequential module",
                    ));
                }
            }
            for &index in &children[3..] {
                if !matches!(node(nodes, index)?.module, PreparedModule::Conditional(_)) {
                    return Err(VoiceError::new(
                        "Sampler latent child must be a conditional stack",
                    ));
                }
            }
            PreparedModule::SamplerNetwork
        }
        _ => return Ok(None),
    };
    // Sequential and ModuleList accept any number of children. They still need
    // valid arena indices, including when this factory is called independently.
    for &index in children {
        node(nodes, index)?;
    }
    Ok(Some(module))
}

fn relative_attention(
    payload: &[u8],
    nodes: &[PreparedNode],
    children: &[usize],
) -> Result<RelativeAttention> {
    match payload {
        [0, 0, 0, 0] => {
            count(children, 0, "Plain relative attention")?;
            RelativeAttention::new(None, None).map_err(voice_error)
        }
        [1, 0, 0, 0] => {
            count(children, 2, "Relative attention")?;
            let key = embedding_matrix(nodes, children[0])?;
            let value = embedding_matrix(nodes, children[1])?;
            RelativeAttention::new(Some(&key), Some(&value)).map_err(voice_error)
        }
        _ => Err(VoiceError::new(
            "Unsupported relative attention configuration",
        )),
    }
}

fn validate_conditional(
    config: ConditionalConfig,
    nodes: &[PreparedNode],
    children: &[usize],
) -> Result<()> {
    count(
        children,
        if config.condition_channels == 0 { 3 } else { 4 },
        "Conditional stack",
    )?;
    let lists = children[..3]
        .iter()
        .map(|&index| {
            let list = node(nodes, index)?;
            if !matches!(list.module, PreparedModule::ModuleList) {
                return Err(VoiceError::new(
                    "Conditional branch must be an ownership list",
                ));
            }
            Ok(list.children.as_slice())
        })
        .collect::<Result<Vec<_>>>()?;
    if lists[0].is_empty() || lists[1].len() != lists[0].len() || lists[2].len() != lists[0].len() {
        return Err(VoiceError::new(
            "Conditional branch lists must have the same nonzero length",
        ));
    }
    let mut input_channels = config.input_channels;
    for ((&gate_node, &skip), &residual) in lists[0].iter().zip(lists[1]).zip(lists[2]) {
        let PreparedModule::Gated(gate) = node(nodes, gate_node)?.module else {
            return Err(VoiceError::new(
                "Conditional gate list contains a non-gated module",
            ));
        };
        if gate.input_channels != input_channels
            || gate.condition_channels != config.condition_channels
        {
            return Err(VoiceError::new(
                "Conditional gate channels differ from its branch inputs",
            ));
        }
        convolution_shape(nodes, skip, gate.output_channels, config.output_channels)?;
        convolution_shape(nodes, residual, input_channels, gate.output_channels)?;
        input_channels = gate.output_channels;
    }
    if config.condition_channels != 0 {
        convolution_shape(
            nodes,
            children[3],
            config.condition_channels,
            config.output_channels,
        )?;
    }
    Ok(())
}

fn convolution_shape(
    nodes: &[PreparedNode],
    index: usize,
    input: usize,
    output: usize,
) -> Result<()> {
    let PreparedModule::Convolution(convolution) = &node(nodes, index)?.module else {
        return Err(VoiceError::new(
            "Conditional projection must be a convolution",
        ));
    };
    if convolution.input_features != input || convolution.output_features != output {
        return Err(VoiceError::new(
            "Conditional convolution has mismatched channel dimensions",
        ));
    }
    Ok(())
}

fn embedding_matrix(nodes: &[PreparedNode], index: usize) -> Result<DenseMatrix> {
    let embedding = node(nodes, index)?;
    if !matches!(embedding.module, PreparedModule::Embedding(_)) {
        return Err(VoiceError::new(
            "Relative positions must use embedding wrappers",
        ));
    }
    count(&embedding.children, 1, "Relative position embedding")?;
    let index = embedding.children[0];
    if !matches!(tensor(nodes, index)?, TensorStorage::FloatMatrix { .. }) {
        return Err(VoiceError::new(
            "Relative positions require float matrix storage",
        ));
    }
    matrix(nodes, index)
}

fn affine(nodes: &[PreparedNode], index: usize) -> Result<Affine> {
    match &node(nodes, index)?.module {
        PreparedModule::Affine(affine) => Ok(affine.operator().clone()),
        _ => Err(VoiceError::new(
            "Attention projection must be an affine module",
        )),
    }
}

fn matrix(nodes: &[PreparedNode], index: usize) -> Result<DenseMatrix> {
    tensor(nodes, index)?.dense()
}

fn vector(nodes: &[PreparedNode], index: usize) -> Result<Arc<[f32]>> {
    match tensor(nodes, index)? {
        TensorStorage::FloatVector { values } => Ok(Arc::clone(values)),
        _ => Err(VoiceError::new(
            "Prepared operator requires float vector storage",
        )),
    }
}

fn tensor(nodes: &[PreparedNode], index: usize) -> Result<&TensorStorage> {
    match &node(nodes, index)?.module {
        PreparedModule::Tensor(tensor) => Ok(tensor),
        _ => Err(VoiceError::new(
            "Prepared operator child must be tensor storage",
        )),
    }
}

fn node(nodes: &[PreparedNode], index: usize) -> Result<&PreparedNode> {
    nodes
        .get(index)
        .ok_or_else(|| VoiceError::new("Prepared operator child is outside its arena"))
}

fn count(children: &[usize], expected: usize, name: &str) -> Result<()> {
    if children.len() != expected {
        return Err(VoiceError::new(format!(
            "{name} requires {expected} children, received {}",
            children.len()
        )));
    }
    Ok(())
}

fn empty(payload: &[u8]) -> Result<()> {
    if !payload.is_empty() {
        return Err(VoiceError::new(
            "Parameterless operator has unexpected payload",
        ));
    }
    Ok(())
}

fn voice_error(error: impl std::fmt::Display) -> VoiceError {
    VoiceError::new(error.to_string())
}
