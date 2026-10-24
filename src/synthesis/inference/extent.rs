use crate::synthesis::voice::{PreparedModule, PreparedTree, Result, VoiceError};

pub mod local;

#[derive(Clone, Copy, Debug, Default)]
pub struct InputExtent {
    pub main: usize,
    pub condition: usize,
}

impl InputExtent {
    pub fn shared(self) -> usize {
        self.main.max(self.condition)
    }
}

pub fn required_input(
    tree: &PreparedTree,
    index: usize,
    output: usize,
    conditioned: bool,
) -> Result<InputExtent> {
    if output == 0 {
        return Ok(InputExtent::default());
    }
    let node = tree.node(index)?;
    let main = match &node.module {
        PreparedModule::Unary(_)
        | PreparedModule::LeakyRelu(_)
        | PreparedModule::Affine(_)
        | PreparedModule::Gru(_)
        | PreparedModule::Embedding(_)
        | PreparedModule::LayerNorm(_)
        | PreparedModule::RmsNorm(_)
        | PreparedModule::ChannelAffine(_) => output,
        PreparedModule::Convolution(op) => convolution_input(
            output,
            op.config.kernel,
            op.config.stride,
            op.config.dilation,
            op.config.padding,
        )?,
        PreparedModule::Pool(op) => {
            convolution_input(output, op.kernel, op.stride, op.dilation, op.left_padding)?
        }
        PreparedModule::TransposedConvolution(op) => {

            let padding = op.config.padding.checked_mul(2).ok_or_else(overflow)?;
            output
                .checked_add(padding)
                .ok_or_else(overflow)?
                .div_ceil(op.config.stride)
        }
        PreparedModule::Sequential => {
            let mut needed = output;
            for &child in node.children.iter().rev() {
                needed = required_input(tree, child, needed, false)?.main;
            }
            needed
        }
        PreparedModule::Gated(_) => {
            let main = required_input(tree, child(&node.children, 0)?, output, false)?.main;
            let condition = if conditioned {
                required_input(tree, child(&node.children, 1)?, output, false)?.main
            } else {
                0
            };
            return Ok(InputExtent { main, condition });
        }
        PreparedModule::Conditional(_) => {
            return conditional(tree, &node.children, output, conditioned);
        }
        PreparedModule::SamplerNetwork if conditioned => {
            return sampler(tree, &node.children, output);
        }
        _ => {
            return Err(VoiceError::new(format!(
                "Finite output-prefix dependency is not established for model node {index}"
            )));
        }
    };
    Ok(InputExtent { main, condition: 0 })
}

fn conditional(
    tree: &PreparedTree,
    children: &[usize],
    output: usize,
    conditioned: bool,
) -> Result<InputExtent> {
    let lists = [
        &tree.node(child(children, 0)?)?.children,
        &tree.node(child(children, 1)?)?.children,
        &tree.node(child(children, 2)?)?.children,
    ];
    let layers = lists[0].len();
    if layers == 0 || lists.iter().any(|list| list.len() != layers) {
        return Err(VoiceError::new("Invalid conditional dependency lists"));
    }
    let mut needed = InputExtent {
        main: 0,
        condition: if conditioned {
            required_input(tree, child(children, 3)?, output, false)?.main
        } else {
            0
        },
    };

    for layer in (0..layers).rev() {
        let skip = required_input(tree, lists[1][layer], output, false)?.main;
        let gate = required_input(tree, lists[0][layer], skip.max(needed.main), conditioned)?;
        let residual = required_input(tree, lists[2][layer], needed.main, false)?.main;
        needed.main = gate.main.max(residual);
        needed.condition = needed.condition.max(gate.condition);
    }
    Ok(needed)
}

fn sampler(tree: &PreparedTree, children: &[usize], output: usize) -> Result<InputExtent> {
    let final_layer = required_input(tree, child(children, 4)?, output, true)?;
    let low_output = required_input(tree, child(children, 2)?, final_layer.main, false)?.main;
    let low = required_input(tree, child(children, 3)?, low_output, true)?;
    let main = required_input(tree, child(children, 0)?, low.main, false)?.main;
    let condition = required_input(tree, child(children, 1)?, low.condition, false)?.main;
    Ok(InputExtent {
        main: main.max(final_layer.main),
        condition: condition.max(final_layer.condition),
    })
}

fn convolution_input(
    output: usize,
    kernel: usize,
    stride: usize,
    dilation: usize,
    left_padding: usize,
) -> Result<usize> {
    let last = (output - 1).checked_mul(stride).ok_or_else(overflow)?;
    let taps = (kernel - 1).checked_mul(dilation).ok_or_else(overflow)?;
    Ok(last
        .checked_add(taps)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(overflow)?
        .saturating_sub(left_padding))
}

fn child(children: &[usize], index: usize) -> Result<usize> {
    children
        .get(index)
        .copied()
        .ok_or_else(|| VoiceError::new("Missing temporal dependency child"))
}

fn overflow() -> VoiceError {
    VoiceError::new("Temporal dependency extent exceeds addressable memory")
}
