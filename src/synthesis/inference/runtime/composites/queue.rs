use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

#[derive(Clone, Default)]
pub(super) struct FrameQueue {
    channels: usize,
    values: Vec<f32>,
    consumed: usize,
}

impl FrameQueue {
    pub(super) fn new(channels: usize) -> Self {
        Self {
            channels,
            ..Self::default()
        }
    }

    pub(super) fn frames(&self) -> usize {
        (self.values.len() - self.consumed)
            .checked_div(self.channels)
            .unwrap_or(0)
    }

    pub(super) fn push(&mut self, sequence: &Sequence) -> Result<()> {
        sequence.validate()?;

        if self.channels == 0 {
            self.channels = sequence.channels;
        }
        if self.channels != sequence.channels {
            return Err(VoiceError::new("Composite queue channel width changed"));
        }
        self.values
            .len()
            .checked_add(sequence.values.len())
            .ok_or_else(|| VoiceError::new("Composite queue size overflows"))?;
        if self.consumed > 0 && self.consumed >= self.values.len() / 2 {
            self.values.drain(..self.consumed);
            self.consumed = 0;
        }
        self.values.extend_from_slice(&sequence.values);
        Ok(())
    }

    pub(super) fn take(&mut self, frames: usize) -> Result<Sequence> {
        if frames > self.frames() {
            return Err(VoiceError::new(
                "Composite queue does not have the requested rows",
            ));
        }
        let end = self.consumed + frames * self.channels;
        let sequence = Sequence::new(
            frames,
            self.channels,
            self.values[self.consumed..end].to_vec(),
        )?;
        self.consumed = end;
        if self.consumed == self.values.len() {
            self.values.clear();
            self.consumed = 0;
        }
        Ok(sequence)
    }

    pub(super) fn ensure_empty(&self, name: &str) -> Result<()> {
        if self.frames() != 0 {
            return Err(VoiceError::new(format!(
                "Final {name} has {} unpaired rows",
                self.frames()
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub(super) struct SumQueue {
    left: FrameQueue,
    right: FrameQueue,
}

impl SumQueue {
    pub(super) fn process(
        &mut self,
        left: &Sequence,
        right: &Sequence,
        flush: bool,
        name: &str,
    ) -> Result<Sequence> {
        if left.channels != right.channels {
            return Err(VoiceError::new(format!("{name} branch widths differ")));
        }
        self.left.push(left)?;
        self.right.push(right)?;
        let frames = self.left.frames().min(self.right.frames());
        let mut output = self.left.take(frames)?;
        let right = self.right.take(frames)?;
        for (target, value) in output.values.iter_mut().zip(right.values) {
            *target += value;
        }
        if flush {
            self.left.ensure_empty(name)?;
            self.right.ensure_empty(name)?;
        }
        Ok(output)
    }
}
