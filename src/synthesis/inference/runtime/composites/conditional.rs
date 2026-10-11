use super::{ModelRuntime, queue::SumQueue};
use crate::synthesis::{
    inference::{Sequence, ops::conditional::ConditionalConfig},
    voice::{PreparedModule, PreparedTree, Result, VoiceError},
};

#[derive(Clone, Default)]
struct LayerState {
    residual: SumQueue,
    skip: SumQueue,
}

#[derive(Clone)]
pub(super) struct ConditionalState {
    config: ConditionalConfig,
    gates: Vec<usize>,
    skips: Vec<usize>,
    residuals: Vec<usize>,
    layers: Vec<LayerState>,
}

impl ConditionalState {
    pub(super) fn new(
        tree: &PreparedTree,
        children: &[usize],
        config: ConditionalConfig,
    ) -> Result<Self> {
        let expected = if config.condition_channels == 0 { 3 } else { 4 };
        if children.len() != expected {
            return Err(VoiceError::new(
                "Conditional stack has an invalid child count",
            ));
        }
        let mut lists = Vec::with_capacity(3);
        for &child in &children[..3] {
            let node = tree.node(child)?;
            if !matches!(node.module, PreparedModule::ModuleList) {
                return Err(VoiceError::new(
                    "Conditional branch is not an ownership list",
                ));
            }
            lists.push(node.children.clone());
        }
        let layers = lists[0].len();
        if layers == 0 || lists.iter().any(|list| list.len() != layers) {
            return Err(VoiceError::new(
                "Conditional branch lists have invalid lengths",
            ));
        }
        let mut lists = lists.into_iter();
        Ok(Self {
            config,
            gates: lists.next().expect("three lists"),
            skips: lists.next().expect("three lists"),
            residuals: lists.next().expect("three lists"),
            layers: vec![LayerState::default(); layers],
        })
    }

    pub(super) fn process(
        &mut self,
        runtime: &mut ModelRuntime<'_>,
        children: &[usize],
        input: &Sequence,
        condition: Option<&Sequence>,
        flush: bool,
    ) -> Result<Sequence> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("op.conditional");
        let mut output = match condition {
            Some(condition) => runtime.execute(children[3], condition, flush)?,
            None => Sequence::zeros(input.frames, self.config.output_channels)?,
        };
        if output.channels != self.config.output_channels {
            return Err(VoiceError::new(
                "Conditional direct projection has an invalid width",
            ));
        }
        let mut state = input.clone();
        for (index, layer) in self.layers.iter_mut().enumerate() {
            let residual = runtime.execute(self.residuals[index], &state, flush)?;
            let gated = match condition {
                Some(condition) => {
                    runtime.execute_conditioned(self.gates[index], &state, condition, flush)?
                }
                None => runtime.execute(self.gates[index], &state, flush)?,
            };
            // Skip always sees the gate before its residual addition.
            // Each sum releases only a matched prefix.
            let skip = runtime.execute(self.skips[index], &gated, flush)?;
            state = layer
                .residual
                .process(&gated, &residual, flush, "conditional residual")?;
            output = layer
                .skip
                .process(&output, &skip, flush, "conditional skip")?;
        }
        Ok(output)
    }
}
