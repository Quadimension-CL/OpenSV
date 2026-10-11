//! Conditional convolution gate.
//!
//! Each projected frame contains two contiguous channel blocks. The gate
//! adds the optional projected condition, splits the frame, applies tanh to the
//! first half and sigmoid to the second, then multiplies the halves. Temporal
//! convolution and its condition queue belong to the caller.

mod parallel;

use crate::synthesis::{
    execution::{self, Cancellation},
    inference::{
        activation::ActivationOperator,
        matrix::{MatrixError, check_length},
    },
};

pub const GATED_TYPE_ID: [u8; 16] = [
    0x43, 0x18, 0xe8, 0x30, 0xef, 0xa6, 0x06, 0xf8, 0x10, 0xb5, 0x93, 0xd3, 0xa7, 0x6e, 0x9f, 0xc4,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatedConfig {
    pub input_channels: usize,
    pub output_channels: usize,
    pub condition_channels: usize,
}

impl GatedConfig {
    pub fn parse(payload: &[u8]) -> Result<Self, MatrixError> {
        check_length("gated configuration", 12, payload.len())?;
        let mut words = [0; 3];
        for (word, bytes) in words.iter_mut().zip(payload.as_chunks::<4>().0.iter()) {
            *word = u32::from_le_bytes(*bytes) as usize;
        }
        if words[0] == 0 || words[1] == 0 || words[1].checked_mul(2).is_none() {
            return Err(MatrixError::InvalidShape);
        }
        Ok(Self {
            input_channels: words[0],
            output_channels: words[1],
            condition_channels: words[2],
        })
    }
}

/// Consumes aligned, frame-major outputs from the main and optional condition
/// convolutions. Each input frame has `2 * channels` values; each output frame
/// has `channels`. The condition has already been projected, so its size is
/// independent of the condition input width in `GatedConfig`.
///
/// The optional condition is added before both nonlinearities. Input buffers
/// are preserved and every shape is checked before writing output. The tanh
/// calculation uses platform libm, so rounding can differ across platforms.
pub fn activate_projected(
    projected: &[f32],
    condition: Option<&[f32]>,
    output: &mut [f32],
    channels: usize,
) -> Result<(), MatrixError> {
    let projected_channels = channels
        .checked_mul(2)
        .filter(|&width| width != 0)
        .ok_or(MatrixError::InvalidShape)?;
    if !projected.len().is_multiple_of(projected_channels) {
        return Err(MatrixError::InvalidShape);
    }
    check_length("gated output", projected.len() / 2, output.len())?;
    if let Some(condition) = condition {
        check_length(
            "gated projected condition",
            projected.len(),
            condition.len(),
        )?;
    }

    // Frame boundaries also preserve the activation's SSE/scalar-tail split.
    // Capture cancellation before dispatch: worker thread locals do not carry
    // the active render's execution scope.
    let cancellation = execution::current();
    parallel::for_frames(output, channels, |first, target| {
        let start = first * projected_channels;
        let end = start + target.len() * 2;
        activate_frames(
            &projected[start..end],
            condition.map(|values| &values[start..end]),
            target,
            channels,
            cancellation.as_ref(),
        )
    })
}

fn activate_frames(
    projected: &[f32],
    condition: Option<&[f32]>,
    output: &mut [f32],
    channels: usize,
    cancellation: Option<&Cancellation>,
) -> Result<(), MatrixError> {
    let projected_channels = channels * 2;
    // Each task reuses one condition workspace for its complete frames. The
    // sigmoid is written straight into output, then multiplied in place, so an
    // unconditional gate needs no temporary allocation.
    let mut combined = if condition.is_some() {
        vec![0.0; projected_channels]
    } else {
        Vec::new()
    };
    for (frame, (projected, output)) in projected
        .chunks_exact(projected_channels)
        .zip(output.chunks_exact_mut(channels))
        .enumerate()
    {
        if frame.is_multiple_of(32) {
            check_cancelled(cancellation)?;
        }
        let projected = if let Some(condition) = condition {
            let start = frame * projected_channels;
            for ((target, &input), &condition) in combined
                .iter_mut()
                .zip(projected)
                .zip(&condition[start..start + projected_channels])
            {
                *target = input + condition;
            }
            &combined
        } else {
            projected
        };
        let (signal, gate_input) = projected.split_at(channels);
        #[cfg(target_arch = "x86_64")]
        let result = ActivationOperator::Sigmoid.execute_baseline(gate_input, output);
        #[cfg(not(target_arch = "x86_64"))]
        let result = ActivationOperator::Sigmoid.execute_reference(gate_input, output);
        // Both slices are exactly `channels` elements after the shape checks.
        debug_assert!(result.is_ok());
        for (target, &signal) in output.iter_mut().zip(signal) {
            let gate = *target;
            *target = signal.tanh() * gate;
        }
    }
    check_cancelled(cancellation)
}

fn check_cancelled(cancellation: Option<&Cancellation>) -> Result<(), MatrixError> {
    if cancellation.is_some_and(Cancellation::is_cancelled) {
        Err(MatrixError::Cancelled)
    } else {
        Ok(())
    }
}
