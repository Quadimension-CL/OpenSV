use rayon::{ThreadPool, ThreadPoolBuilder};
use std::sync::OnceLock;

const MAX_THREADS: usize = 8;

static POOL: OnceLock<Result<ThreadPool, String>> = OnceLock::new();

pub(super) fn pool() -> Result<&'static ThreadPool, String> {
    POOL.get_or_init(|| {
        let threads = std::thread::available_parallelism()
            .map_err(|error| format!("Cannot determine synthesis concurrency: {error}"))?
            .get()
            .min(MAX_THREADS);
        ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|index| format!("synthesis-compute-{index}"))
            .build()
            .map_err(|error| format!("Cannot prepare synthesis compute pool: {error}"))
    })
    .as_ref()
    .map_err(Clone::clone)
}
