//! Cooperative cancellation for one render. Prepared weights are shared, but
//! execution scopes and cancellation state belong to the captured render job.

use super::voice::{Result, VoiceError};
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Debug)]
pub struct Cancellation {
    flag: Arc<AtomicBool>,
    parent: Option<Arc<Cancellation>>,
}

impl Cancellation {
    pub fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self { flag, parent: None }
    }

    /// Cancel an execution cursor without cancelling its owner render. The
    /// parent remains observable by kernels running on the cursor's worker.
    pub(crate) fn child(parent: Option<Self>) -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
            parent: parent.map(Arc::new),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.is_cancelled())
    }

    pub(crate) fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Cancellation>> = const { RefCell::new(None) };
}

/// Parallel kernels must capture this on the calling thread and explicitly
/// pass the token into their work items; thread locals do not follow tasks.
pub fn current() -> Option<Cancellation> {
    CURRENT.with(|current| current.borrow().clone())
}

pub fn checkpoint() -> Result<()> {
    let cancelled = CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .is_some_and(Cancellation::is_cancelled)
    });
    if cancelled {
        Err(VoiceError::new("合成任务已由更新的编辑替代。"))
    } else {
        Ok(())
    }
}

/// Restore the caller's scope on both normal return and unwinding.
pub fn scope<T>(cancellation: Cancellation, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<Cancellation>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|current| *current.borrow_mut() = self.0.take());
        }
    }
    let previous = CURRENT.with(|current| current.replace(Some(cancellation)));
    let _restore = Restore(previous);
    run()
}
