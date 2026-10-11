//! Per-render execution state over shared, immutable prepared weights.
//!
//! `reset` begins a new independent sequence (including each diffusion step).
//! Convolution, recurrent and composite programs retain state between chunks.
//! Branch queues publish each aligned stable prefix before the final flush.

mod bidirectional;
mod composites;
mod shapes;

use super::{
    Sequence,
    convolution::{ConvolutionStream, TransposedConvolutionStream},
    ops::attention_relative::AttentionOptions,
    pooling::PoolStream,
    recurrent::GruState,
};
use crate::synthesis::voice::{PreparedModule, PreparedTree, Result, VoiceError};
use bidirectional::BidirectionalState;
use composites::CompositeState;
use std::collections::{HashMap, HashSet};

/// Clone creates an independent stream checkpoint while retaining shared weights.
#[derive(Clone)]
pub struct ModelRuntime<'a> {
    tree: &'a PreparedTree,
    convolutions: HashMap<usize, ConvolutionStream>,
    transposed: HashMap<usize, TransposedConvolutionStream>,
    recurrent: HashMap<usize, GruState>,
    bidirectional: HashMap<usize, BidirectionalState>,
    pools: HashMap<usize, PoolStream>,
    composites: HashMap<usize, CompositeState>,
    closed: HashSet<usize>,
}

impl<'a> ModelRuntime<'a> {
    pub fn new(tree: &'a PreparedTree) -> Self {
        Self {
            tree,
            convolutions: HashMap::new(),
            transposed: HashMap::new(),
            recurrent: HashMap::new(),
            bidirectional: HashMap::new(),
            pools: HashMap::new(),
            composites: HashMap::new(),
            closed: HashSet::new(),
        }
    }

    /// Discard all stream state. Immutable weights remain shared with the tree.
    pub fn reset(&mut self) {
        self.convolutions.clear();
        self.transposed.clear();
        self.recurrent.clear();
        self.bidirectional.clear();
        self.pools.clear();
        self.composites.clear();
        self.closed.clear();
    }

