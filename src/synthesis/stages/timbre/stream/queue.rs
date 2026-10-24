use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

pub(super) struct Rows {
    channels: usize,
    values: Vec<f32>,
    head: usize,
}

impl Rows {
    pub fn new(channels: usize) -> Self {
        Self {
            channels,
            values: Vec::new(),
            head: 0,
        }
    }

    pub fn len(&self) -> usize {
        (self.values.len() - self.head) / self.channels
    }

    pub fn push(&mut self, sequence: &Sequence) -> Result<()> {
        sequence.validate()?;
        if sequence.channels != self.channels {
            return Err(VoiceError::new("Timbre branch queue channel count changed"));
        }
        if self.head > 0 && self.head * 2 >= self.values.len() {
            self.values.drain(..self.head);
            self.head = 0;
        }
        self.values.extend_from_slice(&sequence.values);
        Ok(())
    }

    pub fn take(&mut self, frames: usize) -> Result<Sequence> {
        if frames > self.len() {
            return Err(VoiceError::new("Timbre branch queue has insufficient rows"));
        }
        let end = self.head + frames * self.channels;
        let output = Sequence::new(frames, self.channels, self.values[self.head..end].to_vec())?;
        self.head = end;
        Ok(output)
    }
}

pub(super) fn align(
    left: &mut Rows,
    right: &mut Rows,
    final_batch: bool,
) -> Result<(Sequence, Sequence)> {
    if final_batch && left.len() != right.len() {
        return Err(VoiceError::new(format!(
            "Timbre branches ended with different row counts: {} / {}",
            left.len(),
            right.len()
        )));
    }
    let count = left.len().min(right.len());
    Ok((left.take(count)?, right.take(count)?))
}

#[derive(Default)]
pub(super) struct TailHold {
    emitted: usize,
    last: Vec<f32>,
}

impl TailHold {
    pub fn extend(
        &mut self,
        mut rows: Sequence,
        target: usize,
        final_batch: bool,
        trim: bool,
    ) -> Result<Sequence> {
        if rows.frames > 0 {
            self.last = rows.frame(rows.frames - 1)?.to_vec();
        }
        if self.emitted + rows.frames > target {
            if !trim {
                return Err(VoiceError::new(
                    "Timbre upsampler exceeded its source timeline",
                ));
            }
            rows.frames = target.saturating_sub(self.emitted);
            rows.values.truncate(rows.frames * rows.channels);
        }
        if final_batch && self.emitted + rows.frames < target {
            if self.last.is_empty() {
                return Err(VoiceError::new(
                    "Timbre upsampler has no final row to extend",
                ));
            }
            let extra = target - self.emitted - rows.frames;
            for _ in 0..extra {
                rows.values.extend_from_slice(&self.last);
            }
            rows.frames += extra;
        }
        self.emitted += rows.frames;
        Ok(rows)
    }
}
