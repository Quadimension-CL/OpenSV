use super::ModelRuntime;
use crate::synthesis::voice::{PreparedModule, Result};

impl ModelRuntime<'_> {

    pub fn input_channels(&self, index: usize) -> Result<Option<usize>> {
        let node = self.tree.node(index)?;
        Ok(match &node.module {
            PreparedModule::Affine(layer) => Some(layer.input_features),
            PreparedModule::Convolution(layer) => Some(layer.input_features),
            PreparedModule::TransposedConvolution(layer) => Some(layer.input_features),
            PreparedModule::Gru(layer) => Some(layer.input_channels()),
            PreparedModule::BidirectionalGru(layer) => Some(layer.input_channels()),
            PreparedModule::LayerNorm(layer) => Some(layer.channels()),
            PreparedModule::RmsNorm(layer) => Some(layer.channels()),
            PreparedModule::ChannelAffine(layer) => Some(layer.channels()),
            PreparedModule::RangeNormalizer(layer) => Some(layer.channels()),
            PreparedModule::StandardNormalizer(layer) => Some(layer.channels()),
            PreparedModule::Embedding(_) => Some(1),
            PreparedModule::Attention(layer) => Some(layer.config().query_channels),
            PreparedModule::Conditional(config) => Some(config.input_channels),
            PreparedModule::Gated(config) => Some(config.input_channels),
            PreparedModule::SamplerNetwork => {
                if let Some(&child) = node.children.first() {
                    self.input_channels(child)?
                } else {
                    None
                }
            }
            PreparedModule::Sequential => {
                let mut result = None;
                for &child in &node.children {
                    if let Some(channels) = self.input_channels(child)? {
                        result = Some(channels);
                        break;
                    }
                }
                result
            }
            _ => None,
        })
    }
    pub(crate) fn flat_elementwise(&self, index: usize) -> Result<bool> {
        let node = self.tree.node(index)?;
        match &node.module {
            PreparedModule::Unary(_) | PreparedModule::LeakyRelu(_) => Ok(true),
            PreparedModule::Sequential => {
                for &child in &node.children {
                    if !self.flat_elementwise(child)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
