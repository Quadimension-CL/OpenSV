use super::state::{WorkState, equivalent_owners};
use crate::synthesis::plan::OwnerId;

impl WorkState<'_, '_> {

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

        owners.into_iter().map(|owner| {
                let audible = self
                    .live
                    .as_ref()
                    .and_then(|live| {
                        equivalent_owners(&self.plan, owner)
                            .into_iter()
                            .find_map(|mirror| live.next_needed(mirror, self.demand_seconds))
                    });
                let seconds=audible.unwrap_or_else(||self.plan.requests.get(&owner)
                    .map_or(self.demand_seconds,|r|r.coverage.sample_start.0 as f64/f64::from(r.coverage.sample_rate)));

                (owner, seconds, audible.is_none())
            })
            .min_by(|(a, ta, silent_a), (b, tb, silent_b)| {
                silent_a.cmp(silent_b)
                    .then_with(||(ta < &self.demand_seconds).cmp(&(tb < &self.demand_seconds)))
                    .then_with(|| ta.total_cmp(tb))
                    .then_with(|| (!self.sessions.contains_key(a)).cmp(&!self.sessions.contains_key(b)))
                    .then_with(|| a.cmp(b))
            })
            .map(|(owner,seconds,_)|(owner,seconds))
    }
}

#[cfg(feature="acoustic-diagnostic")]
pub fn diagnostic(project:&crate::model::Project,seconds:f64)->serde_json::Value {
    use super::Command;
    use crate::synthesis::{plan::Planner,runtime::mixer::LiveMixer};
    let mut planner=Planner::default();
    let plan=planner.update(project,0,|_|Some("queue-observation".into())).clone();
    let mut state=WorkState::new();
    state.apply(Command::Plan{plan:plan.clone(),revision:0,solo_tracks:Default::default()});
    state.live=Some(LiveMixer::new(&plan,&Default::default()).expect("valid project mixer"));
    state.demand_seconds=seconds;state.playing=true;
    let rows=state.cache.queued_owners().into_iter().map(|owner| {
        serde_json::json!({"owner":owner,"coverage_start":plan.requests[&owner].coverage.sample_start.0 as f64/48000.0,
            "next_needed":state.live.as_ref().and_then(|l|l.next_needed(owner,seconds))})
    }).collect::<Vec<_>>();
    let previous=plan.requests.iter().min_by_key(|(owner,r)|(r.coverage.sample_start.0,**owner)).map(|(&owner,_)|owner);
    serde_json::json!({"demand_seconds":seconds,"queued":rows,"previous_background_owner_without_warm_cursor":previous,
        "actual_next_work":state.next_work(),"neural_execution":false})
}
