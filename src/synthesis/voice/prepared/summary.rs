use super::{DurationInputLayout, PreparedModule, PreparedTree, TensorLayout, hex};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct PreparedTreeSummary {
    pub nodes: usize,
    pub root: usize,
    pub tensor_nodes: usize,
    pub unary_nodes: usize,
    pub affine_nodes: usize,
    pub phone_set_nodes: usize,
    pub prepared_operator_counts: BTreeMap<String, usize>,
    pub phone_sets: Vec<PhoneSetSummary>,
    pub unsupported_nodes: usize,
    pub unsupported_types: BTreeMap<String, usize>,
    pub tensor_shapes: Vec<TensorShapeSummary>,
    pub executable_nodes: Vec<ExecutableNodeSummary>,
    pub root_supported: bool,
    pub complete_tree_execution_available: bool,
    pub duration_input: Option<DurationInputLayout>,
}

#[derive(Serialize)]
pub struct PhoneSetSummary {
    pub node_index: usize,
    pub name: String,
    pub symbols: usize,
    pub classes: usize,
    pub mapped_symbols: usize,
}

#[derive(Serialize)]
pub struct TensorShapeSummary {
    pub layout: TensorLayout,
    pub nodes: usize,
}

#[derive(Serialize)]
pub struct ExecutableNodeSummary {
    pub node_index: usize,
    pub resource_index: usize,
    pub operator: String,
    pub input_features: Option<usize>,
    pub output_features: Option<usize>,
    pub bias: bool,
}

impl PreparedTree {
    pub fn summary(&self) -> PreparedTreeSummary {
        let mut summary = PreparedTreeSummary {
            nodes: self.nodes.len(),
            root: self.root,
            tensor_nodes: 0,
            unary_nodes: 0,
            affine_nodes: 0,
            unsupported_nodes: 0,
            phone_set_nodes: 0,
            prepared_operator_counts: BTreeMap::new(),
            phone_sets: Vec::new(),
            unsupported_types: BTreeMap::new(),
            tensor_shapes: Vec::new(),
            executable_nodes: Vec::new(),
            root_supported: !matches!(self.nodes[self.root].module, PreparedModule::Unsupported),
            // Containment alone does not identify the stage's forward program.
            complete_tree_execution_available: false,
            duration_input: self.duration_input.clone(),
        };
        let mut shapes = BTreeMap::<_, TensorShapeSummary>::new();
        for (index, node) in self.nodes.iter().enumerate() {
            match &node.module {
                PreparedModule::Tensor(storage) => {
                    summary.tensor_nodes += 1;
                    let layout = storage.layout();
                    let key = (
                        layout.storage,
                        layout.rows,
                        layout.columns,
                        layout.stored_bits,
                    );
                    shapes
                        .entry(key)
                        .or_insert_with(|| TensorShapeSummary { layout, nodes: 0 })
                        .nodes += 1;
                }
                PreparedModule::Unary(operator) => {
                    summary.unary_nodes += 1;
                    summary.executable_nodes.push(ExecutableNodeSummary {
                        node_index: index,
                        resource_index: node.resource_index,
                        operator: format!("{operator:?}").to_lowercase(),
                        input_features: None,
                        output_features: None,
                        bias: false,
                    });
                }
                PreparedModule::Affine(affine) => {
                    summary.affine_nodes += 1;
                    summary.executable_nodes.push(ExecutableNodeSummary {
                        node_index: index,
                        resource_index: node.resource_index,
                        operator: "affine".to_owned(),
                        input_features: Some(affine.input_features),
                        output_features: Some(affine.output_features),
                        bias: affine.bias.is_some(),
                    });
                }
                PreparedModule::PhoneSet(phones) => {
                    summary.phone_set_nodes += 1;
                    summary.phone_sets.push(PhoneSetSummary {
                        node_index: index,
                        name: phones.name.clone(),
                        symbols: phones.symbols.len(),
                        classes: phones.classes.len(),
                        mapped_symbols: phones.mapped_symbols.len(),
                    });
                }
                PreparedModule::ModuleList => {
                    *summary
                        .prepared_operator_counts
                        .entry("module_list".into())
                        .or_default() += 1;
                }
                module if descriptor(module).is_some() => {
                    let (name, input, output, bias) =
                        descriptor(module).expect("descriptor checked");
                    *summary
                        .prepared_operator_counts
                        .entry(name.into())
                        .or_default() += 1;
                    summary.executable_nodes.push(ExecutableNodeSummary {
                        node_index: index,
                        resource_index: node.resource_index,
                        operator: name.into(),
                        input_features: input,
                        output_features: output,
                        bias,
                    });
                }
                _ => {
                    summary.unsupported_nodes += 1;
                    *summary
                        .unsupported_types
                        .entry(hex(&node.type_id))
                        .or_default() += 1;
                }
            }
        }
        summary.tensor_shapes = shapes.into_values().collect();
        summary
    }
}

fn descriptor(
    module: &PreparedModule,
) -> Option<(&'static str, Option<usize>, Option<usize>, bool)> {
    Some(match module {
        PreparedModule::LeakyRelu(_) => ("leaky_relu", None, None, false),
        PreparedModule::Pool(_) => ("temporal_pool", None, None, false),
        PreparedModule::Convolution(op) => (
            "convolution",
            Some(op.input_features),
            Some(op.output_features),
            op.bias.is_some(),
        ),
        PreparedModule::TransposedConvolution(op) => (
            "transposed_convolution",
            Some(op.input_features),
            Some(op.output_features),
            op.bias.is_some(),
        ),
        PreparedModule::Gru(op) => (
            "gru",
            Some(op.input_channels()),
            Some(op.output_channels()),
            true,
        ),
        PreparedModule::BidirectionalGru(op) => (
            "bidirectional_gru",
            Some(op.input_channels()),
            Some(op.output_channels()),
            true,
        ),
        PreparedModule::Sequential => ("sequential", None, None, false),
        PreparedModule::Embedding(op) => ("embedding", Some(1), Some(op.features()), false),
        PreparedModule::LayerNorm(op) => (
            "layer_norm",
            Some(op.channels()),
            Some(op.channels()),
            false,
        ),
        PreparedModule::ChannelAffine(op) => (
            "channel_affine",
            Some(op.channels()),
            Some(op.channels()),
            true,
        ),
        PreparedModule::RangeNormalizer(op) => (
            "range_normalizer",
            Some(op.channels()),
            Some(op.channels()),
            false,
        ),
        PreparedModule::StandardNormalizer(op) => (
            "standard_normalizer",
            Some(op.channels()),
            Some(op.channels()),
            false,
        ),
        PreparedModule::RelativeAttention(_) => ("relative_attention", None, None, false),
        PreparedModule::Attention(op) => (
            "attention",
            Some(op.config().query_channels),
            Some(op.config().output_channels),
            false,
        ),
        PreparedModule::Conditional(config) => (
            "conditional",
            Some(config.input_channels),
            Some(config.output_channels),
            false,
        ),
        PreparedModule::Gated(config) => (
            "gated",
            Some(config.input_channels),
            Some(config.output_channels),
            false,
        ),
        PreparedModule::SamplerNetwork => ("sampler_network", None, Some(256), false),
        _ => return None,
    })
}
