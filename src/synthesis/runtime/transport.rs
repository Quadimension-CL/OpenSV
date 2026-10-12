use super::{Runtime, worker};

impl Runtime {
    pub fn set_loop_range(&mut self, range: Option<(f64, f64)>) {
        if range == self.loop_range {
            return;
        }
        if let Err(error) = self.playback.set_loop_range(range) {
            self.error = Some(error);
            return;
        }
        self.loop_range = range;
        if range.is_some() && self.playback.snapshot().position_seconds.is_none() {
            self.refresh_playback();
        }
    }

    pub fn playback_clock(&self, playing: bool) -> Option<f64> {
        if !playing || !self.desired_playing {
            return None;
        }
        let snapshot = self.playback.snapshot();
        if snapshot.ready {
            // Pending blocks keep the device clock at the requested position.
            // After the actual source end, let the editor finish its own tail.
            return (snapshot.playing || snapshot.buffering)
                .then_some(snapshot.position_seconds)
                .flatten();
        }
        // Loading a new source must not advance the editor's wall clock while
        // the worker or device is still preparing the first requested block.
        (!self.planner.plan().requests.is_empty()
            || self.recordings.end > 0
            || self.metronome_enabled()
            || self.loop_range.is_some())
        .then_some(self.desired_seconds)
    }

    pub fn sync_transport(&mut self, playing: bool, seconds: f64, seek: bool) {
        let seconds = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        let changed = playing != self.desired_playing;
        // The device wraps independently. Only synthesis priority needs a
        // seek notification when its clock jumps back to the loop entrance.
        let wrapped = playing && self.loop_range.is_some() && seconds < self.desired_seconds;
        if changed || seek {
            self.playback.control(playing, Some(seconds));
        }
        let block = (seconds * 2.0).floor() as u64;
        if changed || seek || wrapped || self.demand_block != Some(block) {
            for worker in self.voices.values().filter_map(|v| v.worker.as_ref()) {
                worker.send(worker::Command::Demand {
                    seconds,
                    playing,
                    seek: changed || seek || wrapped,
                });
            }
            self.demand_block = Some(block);
        }
        self.desired_playing = playing;
        self.desired_seconds = seconds;
    }
}
