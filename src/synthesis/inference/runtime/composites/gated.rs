use super::{ModelRuntime, queue::FrameQueue};
use crate::synthesis::{
    inference::{
        Sequence,
        ops::gated::{self, GatedConfig},
    },
    voice::{Result, VoiceError},
};

#[derive(Clone)]
pub(super) struct GatedState {
    config: GatedConfig,
    main: FrameQueue,
    condition: Option<FrameQueue>,
}

impl GatedState {
    pub(super) fn new(config: GatedConfig, conditioned: bool) -> Result<Self> {
        let width = config
            .output_channels
            .checked_mul(2)
            .ok_or_else(|| VoiceError::new("Gated projection width overflows"))?;
        Ok(Self {
            config,
            main: FrameQueue::new(width),
            condition: conditioned.then(|| FrameQueue::new(width)),
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
        let _profile = crate::synthesis::profiling::span("op.gated");
        let expected = if self.config.condition_channels == 0 {
            1
        } else {
            2
        };
        if children.len() != expected {
            return Err(VoiceError::new("Gated block has an invalid child count"));
        }
        self.main
            .push(&runtime.execute(children[0], input, flush)?)?;
        if let (Some(queue), Some(condition)) = (&mut self.condition, condition) {
            queue.push(&runtime.execute(children[1], condition, flush)?)?;
        }
        let frames = self.condition.as_ref().map_or(self.main.frames(), |queue| {
            queue.frames().min(self.main.frames())
        });
        let main = self.main.take(frames)?;
        let condition = self
            .condition
            .as_mut()
            .map(|queue| queue.take(frames))
            .transpose()?;
        let mut output = Sequence::zeros(frames, self.config.output_channels)?;
        let activate=if runtime.tree.legacy_numeric(){gated::activate_projected_legacy}else{gated::activate_projected};
        activate(
            &main.values,
            condition
                .as_ref()
                .map(|sequence| sequence.values.as_slice()),
            &mut output.values,
            self.config.output_channels,
        )
        .map_err(|error| VoiceError::new(error.to_string()))?;
        if flush {
            self.main.ensure_empty("gated main projection")?;
            if let Some(queue) = &self.condition {
                queue.ensure_empty("gated condition projection")?;
            }
        }
        Ok(output)
    }
}
