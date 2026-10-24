use super::super::rendered::VisualSnapshot;
use super::{
    CacheStats, Completion, OwnerId, PublishedOwner, RenderCache, RenderJob, RenderedClip, State,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

impl RenderCache {

    pub fn finish(
        &mut self,
        job: &RenderJob,
        result: Result<Arc<RenderedClip>, String>,
    ) -> Completion {
        let current = self.owners.get_mut(&job.request.owner).filter(|owner| {
            !job.is_cancelled()
                && job.generation == self.generation
                && owner.request.epoch == job.request.epoch
                && owner.request.coverage == job.request.coverage
                && *owner.request.input == *job.request.input
                && match (&owner.committed, &job.base) {
                    (Some(current), Some(base)) => Arc::ptr_eq(current, base),
                    (None, None) => true,
                    _ => false,
                }
                && matches!(&owner.state, State::Running(active) if active.serial == job.serial)
        });
        let Some(owner) = current else {
            self.discarded = self.discarded.saturating_add(1);
            return Completion::Discarded;
        };
        match result {
            Ok(rendered) => {
                owner.complete(rendered);
                self.advance_publication();
                Completion::Published
            }
            Err(error) => {
                owner.state = State::Failed(error.clone());
                Completion::Failed(error)
            }
        }
    }

    pub fn retry_failed(&mut self, id: OwnerId) -> bool {
        if let Some(owner) = self.owners.get_mut(&id)
            && matches!(owner.state, State::Failed(_))
        {
            owner.state = State::Queued;
            true
        } else {
            false
        }
    }

    pub fn failure(&self, id: OwnerId) -> Option<&str> {
        match &self.owners.get(&id)?.state {
            State::Failed(error) => Some(error),
            _ => None,
        }
    }

    pub fn snapshot(&self) -> Vec<PublishedOwner> {
        self.owners
            .iter()
            .filter_map(|(&id, owner)| {
                let completed = owner.ready()?;
                (!owner.controls.reference_muted).then(|| PublishedOwner {
                    owner: id,
                    coverage: owner.request.coverage,
                    controls: owner.controls.clone(),
                    audio: Arc::clone(&completed.clip.audio),
                    visuals: Arc::clone(&completed.clip.visuals),
                })
            })
            .collect()
    }

    pub fn visual_snapshot(&self) -> BTreeMap<OwnerId, VisualSnapshot> {
        self.owners
            .iter()
            .filter_map(|(&id, owner)| {
                let completed = owner.ready()?;
                Some((
                    id,
                    VisualSnapshot {
                        data: Arc::clone(&completed.clip.visuals),
                        input: Arc::clone(&completed.input),
                    },
                ))
            })
            .collect()
    }

    pub fn publication_revision(&self) -> u64 {
        self.publication_revision
    }

    pub fn stats(&self) -> CacheStats {
        let mut stats = CacheStats {
            discarded_completions: self.discarded,
            ..CacheStats::default()
        };
        let mut seen = BTreeSet::new();
        for owner in self.owners.values() {
            match &owner.state {
                State::Queued => stats.queued += 1,
                State::Running(_) => stats.running += 1,
                State::Failed(_) => stats.failed += 1,
                State::Ready => stats.ready += 1,
            }
            if let Some(completed) = &owner.committed
                && seen.insert(Arc::as_ptr(&completed.clip))
            {
                stats.current_render_bytes += completed.clip.memory_bytes();
            }
        }
        stats
    }
}