    pub fn execute(&mut self, index: usize, input: &Sequence, flush: bool) -> Result<Sequence> {
        crate::synthesis::execution::checkpoint()?;
        input.validate()?;
        let tree = self.tree;
        let node = tree.node(index)?;
        if let Some(channels) = self.input_channels(index)? {
            require_channels(input, channels)?;
        }
        let output = match &node.module {
            PreparedModule::Unary(operator) => {
                #[cfg(feature = "synth-profile")]
                let _profile = crate::synthesis::profiling::span("op.unary");
                let mut output = Sequence::zeros(input.frames, input.channels)?;
                operator
                    .execute(&input.values, &mut output.values)
                    .map_err(error)?;
                output
            }
            PreparedModule::LeakyRelu(operator) => {
                #[cfg(feature = "synth-profile")]
                let _profile = crate::synthesis::profiling::span("op.leaky_relu");
                let mut output = Sequence::zeros(input.frames, input.channels)?;
                operator
                    .execute(&input.values, &mut output.values)
                    .map_err(error)?;
                output
            }
            PreparedModule::Affine(operator) => {
                let mut output = Sequence::zeros(input.frames, operator.output_features)?;
                operator
                    .operator()
                    .execute_batch(&input.values, &mut output.values, input.frames)
                    .map_err(error)?;
                output
            }
            PreparedModule::Convolution(operator) => {
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    self.convolutions.entry(index)
                {
                    entry.insert(operator.stream()?);
                }
                let stream = self
                    .convolutions
                    .get_mut(&index)
                    .expect("inserted convolution state");
                Sequence::from_values(
                    operator.output_features,
                    stream.process(&input.values, flush).map_err(error)?,
                )?
            }
            PreparedModule::TransposedConvolution(operator) => {
                let stream = self
                    .transposed
                    .entry(index)
                    .or_insert_with(|| operator.stream());
                Sequence::from_values(
                    operator.output_features,
                    stream.process(&input.values, flush).map_err(error)?,
                )?
            }
            PreparedModule::Pool(config) => {
                if let std::collections::hash_map::Entry::Vacant(entry) = self.pools.entry(index) {
                    entry.insert(config.stream(input.channels).map_err(error)?);
                }
                let stream = self.pools.get_mut(&index).expect("inserted pooling state");
                require_channels(input, stream.channels())?;
                Sequence::from_values(
                    input.channels,
                    stream.process(&input.values, flush).map_err(error)?,
                )?
            }
            PreparedModule::Gru(operator) => {
                if self.closed.contains(&index) {
                    return Err(VoiceError::new(
                        "GRU sequence finished; reset before independent execution",
                    ));
                }
                let state = self
                    .recurrent
                    .entry(index)
                    .or_insert_with(|| operator.new_state());
                let mut output = Sequence::zeros(input.frames, operator.output_channels())?;
                operator
                    .execute_sequence(&input.values, &mut output.values, input.frames, state)
                    .map_err(error)?;
                if flush {
                    self.closed.insert(index);
                }
                output
            }
            PreparedModule::BidirectionalGru(operator) => {
                if self.closed.contains(&index) {
                    return Err(VoiceError::new(
                        "Bidirectional GRU sequence finished; reset before independent execution",
                    ));
                }
                let state = self
                    .bidirectional
                    .entry(index)
                    .or_insert_with(|| BidirectionalState::new(operator));
                let output = state.process(operator, input, flush)?;
                if flush {
                    self.closed.insert(index);
                }
                output
            }
            PreparedModule::Sequential => {
                let mut output = input.clone();
                for &child in &node.children {
                    output = self.execute(child, &output, flush)?;
                }
                output
            }
            PreparedModule::Embedding(operator) => {
                #[cfg(feature = "synth-profile")]
                let _profile = crate::synthesis::profiling::span("op.embedding");
                let mut output = Sequence::zeros(input.frames, operator.features())?;
                operator
                    .execute(&input.values, &mut output.values)
                    .map_err(error)?;
                output
            }
            PreparedModule::LayerNorm(operator) => {
                let mut output = Sequence::zeros(input.frames, input.channels)?;
                operator
                    .execute_batch(&input.values, &mut output.values, input.frames)
                    .map_err(error)?;
                output
            }
            PreparedModule::ChannelAffine(operator) => {
                let mut output = Sequence::zeros(input.frames, input.channels)?;
                operator
                    .execute_batch(&input.values, &mut output.values, input.frames)
                    .map_err(error)?;
                output
            }
            PreparedModule::Attention(operator) => {
                if !flush {
                    return Err(VoiceError::new(
                        "Attention requires an explicit windowed stage or complete sequence",
                    ));
                }
                Sequence::new(
                    input.frames,
                    operator.config().output_channels,
                    operator
                        .execute(
                            &input.values,
                            &input.values,
                            &input.values,
                            AttentionOptions::default(),
                        )
                        .map_err(error)?,
                )?
            }
            PreparedModule::Conditional(_)
            | PreparedModule::Gated(_)
            | PreparedModule::SamplerNetwork => self.composite(index, input, None, flush)?,
            PreparedModule::RangeNormalizer(_) => {
                return Err(VoiceError::new(
                    "Range normalization needs an explicit normalize or denormalize direction",
                ));
            }
            PreparedModule::StandardNormalizer(_) => {
                return Err(VoiceError::new(
                    "Standard normalization needs an explicit normalize or denormalize direction",
                ));
            }
            PreparedModule::RelativeAttention(_) => {
                return Err(VoiceError::new(
                    "Relative attention requires explicit query/key/value and head dimensions",
                ));
            }
            PreparedModule::ModuleList => {
                return Err(VoiceError::new(
                    "An ownership list is not an executable sequential program",
                ));
            }
            PreparedModule::Tensor(_) | PreparedModule::PhoneSet(_) => {
                return Err(VoiceError::new("Model data is not an executable operator"));
            }
            PreparedModule::Unsupported => {
                return Err(VoiceError::new(format!(
                    "Execution is not implemented for model node {index} type {:02x?}",
                    node.type_id
                )));
            }
        };
        output.validate()?;
        Ok(output)
    }

    pub fn execute_conditioned(
        &mut self,
        index: usize,
        input: &Sequence,
        condition: &Sequence,
        flush: bool,
    ) -> Result<Sequence> {
        crate::synthesis::execution::checkpoint()?;
        input.validate()?;
        condition.validate()?;
        match &self.tree.node(index)?.module {
            PreparedModule::Conditional(_)
            | PreparedModule::Gated(_)
            | PreparedModule::SamplerNetwork => {
                self.composite(index, input, Some(condition), flush)
            }
            _ => Err(VoiceError::new(
                "This operator does not accept a conditioning stream",
            )),
        }
    }
}

fn require_channels(sequence: &Sequence, channels: usize) -> Result<()> {
    if sequence.channels != channels {
        return Err(VoiceError::new(format!(
            "Model expects {channels} channels; received {}",
            sequence.channels
        )));
    }
    Ok(())
}

fn error(error: impl std::fmt::Display) -> VoiceError {
    VoiceError::new(error.to_string())
}
