use super::state::{WorkState, equivalent_owners};
use crate::synthesis::plan::OwnerId;

impl WorkState<'_, '_> {
    /// A playhead block must be ready in every overlapping audible owner.
    /// Once that block and a short lookahead are covered, use spare time to
    /// finish missing intervals and eventually produce the export snapshot.
    pub fn next_work(&self) -> Option<(OwnerId, f64)> {
        let mut owners = self.cache.queued_owners();
        owners.extend(self.sessions.keys().copied());
        let lookahead = if self.playing { 4 } else { 1 };
        for offset in 0..lookahead {
            let seconds = self.demand_seconds + f64::from(offset) * 0.5;
            for &owner in &owners {
                if let Some(target) = self.live.as_ref().and_then(|live| {
                    equivalent_owners(&self.plan, owner)
                        .into_iter()
                        .find_map(|mirror| {
                            live.needs(mirror, seconds)
                                .then(|| live.next_needed(mirror, seconds))
                                .flatten()
                        })
                }) {
                    return Some((owner, target));
                }
            }
        }
        owners
            .into_iter()
            .min_by_key(|owner| {
                let start = self
                    .plan
                    .requests
                    .get(owner)
                    .map_or(i64::MAX, |request| request.coverage.sample_start.0);
                // Keep a running cursor warm during background completion.
                (!self.sessions.contains_key(owner), start, *owner)
            })
            .map(|owner| {
                let seconds = self
                    .live
                    .as_ref()
                    .and_then(|live| {
                        equivalent_owners(&self.plan, owner)
                            .into_iter()
                            .find_map(|mirror| live.next_needed(mirror, self.demand_seconds))
                    })
                    .unwrap_or(self.demand_seconds);
                (owner, seconds)
            })
    }
}
