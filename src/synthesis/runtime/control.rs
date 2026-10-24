use super::worker::Command;
use crate::synthesis::{
    plan::{OwnerId, OwnerRequest, RenderPlan},
    render_cache::RenderJob,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, mpsc},
};

#[derive(Default)]
pub(super) struct Control(Mutex<State>);

#[derive(Default)]
struct State {
    generation: u64,
    requests: BTreeMap<OwnerId, Arc<OwnerRequest>>,
    active: Option<RenderJob>,
}

impl State {
    fn matches(&self, job: &RenderJob) -> bool {
        self.requests
            .get(&job.request.owner)
            .is_some_and(|request| {
                request.epoch == job.request.epoch
                    && request.coverage == job.request.coverage
                    && *request.input == *job.request.input
            })
    }

    fn update(&mut self, plan: &RenderPlan) {
        self.requests = plan
            .requests
            .iter()
            .filter(|(owner, _)| plan.output_controls.contains_key(owner))
            .map(|(&owner, request)| (owner, Arc::clone(request)))
            .collect();
        if let Some(job) = &self.active
            && !self.matches(job)
        {
            job.cancellation().cancel();
        }
    }

    fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.requests.clear();
        if let Some(job) = &self.active {
            job.cancellation().cancel();
        }
    }
}

impl Control {
    pub fn send(&self, sender: &mpsc::Sender<Command>, command: Command) -> bool {
        let mut state = self.0.lock().expect("synthesis submission lock");
        match &command {
            Command::Plan { plan, .. } => state.update(plan),
            Command::Reset => state.reset(),
            Command::Retry | Command::Demand { .. } => {}
        }

        sender.send(command).is_ok()
    }

    pub fn register(&self, job: &RenderJob, generation: u64) -> Active<'_> {
        let mut state = self.0.lock().expect("synthesis submission lock");
        if state.generation != generation || !state.matches(job) {
            job.cancellation().cancel();
        }
        state.active = Some(job.clone());
        Active(self)
    }
}

pub(super) struct Active<'a>(&'a Control);

impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.0.lock().expect("synthesis submission lock").active = None;
    }
}
