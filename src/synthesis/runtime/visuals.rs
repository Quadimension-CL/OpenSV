use super::Runtime;
use crate::synthesis::{
    plan::OwnerId,
    rendered::{PhonemeSnapshot, VisualPatch, VisualSnapshot},
};
use std::{collections::BTreeMap, sync::Arc, time::Instant};

impl Runtime {
    pub(super) fn sync_visuals(&mut self) {
        let plan = self.planner.plan();
        self.visuals
            .retain(|owner, _| plan.requests.contains_key(owner));
        for (&owner, request) in &plan.requests {
            // Preserve each old piece's geometry until this revision supplies
            // replacement data for its interval. Empty/deleted owners retire.
            self.visuals
                .entry(owner)
                .or_default()
                .begin(Arc::clone(&request.input));
        }
    }

    pub(super) fn patch_visuals(&mut self, patches: Vec<(OwnerId, VisualPatch)>) {
        let now = Instant::now();
        for (owner, patch) in patches {
            if self
                .planner
                .plan()
                .requests
                .get(&owner)
                .is_some_and(|request| *request.input == *patch.input)
            {
                self.visuals.entry(owner).or_default().apply(patch, now);
            }
        }
    }

    pub(super) fn complete_visuals(&mut self, snapshots: BTreeMap<OwnerId, VisualSnapshot>) {
        for (owner, snapshot) in snapshots {
            if self
                .planner
                .plan()
                .requests
                .get(&owner)
                .is_some_and(|request| *request.input == *snapshot.input)
            {
                // Consolidate block views without restarting or erasing the
                // still-fading outlines of blocks that just became ready.
                self.phonemes.insert(
                    owner,
                    PhonemeSnapshot {
                        input: Arc::clone(&snapshot.input),
                        data: snapshot.data.phonemes.clone(),
                    },
                );
                self.visuals.entry(owner).or_default().complete(snapshot);
            }
        }
    }
}
