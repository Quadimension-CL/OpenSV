//! One bounded, persistent operator pool. Tasks own disjoint complete frames;
//! splitting never changes the reduction order within an output sample.

use rayon::prelude::*;

use crate::synthesis::inference::{convolution::ConvolutionError, parallel::pool};

const MIN_PARALLEL_MACS: usize = 2 * 1024 * 1024;
const MIN_TASK_MACS: usize = 256 * 1024;

pub(super) fn for_frames(
    output: &mut [f32],
    channels: usize,
    macs_per_frame: usize,
    execute: impl Fn(usize, &mut [f32]) -> Result<(), ConvolutionError> + Send + Sync,
) -> Result<(), ConvolutionError> {
    let frames = output.len() / channels;
    if frames < 8 || frames.saturating_mul(macs_per_frame) < MIN_PARALLEL_MACS {
        return execute(0, output);
    }
    let pool = pool().map_err(ConvolutionError::Parallel)?;
    let grain = frames
        .div_ceil(pool.current_num_threads() * 2)
        .max(MIN_TASK_MACS.div_ceil(macs_per_frame))
        .div_ceil(4)
        .saturating_mul(4)
        .min(frames);
    if pool.current_num_threads() == 1 || grain == frames {
        return execute(0, output);
    }
    // The caller's kernel span measures scheduling/waiting wall latency. Task
    // wall time is reported separately, never added to that inclusive time.
    #[cfg(feature = "synth-profile")]
    let work = crate::synthesis::profiling::parallel_work("kernel.convolution.blocks");
    pool.install(|| {
        output
            .par_chunks_mut(grain * channels)
            .enumerate()
            .try_for_each(|(index, target)| {
                #[cfg(feature = "synth-profile")]
                return work.measure(|| execute(index * grain, target));
                #[cfg(not(feature = "synth-profile"))]
                execute(index * grain, target)
            })
    })
}
