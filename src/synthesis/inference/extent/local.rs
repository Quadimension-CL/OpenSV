use crate::synthesis::voice::{PreparedModule, PreparedTree, Result, VoiceError};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Context {
    pub left: usize,
    pub right: usize,
}

impl Context {
    fn then(self, next: Self) -> Result<Self> {
        Ok(Self {
            left: self.left.checked_add(next.left).ok_or_else(overflow)?,
            right: self.right.checked_add(next.right).ok_or_else(overflow)?,
        })
    }

    fn union(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            right: self.right.max(other.right),
        }
    }
}

pub fn context(tree: &PreparedTree, index: usize) -> Result<Context> {
    let node = tree.node(index)?;
    match &node.module {
        PreparedModule::Unary(_)
        | PreparedModule::LeakyRelu(_)
        | PreparedModule::Affine(_)
        | PreparedModule::Embedding(_)
        | PreparedModule::LayerNorm(_)
        | PreparedModule::RmsNorm(_)
        | PreparedModule::ChannelAffine(_) => Ok(Context::default()),
        PreparedModule::Convolution(op) => {
            let config = op.config;
            let span = config
                .kernel
                .checked_sub(1)
                .and_then(|value| value.checked_mul(config.dilation))
                .ok_or_else(overflow)?;
            let padding = config.padding.checked_mul(2).ok_or_else(overflow)?;
            if config.stride != 1 || config.kernel == 0 || config.dilation == 0 || span != padding {
                return Err(unsupported(index));
            }
            Ok(Context {
                left: config.padding,
                right: span - config.padding,
            })
        }
        PreparedModule::Sequential => {
            let mut result = Context::default();
            for &child in &node.children {
                result = result.then(context(tree, child)?)?;
            }
            Ok(result)
        }
        PreparedModule::Gated(config) => {
            let expected = if config.condition_channels == 0 { 1 } else { 2 };
            if node.children.len() != expected {
                return Err(VoiceError::new("Invalid local-context gated children"));
            }

            context(tree, node.children[0])
        }
        PreparedModule::Conditional(config) => {
            let expected = if config.condition_channels == 0 { 3 } else { 4 };
            if node.children.len() != expected {
                return Err(VoiceError::new(
                    "Invalid local-context conditional children",
                ));
            }
            conditional(tree, &node.children)
        }

        _ => Err(unsupported(index)),
    }
}

fn conditional(tree: &PreparedTree, children: &[usize]) -> Result<Context> {
    let mut lists = Vec::with_capacity(3);
    for &child in &children[..3] {
        let list = tree.node(child)?;
        if !matches!(list.module, PreparedModule::ModuleList) {
            return Err(VoiceError::new("Local-context branch is not a module list"));
        }
        lists.push(list.children.as_slice());
    }
    let layers = lists[0].len();
    if layers == 0 || lists.iter().any(|list| list.len() != layers) {
        return Err(VoiceError::new("Invalid local-context conditional lists"));
    }

    let mut state = Context::default();
    let mut output = Context::default();
    for ((&gate_node, &skip_node), &residual_node) in lists[0].iter().zip(lists[1]).zip(lists[2]) {

        let gate = state.then(context(tree, gate_node)?)?;
        output = output.union(gate.then(context(tree, skip_node)?)?);
        let residual = state.then(context(tree, residual_node)?)?;
        state = gate.union(residual);
    }
    Ok(output)
}

fn unsupported(index: usize) -> VoiceError {
    VoiceError::new(format!(
        "Finite same-length local context is not established for model node {index}"
    ))
}

fn overflow() -> VoiceError {
    VoiceError::new("Local temporal context exceeds addressable memory")
}
