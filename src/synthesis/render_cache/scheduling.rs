use super::{OwnerId, RenderCache, RenderJob, State};
use std::sync::{Arc, atomic::AtomicBool};

impl RenderCache {
    /// Share current ready results after reconciling the complete owner set,
    /// independently of scheduling priority. Pending owners' old baselines are
    /// reserved for their own incremental render, never for other owners.
    pub fn reuse_queued(&mut self) {
        let queued: Vec<_> = self
            .owners
            .iter()
            .filter(|(_, owner)| matches!(owner.state, State::Queued))
            .map(|(&id, owner)| (id, Arc::clone(&owner.request.input)))
            .collect();
        for (id, input) in queued {
            if let Some(rendered) = self.reusable(&input) {
                self.owners
                    .get_mut(&id)
                    .expect("queued owner")
                    .complete(rendered);
                self.advance_publication();
            }
        }
    }

    /// Only independently dispatchable owners: equivalent work already running
    /// remains queued until that owner publishes its complete cache baseline.
    pub fn queued_owners(&self) -> Vec<OwnerId> {
        self.owners
            .iter()
            .filter(|(_, owner)| {
                matches!(owner.state, State::Queued)
                    && !self.owners.values().any(|other| {
                        matches!(other.state, State::Running(_))
                            && *owner.request.input == *other.request.input
                    })
            })
            .map(|(&id, _)| id)
            .collect()
    }

    /// The caller chooses demand priority; the job retains the same commit and
    /// cancellation identity throughout every bounded streaming batch.
    pub fn take_owner(&mut self, id: OwnerId) -> Option<RenderJob> {
        if !self.queued_owners().contains(&id) {
            return None;
        }
        self.next_serial = self
            .next_serial
            .checked_add(1)
            .expect("render serial exhausted");
        let owner = self.owners.get_mut(&id)?;
        let job = RenderJob {
            request: Arc::clone(&owner.request),
            base: owner.committed.clone(),
            generation: self.generation,
            serial: self.next_serial,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        owner.state = State::Running(job.clone());
        Some(job)
    }

    pub fn take_next(&mut self) -> Option<RenderJob> {
        self.reuse_queued();
        let id = self
            .queued_owners()
            .into_iter()
            .min_by_key(|id| (self.owners[id].request.coverage.sample_start.0, *id))?;
        self.take_owner(id)
    }
}
