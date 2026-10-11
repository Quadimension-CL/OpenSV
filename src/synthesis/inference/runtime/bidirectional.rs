use super::error;
use crate::synthesis::{
    inference::{Sequence, ops::bidirectional::BidirectionalGru, recurrent::GruState},
    voice::{Result, VoiceError},
};

/// No row is stable until the backward recurrence has received the final input.
#[derive(Clone)]
pub(super) struct BidirectionalState {
    input: Vec<f32>,
    states: [GruState; 2],
}

impl BidirectionalState {
    pub(super) fn new(operator: &BidirectionalGru) -> Self {
        Self {
            input: Vec::new(),
            states: operator.new_state(),
        }
    }

    pub(super) fn process(
        &mut self,
        operator: &BidirectionalGru,
        input: &Sequence,
        flush: bool,
    ) -> Result<Sequence> {
        self.input
            .try_reserve(input.values.len())
            .map_err(|error| {
                VoiceError::new(format!("Bidirectional GRU input allocation: {error}"))
            })?;
        self.input.extend_from_slice(&input.values);
        if !flush {
            return Sequence::zeros(0, operator.output_channels());
        }

        let frames = self.input.len() / operator.input_channels();
        let mut output = Sequence::zeros(frames, operator.output_channels())?;
        operator
            .execute_sequence(&self.input, &mut output.values, frames, &mut self.states)
            .map_err(error)?;
        self.input = Vec::new();
        Ok(output)
    }
}
