mod results;
mod scheduling;
mod types;

pub use types::*;

use super::{
    plan::{OwnerId, OwnerInput, OwnerOutputControls, OwnerRequest, RenderPlan},
    rendered::RenderedClip,
};
use std::{collections::BTreeMap, sync::Arc};

enum State {
    Queued,
    Running(RenderJob),
    Ready,
    Failed(String),
}

struct Owner {
    request: Arc<OwnerRequest>,
    controls: OwnerOutputControls,
    state: State,
    committed: Option<Arc<CompletedOwner>>,
}

impl Owner {
    fn cancel(&self) {
        if let State::Running(job) = &self.state {
            job.cancel();
        }
    }

    fn audible(&self) -> bool {
        !self.controls.reference_muted && matches!(self.state, State::Ready)
    }

    fn complete(&mut self, clip: Arc<RenderedClip>) {
        self.committed = Some(Arc::new(CompletedOwner {
            input: Arc::clone(&self.request.input),
            clip,
        }));
        self.state = State::Ready;
    }

    fn ready(&self) -> Option<&CompletedOwner> {
        if matches!(self.state, State::Ready) {
            self.committed.as_deref()
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct RenderCache {
    owners: BTreeMap<OwnerId, Owner>,
    generation: u64,
    next_serial: u64,
    publication_revision: u64,
    discarded: u64,
}

impl RenderCache {

    pub fn reconcile(&mut self, plan: &RenderPlan) -> CacheUpdate {
        let mut update = CacheUpdate::default();
        let mut previous = std::mem::take(&mut self.owners);
        for (&id, request) in &plan.requests {
            let Some(controls) = plan.output_controls.get(&id) else {

                continue;
            };
            let old = previous.remove(&id);
            let owner = match old {
                Some(mut old)
                    if old.request.epoch == request.epoch
                        && old.request.coverage == request.coverage
                        && *old.request.input == *request.input =>
                {
                    if old.controls != *controls {
                        if old.audible() && controls.reference_muted {
                            update.withdrawn.insert(id, old.request.coverage);
                        }
                        update.publication_changed |= matches!(old.state, State::Ready);
                        old.controls = controls.clone();
                    }
                    old.request = Arc::clone(request);
                    old
                }
                old => {
                    let reused = old
                        .as_ref()
                        .and_then(|old| committed_matching(old, &request.input));
                    let committed = old.as_ref().and_then(|old| old.committed.clone());
                    if let Some(old) = old {
                        old.cancel();
                        update.withdrawn.insert(id, old.request.coverage);
                        update.publication_changed |= matches!(old.state, State::Ready);
                    }
                    let mut owner = Owner {
                        request: Arc::clone(request),
                        controls: controls.clone(),
                        state: State::Queued,

                        committed,
                    };
                    if let Some(rendered) = reused {
                        update.publication_changed = true;
                        owner.complete(rendered);
                    }
                    owner
                }
            };
            self.owners.insert(id, owner);
        }
        for (id, old) in previous {
            old.cancel();
            update.withdrawn.insert(id, old.request.coverage);
            update.publication_changed |= matches!(old.state, State::Ready);
        }
        if update.publication_changed {
            self.advance_publication();
        }
        update
    }

    pub fn reset(&mut self) {
        for owner in self.owners.values() {
            owner.cancel();
        }
        self.owners.clear();
        self.generation = self
            .generation
            .checked_add(1)
            .expect("render generation exhausted");
        self.advance_publication();
    }

    fn advance_publication(&mut self) {
        self.publication_revision = self
            .publication_revision
            .checked_add(1)
            .expect("render publication revision exhausted");
    }

    fn reusable(&self, input: &OwnerInput) -> Option<Arc<RenderedClip>> {
        self.owners.values().find_map(|owner| {
            let completed = owner.ready()?;
            (*owner.request.input == *input && *completed.input == *input)
                .then(|| Arc::clone(&completed.clip))
        })
    }
}

impl Drop for RenderCache {
    fn drop(&mut self) {
        for owner in self.owners.values() {
            owner.cancel();
        }
    }
}

fn committed_matching(owner: &Owner, input: &OwnerInput) -> Option<Arc<RenderedClip>> {
    let committed = owner.committed.as_ref()?;
    (*committed.input == *input).then(|| Arc::clone(&committed.clip))
}
