//! Parallelize independent frames, retaining the reduction order of every dot.

use super::{Affine, MatrixError, baseline_dot};
use crate::synthesis::{
    execution::{self, Cancellation},
    inference::parallel,
};
use rayon::prelude::*;

const MIN_PARALLEL_MACS: usize = 2 * 1024 * 1024;
const MIN_TASK_MACS: usize = 256 * 1024;

impl Affine {
    pub(super) fn execute_frames(
        &self,
        input: &[f32],
        output: &mut [f32],
        frames: usize,
    ) -> Result<(), MatrixError> {
        // GRU/sample loops call one frame at a time. Keep them free of pool
        // dispatch and per-call TLS cloning; their outer loops check cancellation.
        if frames < 8 {
            return self.execute_block(input, output, None);
        }
        let cancellation = execution::current();
        let macs = self.input_channels().saturating_mul(self.output_channels());
        if frames.saturating_mul(macs) < MIN_PARALLEL_MACS {
            return self.execute_block(input, output, cancellation.as_ref());
        }
        let pool = parallel::pool().map_err(MatrixError::Parallel)?;
        let grain = frames
            .div_ceil(pool.current_num_threads() * 2)
            .max(MIN_TASK_MACS.div_ceil(macs))
            .min(frames);
        if pool.current_num_threads() == 1 || grain == frames {
            return self.execute_block(input, output, cancellation.as_ref());
        }
        #[cfg(feature = "synth-profile")]
        let work = crate::synthesis::profiling::parallel_work("kernel.affine.blocks");
        pool.install(|| {
            output
                .par_chunks_mut(grain * self.output_channels())
                .enumerate()
                .try_for_each(|(index, target)| {
                    let first = index * grain * self.input_channels();
                    let count = target.len() / self.output_channels() * self.input_channels();
                    #[cfg(feature = "synth-profile")]
                    return work.measure(|| {
                        self.execute_block(
                            &input[first..first + count],
                            target,
                            cancellation.as_ref(),
                        )
                    });
                    #[cfg(not(feature = "synth-profile"))]
                    self.execute_block(&input[first..first + count], target, cancellation.as_ref())
                })
        })
    }

    fn execute_block(
        &self,
        input: &[f32],
        output: &mut [f32],
        cancellation: Option<&Cancellation>,
    ) -> Result<(), MatrixError> {
        for (input, output) in input
            .chunks_exact(self.input_channels())
            .zip(output.chunks_exact_mut(self.output_channels()))
        {
            if cancellation.is_some_and(Cancellation::is_cancelled) {
                return Err(MatrixError::Cancelled);
            }
            for (channel, target) in output.iter_mut().enumerate() {
                let mut value = baseline_dot(self.matrix.row(channel), input);
                // Bias remains after the dot product, matching the scalar path.
                if let Some(bias) = &self.bias {
                    value += bias[channel];
                }
                *target = value;
            }
        }
        Ok(())
    }
}
