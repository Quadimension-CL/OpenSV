use super::{
    Event,
    state::{WorkState, equivalent_owners},
};
use crate::synthesis::{
    plan::{OwnerId, RenderPlan},
    rendered::{PhonemeSnapshot, RenderedPhonemeTrack, VisualSnapshot},
    runtime::events::Events,
};
use std::{collections::BTreeMap, sync::Arc};

impl WorkState<'_, '_> {
    pub(super) fn publish_phonemes(
        &self,
        events: &Events,
        completed: &BTreeMap<OwnerId, VisualSnapshot>,
    ) {
        let mut snapshots: BTreeMap<_, _> = completed
            .iter()
            .map(|(&owner, snapshot)| {
                (
                    owner,
                    PhonemeSnapshot {
                        input: Arc::clone(&snapshot.input),
                        data: snapshot.data.phonemes.clone(),
                    },
                )
            })
            .collect();
        for (&owner, session) in &self.sessions {
            snapshots.extend(mirrored(&self.plan, owner, session.visuals.phonemes()));
        }
        let _ = events.send(Event::Phonemes {
            revision: self.revision,
            snapshots,
        });
    }
}

pub(super) fn publish(
    events: &Events,
    plan: &RenderPlan,
    revision: u64,
    owner: OwnerId,
    data: &RenderedPhonemeTrack,
) {
    let snapshots = mirrored(plan, owner, data);
    if !snapshots.is_empty() {
        let _ = events.send(Event::Phonemes {
            revision,
            snapshots,
        });
    }
}

fn mirrored(
    plan: &RenderPlan,
    owner: OwnerId,
    data: &RenderedPhonemeTrack,
) -> BTreeMap<OwnerId, PhonemeSnapshot> {
    equivalent_owners(plan, owner)
        .into_iter()
        .map(|mirror| {
            let request = &plan.requests[&mirror];
            (
                mirror,
                PhonemeSnapshot {
                    input: Arc::clone(&request.input),
                    data: data.clone(),
                },
            )
        })
        .collect()
}
