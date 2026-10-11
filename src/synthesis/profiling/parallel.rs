//! Cross-thread task timings associated with a session's captured call path.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ParallelTiming {
    pub path: String,
    /// Completed task scopes, rather than calls dispatching a parallel batch.
    pub calls: u64,
    /// Sum of each task's wall duration. This is not CPU time; workers can be
    /// descheduled, and concurrent tasks overlap with main-thread span time.
    pub worker_task_ms: f64,
    /// Tasks still running when the session ended; excluded from the sum.
    pub open_tasks_at_finish: usize,
}

/// Share this captured path across worker tasks without creating a session on
/// those threads. An inactive handle calls the closure without a clock read,
/// allocation, or synchronization.
#[derive(Clone, Default)]
pub struct ParallelWork {
    counter: Option<Arc<Counter>>,
}

impl ParallelWork {
    pub fn measure<T>(&self, task: impl FnOnce() -> T) -> T {
        let _timer = self.counter.as_ref().and_then(|counter| counter.begin());
        task()
    }
}

#[derive(Default)]
pub(super) struct Registry {
    counters: HashMap<String, Arc<Counter>>,
}

impl Registry {
    pub(super) fn register(&mut self, path: String) -> ParallelWork {
        let counter = self
            .counters
            .entry(path)
            .or_insert_with(|| Arc::new(Counter::default()));
        ParallelWork {
            counter: Some(Arc::clone(counter)),
        }
    }

    pub(super) fn finish(&mut self) -> Vec<ParallelTiming> {
        let mut entries: Vec<_> = self
            .counters
            .iter()
            .map(|(path, counter)| {
                let mut state = counter.lock();
                state.active = false;
                ParallelTiming {
                    path: path.clone(),
                    calls: state.calls,
                    worker_task_ms: super::milliseconds(state.worker_wall),
                    open_tasks_at_finish: state.running,
                }
            })
            .collect();
        entries.sort_unstable_by(|left, right| {
            right
                .worker_task_ms
                .total_cmp(&left.worker_task_ms)
                .then_with(|| left.path.cmp(&right.path))
        });
        entries
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        // Starting another session also ends any handles retained from the
        // previous generation, even if that session never called finish.
        for counter in self.counters.values() {
            counter.lock().active = false;
        }
    }
}

#[derive(Default)]
struct Counter {
    state: Mutex<State>,
}

struct State {
    active: bool,
    calls: u64,
    running: usize,
    worker_wall: Duration,
}

impl Default for State {
    fn default() -> Self {
        Self {
            active: true,
            calls: 0,
            running: 0,
            worker_wall: Duration::ZERO,
        }
    }
}

impl Counter {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn begin(&self) -> Option<Task<'_>> {
        {
            let mut state = self.lock();
            if !state.active {
                return None;
            }
            state.running += 1;
        }
        Some(Task {
            counter: self,
            started: Instant::now(),
        })
    }
}

struct Task<'a> {
    counter: &'a Counter,
    started: Instant,
}

impl Drop for Task<'_> {
    fn drop(&mut self) {
        // Exclude the bookkeeping lock from the task's measured duration.
        let elapsed = self.started.elapsed();
        let mut state = self.counter.lock();
        state.running -= 1;
        if state.active {
            state.calls += 1;
            state.worker_wall += elapsed;
        }
    }
}
