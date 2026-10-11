//! Persistent two-rate sampler queues.

use super::queue::FrameQueue;
use crate::synthesis::{
    inference::{Sequence, runtime::ModelRuntime},
    voice::{Result, VoiceError},
};

const RATE: usize = 4;
const CHANNELS: usize = 256;

#[derive(Clone)]
pub(super) struct SamplerState {
    main: FrameQueue,
    condition: FrameQueue,
    upsampled: FrameQueue,
    last_upsampled: Option<Vec<f32>>,
    totals: Counts,
}

#[derive(Clone, Default)]
struct Counts {
    main: usize,
    condition: usize,
    down_main: usize,
    down_condition: usize,
    latent: usize,
    upsampled: usize,
    output: usize,
}

impl SamplerState {
    pub(super) fn new(input_channels: usize, condition_channels: usize) -> Result<Self> {
        if input_channels == 0 || condition_channels == 0 {
            return Err(VoiceError::new("Sampler input channels must be positive"));
        }
        CHANNELS
            .checked_add(input_channels)
            .ok_or_else(|| VoiceError::new("Sampler concatenation width overflow"))?;
        Ok(Self {
            main: FrameQueue::new(input_channels),
            condition: FrameQueue::new(condition_channels),
            upsampled: FrameQueue::new(CHANNELS),
            last_upsampled: None,
            totals: Counts::default(),
        })
    }

    pub(super) fn process(
        &mut self,
        runtime: &mut ModelRuntime<'_>,
        children: &[usize],
        input: &Sequence,
        condition: &Sequence,
        flush: bool,
    ) -> Result<Sequence> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("op.sampler_network");
        if children.len() != 5 {
            return Err(VoiceError::new(
                "Sampler network has an invalid child count",
            ));
        }
        self.main.push(input)?;
        self.condition.push(condition)?;
        add_frames(&mut self.totals.main, input.frames)?;
        add_frames(&mut self.totals.condition, condition.frames)?;
        if flush {
            if self.totals.main != self.totals.condition {
                return Err(VoiceError::new(
                    "Sampler input and condition lengths differ",
                ));
            }
            if self.totals.main == 0 {
                return Sequence::zeros(0, CHANNELS);
            }
            if self.totals.main < RATE {
                return Err(VoiceError::new(
                    "Sampler network requires at least four conditioning frames",
                ));
            }
        }
        if !flush && input.frames == 0 && condition.frames == 0 {
            return Sequence::zeros(0, CHANNELS);
        }

        // Each downsampling branch has its own lookahead and may release a
        // different number of frames. The conditioned child aligns its queues.
        let down_main = runtime.execute(children[0], input, flush)?;
        count_output(&mut self.totals.down_main, &down_main)?;
        let down_condition = runtime.execute(children[1], condition, flush)?;
        count_output(&mut self.totals.down_condition, &down_condition)?;
        let latent =
            runtime.execute_conditioned(children[3], &down_main, &down_condition, flush)?;
        count_output(&mut self.totals.latent, &latent)?;
        let upsampled = runtime.execute(children[2], &latent, flush)?;
        count_output(&mut self.totals.upsampled, &upsampled)?;
        if upsampled.frames != 0 {
            self.last_upsampled = Some(upsampled.frame(upsampled.frames - 1)?.to_vec());
        }
        self.upsampled.push(&upsampled)?;

        if flush {
            let low_frames = self.totals.main / RATE;
            if self.totals.down_main != low_frames
                || self.totals.down_condition != low_frames
                || self.totals.latent != low_frames
                || self.totals.upsampled != low_frames * RATE
            {
                return Err(VoiceError::new(
                    "Sampler branches have invalid final frame counts",
                ));
            }
            // Retain the last released upsampled row,
            // including when this final call itself released no new rows.
            let remainder = self.totals.main % RATE;
            if remainder != 0 {
                let last = self.last_upsampled.as_ref().ok_or_else(|| {
                    VoiceError::new("Sampler final extension has no upsampled row")
                })?;
                let mut extension = Sequence::zeros(remainder, CHANNELS)?;
                for row in extension.values.as_chunks_mut::<CHANNELS>().0 {
                    row.copy_from_slice(last);
                }
                self.upsampled.push(&extension)?;
            }
        }

        let frames = self
            .main
            .frames()
            .min(self.condition.frames())
            .min(self.upsampled.frames());
        let upsampled = self.upsampled.take(frames)?;
        let main = self.main.take(frames)?;
        let condition = self.condition.take(frames)?;
        let joined = Sequence::concat_channels(&[&upsampled, &main])?;
        let output = runtime.execute_conditioned(children[4], &joined, &condition, flush)?;
        count_output(&mut self.totals.output, &output)?;
        if flush {
            self.main.ensure_empty("Sampler original input")?;
            self.condition.ensure_empty("Sampler original condition")?;
            self.upsampled.ensure_empty("Sampler upsampled branch")?;
            if self.totals.output != self.totals.main {
                return Err(VoiceError::new(
                    "Sampler final output has an invalid frame count",
                ));
            }
        }
        Ok(output)
    }
}

fn count_output(total: &mut usize, sequence: &Sequence) -> Result<()> {
    sequence.validate()?;
    if sequence.channels != CHANNELS {
        return Err(VoiceError::new(
            "Sampler branch has an invalid channel count",
        ));
    }
    add_frames(total, sequence.frames)
}

fn add_frames(total: &mut usize, frames: usize) -> Result<()> {
    *total = total
        .checked_add(frames)
        .ok_or_else(|| VoiceError::new("Sampler stream frame count overflow"))?;
    Ok(())
}
