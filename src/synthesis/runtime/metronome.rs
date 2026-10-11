use super::Runtime;
use crate::{
    model::{Project, QUARTER, TimeAxis},
    synthesis::audio::metronome::Metronome,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct MetronomeState {
    pub track: Arc<Metronome>,
    pub end_frames: usize,
    enabled: Arc<AtomicBool>,
}

impl Default for MetronomeState {
    fn default() -> Self {
        let enabled = Arc::new(AtomicBool::new(false));
        let time = TimeAxis::default();
        let end_frames = (time.seconds_at(32 * QUARTER) * 48_000.0).ceil() as usize;
        Self {
            track: Arc::new(Metronome::new(time, Arc::clone(&enabled))),
            end_frames,
            enabled,
        }
    }
}

impl MetronomeState {
    pub fn update(&mut self, project: &Project) {
        if self.track.time != project.time {
            self.track = Arc::new(Metronome::new(
                project.time.clone(),
                Arc::clone(&self.enabled),
            ));
        }
        // Match the editor's transport limit, including an otherwise empty song.
        let seconds = project
            .time
            .seconds_at(project.end().max(32 * QUARTER))
            .max(0.0);
        self.end_frames = (seconds * 48_000.0).ceil() as usize;
    }
}

impl Runtime {
    pub fn metronome_enabled(&self) -> bool {
        self.metronome.enabled.load(Ordering::Relaxed)
    }

    pub fn set_metronome(&mut self, enabled: bool) {
        self.metronome.enabled.store(enabled, Ordering::Relaxed);
        // Existing playback sources share this switch. Do not seek/reload them
        // on a toggle; only an empty project needs a new silent clock source.
        if enabled && !self.playback.snapshot().ready {
            self.refresh_playback();
        }
    }
}
