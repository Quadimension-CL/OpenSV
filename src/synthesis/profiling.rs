//! Opt-in, thread-local wall-time attribution for native synthesis.
//!
//! Compiled only with `synth-profile`. Each span's self time excludes the
//! complete durations of its direct children, so nested operators are not
//! counted twice when summing self times. Timers do not measure CPU cycles.
//! Parallel worker measurements are reported separately: their summed wall
//! time may exceed the session's elapsed time and must not be added to it.

mod parallel;

pub use parallel::{ParallelTiming, ParallelWork};

use std::{
    cell::RefCell,
    collections::HashMap,
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};

use serde::Serialize;

thread_local! {
    static SESSION: RefCell<Session> = RefCell::new(Session::default());
}

#[derive(Default)]
struct Session {
    generation: u64,
    started: Option<Instant>,
    next_token: u64,
    nodes: Vec<Node>,
    lookup: HashMap<(Option<usize>, &'static str), usize>,
    stack: Vec<ActiveSpan>,
    parallel: parallel::Registry,
}

struct Node {
    parent: Option<usize>,
    name: &'static str,
    calls: u64,
    inclusive: Duration,
    own: Duration,
}

struct ActiveSpan {
    node: usize,
    token: u64,
    started: Instant,
    children: Duration,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub elapsed_ms: f64,
    /// Nonzero means `finish` cut short scopes which had not yet been dropped.
    pub open_spans_at_finish: usize,
    /// Full call paths, sorted by descending self time.
    pub entries: Vec<SpanTiming>,
    /// Worker task wall times, independent of main-thread span attribution.
    pub parallel_entries: Vec<ParallelTiming>,
}

#[derive(Debug, Serialize)]
pub struct SpanTiming {
    pub path: String,
    pub calls: u64,
    pub inclusive_ms: f64,
    pub self_ms: f64,
}

/// Begin a fresh session on the calling thread. Other threads are unaffected.
pub fn start() {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        let generation = session.generation.wrapping_add(1);
        *session = Session {
            generation,
            started: Some(Instant::now()),
            ..Session::default()
        };
    });
}

/// Stop this thread's session and return its aggregated measurements.
pub fn finish() -> Report {
    let now = Instant::now();
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        let elapsed = session.started.take().map_or(Duration::ZERO, |t| now - t);
        let open_spans_at_finish = session.stack.len();
        while !session.stack.is_empty() {
            session.close_top(now);
        }
        let mut entries: Vec<_> = session
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| SpanTiming {
                path: session.path(index),
                calls: node.calls,
                inclusive_ms: milliseconds(node.inclusive),
                self_ms: milliseconds(node.own),
            })
            .collect();
        entries.sort_unstable_by(|left, right| {
            right
                .self_ms
                .total_cmp(&left.self_ms)
                .then_with(|| left.path.cmp(&right.path))
        });
        Report {
            elapsed_ms: milliseconds(elapsed),
            open_spans_at_finish,
            entries,
            parallel_entries: session.parallel.finish(),
        }
    })
}

/// Scope guard cannot move to another thread because sessions are thread-local.
#[must_use = "keep the guard alive for the measured scope"]
pub struct Span {
    identity: Option<(u64, u64)>,
    thread: PhantomData<Rc<()>>,
}

/// Open a scope. Calls outside an active session perform no clock reads.
pub fn span(name: &'static str) -> Span {
    let identity = SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.started?;
        let parent = session.stack.last().map(|active| active.node);
        let key = (parent, name);
        let node = if let Some(&index) = session.lookup.get(&key) {
            index
        } else {
            let index = session.nodes.len();
            session.nodes.push(Node {
                parent,
                name,
                calls: 0,
                inclusive: Duration::ZERO,
                own: Duration::ZERO,
            });
            session.lookup.insert(key, index);
            index
        };
        let token = session.next_token;
        session.next_token = session.next_token.wrapping_add(1);
        session.stack.push(ActiveSpan {
            node,
            token,
            started: Instant::now(),
            children: Duration::ZERO,
        });
        Some((session.generation, token))
    });
    Span {
        identity,
        thread: PhantomData,
    }
}

/// Capture the calling thread's current path for measurements on worker
/// threads. No clock or cross-thread synchronization is used if this thread
/// has no active session. The handle does not create a main-thread span.
pub fn parallel_work(name: &'static str) -> ParallelWork {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        if session.started.is_none() {
            return ParallelWork::default();
        }
        let path = match session.stack.last() {
            Some(active) => format!("{}/{name}", session.path(active.node)),
            None => name.to_owned(),
        };
        session.parallel.register(path)
    })
}

impl Drop for Span {
    fn drop(&mut self) {
        let Some((generation, token)) = self.identity else {
            return;
        };
        let now = Instant::now();
        SESSION.with(|session| {
            let mut session = session.borrow_mut();
            if session.started.is_none() || session.generation != generation {
                return;
            }
            if let Some(index) = session
                .stack
                .iter()
                .rposition(|active| active.token == token)
            {
                // Normal lexical RAII closes exactly one scope. An explicitly
                // dropped parent also ends its children at that same boundary.
                while session.stack.len() > index {
                    session.close_top(now);
                }
            }
        });
    }
}

impl Session {
    fn close_top(&mut self, now: Instant) {
        let active = self.stack.pop().expect("active profiling scope");
        let elapsed = now - active.started;
        let node = &mut self.nodes[active.node];
        node.calls += 1;
        node.inclusive += elapsed;
        node.own += elapsed.saturating_sub(active.children);
        if let Some(parent) = self.stack.last_mut() {
            parent.children += elapsed;
        }
    }

    fn path(&self, index: usize) -> String {
        let node = &self.nodes[index];
        match node.parent {
            Some(parent) => format!("{}/{}", self.path(parent), node.name),
            None => node.name.to_owned(),
        }
    }
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
