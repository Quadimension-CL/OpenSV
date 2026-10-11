use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

/// Same-rate branch rows waiting for the corresponding temporal position.
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

    pub fn push(&mut self, input: &Sequence) -> Result<()> {
        input.validate()?;
        if input.channels != self.channels {
            return Err(VoiceError::new("Compatible acoustic branch width changed"));
        }
        if self.head > 0 && self.head * 2 >= self.values.len() {
            self.values.drain(..self.head);
            self.head = 0;
        }
        self.values.extend_from_slice(&input.values);
        Ok(())
    }

    pub fn take(&mut self, count: usize) -> Result<Sequence> {
        if count > self.len() {
            return Err(VoiceError::new(
                "Compatible acoustic branch has insufficient rows",
            ));
        }
        let end = self.head + count * self.channels;
        let output = Sequence::new(count, self.channels, self.values[self.head..end].to_vec())?;
        self.head = end;
        Ok(output)
    }
}

pub(super) fn align(
    left: &mut Rows,
    right: &mut Rows,
    flush: bool,
) -> Result<(Sequence, Sequence)> {
    if flush && left.len() != right.len() {
        return Err(VoiceError::new(format!(
            "Compatible acoustic branches ended at different rows: {} / {}",
            left.len(),
            right.len()
        )));
    }
    let count = left.len().min(right.len());
    Ok((left.take(count)?, right.take(count)?))
}
