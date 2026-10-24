mod conditional;
mod gated;
mod queue;
mod sampler;

use super::ModelRuntime;
use crate::synthesis::{
    inference::Sequence,
    voice::{PreparedModule, Result, VoiceError},
};

#[derive(Clone)]
pub(super) struct CompositeState {
    program: Program,
    input_channels: usize,
    condition_channels: Option<usize>,
    input_frames: usize,
    condition_frames: usize,
}

#[derive(Clone)]
enum Program {
    Gated(gated::GatedState),
    Conditional(conditional::ConditionalState),
    Sampler(sampler::SamplerState),
}

impl CompositeState {
    fn accept(
        &mut self,
        input: &Sequence,
        condition: Option<&Sequence>,
        flush: bool,
    ) -> Result<()> {
        if input.channels != self.input_channels
            || condition.map(|sequence| sequence.channels) != self.condition_channels
        {
            return Err(VoiceError::new(
                "Composite input shape or condition presence changed",
            ));
        }
        self.input_frames = self
            .input_frames
            .checked_add(input.frames)
            .ok_or_else(|| VoiceError::new("Composite input frame count overflows"))?;
        self.condition_frames = self
            .condition_frames
            .checked_add(condition.map_or(0, |s| s.frames))
            .ok_or_else(|| VoiceError::new("Composite condition frame count overflows"))?;
        if flush && condition.is_some() && self.input_frames != self.condition_frames {
            return Err(VoiceError::new(
                "Final composite input and condition lengths differ",
            ));
        }
        Ok(())
    }
}

impl ModelRuntime<'_> {
    pub(super) fn composite(
        &mut self,
        index: usize,
        input: &Sequence,
        condition: Option<&Sequence>,
        flush: bool,
    ) -> Result<Sequence> {
        if self.closed.contains(&index) {
            return Err(VoiceError::new(
                "Composite stream is finalized; reset before reuse",
            ));
        }
        input.validate()?;
        if let Some(condition) = condition {
            condition.validate()?;
        }
        let tree = self.tree;
        let node = tree.node(index)?;

        let mut state = match self.composites.remove(&index) {
            Some(state) => state,
            None => {
                let program = match node.module {
                    PreparedModule::Gated(config) => {
                        check_inputs(
                            input,
                            condition,
                            config.input_channels,
                            config.condition_channels,
                        )?;
                        Program::Gated(gated::GatedState::new(config, condition.is_some())?)
                    }
                    PreparedModule::Conditional(config) => {
                        check_inputs(
                            input,
                            condition,
                            config.input_channels,
                            config.condition_channels,
                        )?;
                        Program::Conditional(conditional::ConditionalState::new(
                            tree,
                            &node.children,
                            config,
                        )?)
                    }
                    PreparedModule::SamplerNetwork => {
                        let condition = condition.ok_or_else(|| {
                            VoiceError::new("Sampler network requires a condition sequence")
                        })?;
                        if tree.payload(index)?.as_ref() != [4, 0, 0, 1, 0, 1, 0, 1, 0, 1]
                            || node.children.len() != 5
                        {
                            return Err(VoiceError::new(
                                "Unsupported sampler network configuration",
                            ));
                        }
                        Program::Sampler(sampler::SamplerState::new(
                            input.channels,
                            condition.channels,
                        )?)
                    }
                    _ => return Err(VoiceError::new("Expected a recovered composite operator")),
                };
                CompositeState {
                    program,
                    input_channels: input.channels,
                    condition_channels: condition.map(|s| s.channels),
                    input_frames: 0,
                    condition_frames: 0,
                }
            }
        };
        let output = (|| {
            state.accept(input, condition, flush)?;
            match &mut state.program {
                Program::Gated(program) => {
                    program.process(self, &node.children, input, condition, flush)
                }
                Program::Conditional(program) => {
                    program.process(self, &node.children, input, condition, flush)
                }
                Program::Sampler(program) => program.process(
                    self,
                    &node.children,
                    input,
                    condition.ok_or_else(|| VoiceError::new("Missing sampler condition"))?,
                    flush,
                ),
            }
        })();
        self.composites.insert(index, state);
        if output.is_ok() && flush {
            self.closed.insert(index);
        }
        output
    }
}

fn check_inputs(
    input: &Sequence,
    condition: Option<&Sequence>,
    channels: usize,
    condition_channels: usize,
) -> Result<()> {
    if input.channels != channels {
        return Err(VoiceError::new(
            "Composite input has an invalid channel count",
        ));
    }
    if let Some(condition) = condition
        && (condition_channels == 0 || condition.channels != condition_channels)
    {
        return Err(VoiceError::new(
            "Composite condition has an invalid channel count",
        ));
    }
    Ok(())
}
