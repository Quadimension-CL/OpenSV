use rayon::prelude::*;

use super::MatrixError;
use crate::synthesis::inference::parallel::pool;

const MIN_PARALLEL_VALUES: usize = 16 * 1024;
const MIN_TASK_VALUES: usize = 2 * 1024;

pub(super) fn for_frames(
    output: &mut [f32],
    channels: usize,
    execute: impl Fn(usize, &mut [f32]) -> Result<(), MatrixError> + Send + Sync,
) -> Result<(), MatrixError> {
    let frames = output.len() / channels;
    if frames < 8 || output.len() < MIN_PARALLEL_VALUES {
        return execute(0, output);
    }
    let pool = pool().map_err(MatrixError::Parallel)?;
    let grain = frames
        .div_ceil(pool.current_num_threads() * 2)
        .max(MIN_TASK_VALUES.div_ceil(channels))
        .div_ceil(4)
        .saturating_mul(4)
        .min(frames);
    if pool.current_num_threads() == 1 || grain == frames {
        return execute(0, output);
    }
    #[cfg(feature = "synth-profile")]
    let work = crate::synthesis::profiling::parallel_work("kernel.gated.blocks");
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
