//! Complete-sequence bidirectional GRU.
//!
//! Child 0 consumes rows in order; child 1 consumes the same rows in reverse.
//! Their outputs are concatenated at each input row, without reversing
//! channels. Immutable GRU clones share weights with their source children.

use crate::synthesis::inference::{
    matrix::{MatrixError, check_length},
    recurrent::{Gru, GruState, RecurrentError},
};

pub const BIDIRECTIONAL_GRU_TYPE_ID: [u8; 16] = [
    0xef, 0x24, 0xbd, 0x14, 0x67, 0xa0, 0x69, 0x5f, 0x89, 0x55, 0xfa, 0xdd, 0xb2, 0x90, 0x50, 0xb8,
];

#[derive(Clone)]
pub struct BidirectionalGru {
    forward: Gru,
    backward: Gru,
    output_channels: usize,
}

impl BidirectionalGru {
    pub fn new(forward: Gru, backward: Gru) -> Result<Self, MatrixError> {
        if forward.input_channels() != backward.input_channels() {
            return Err(MatrixError::InvalidShape);
        }
        let output_channels = forward
            .output_channels()
            .checked_add(backward.output_channels())
            .ok_or(MatrixError::InvalidShape)?;
        Ok(Self {
            forward,
            backward,
            output_channels,
        })
    }

    pub fn input_channels(&self) -> usize {
        self.forward.input_channels()
    }

    pub fn output_channels(&self) -> usize {
        self.output_channels
    }

    pub fn new_state(&self) -> [GruState; 2] {
        [self.forward.new_state(), self.backward.new_state()]
    }

    /// Execute a full sequence; callers must not publish a prefix before flush.
    pub fn execute_sequence(
        &self,
        input: &[f32],
        output: &mut [f32],
        frames: usize,
        states: &mut [GruState; 2],
    ) -> Result<(), RecurrentError> {
        let inputs = frames
            .checked_mul(self.input_channels())
            .ok_or(MatrixError::InvalidShape)?;
        let outputs = frames
            .checked_mul(self.output_channels())
            .ok_or(MatrixError::InvalidShape)?;
        check_length("Bidirectional GRU input", inputs, input.len())?;
        check_length("Bidirectional GRU output", outputs, output.len())?;
        for (operator, state) in [&self.forward, &self.backward]
            .into_iter()
            .zip(states.iter())
        {
            check_length(
                "Bidirectional GRU hidden state",
                operator.output_channels(),
                state.hidden().len(),
            )?;
        }
        let channels = self.input_channels();
        let split = self.forward.output_channels();
        let cancellation = crate::synthesis::execution::current();
        for frame in 0..frames {
            if frame.is_multiple_of(32)
                && cancellation
                    .as_ref()
                    .is_some_and(|token| token.is_cancelled())
            {
                return Err(MatrixError::Cancelled.into());
            }
            self.forward.step(
                &input[frame * channels..(frame + 1) * channels],
                &mut states[0],
            )?;
            let start = frame * self.output_channels;
            output[start..start + split].copy_from_slice(states[0].hidden());

            let reverse = frames - frame - 1;
            self.backward.step(
                &input[reverse * channels..(reverse + 1) * channels],
                &mut states[1],
            )?;
            let start = reverse * self.output_channels;
            output[start + split..start + self.output_channels].copy_from_slice(states[1].hidden());
        }
        Ok(())
    }
}
