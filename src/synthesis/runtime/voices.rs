use super::{Runtime, VoiceState, worker};
use crate::synthesis::{
    audio::{LiveAudio, RenderedAudio},
    plan::RenderPlan,
    render_cache::{CacheStats, PublishedOwner},
    settings::VoiceSelection,
};
use std::{
    collections::BTreeSet,
    sync::{Arc, mpsc::TryRecvError},
    time::Duration,
};

pub(super) struct VoiceSlot {
    pub state: VoiceState,
    pub elapsed: Option<Duration>,
    pub worker: Option<worker::Worker>,
    pub ready: bool,
    pub context_required: bool,
    pub error: Option<String>,
    pub progress: Option<String>,
    pub revision: u64,
    pub plan: RenderPlan,
    pub solo: BTreeSet<u64>,
    pub waiting: bool,
    pub stats: CacheStats,
    pub failures: Vec<(crate::synthesis::plan::OwnerId, String)>,
    pub live: Option<Arc<LiveAudio>>,
    pub audio: Option<Arc<RenderedAudio>>,
    pub owners: Vec<PublishedOwner>,
}
impl VoiceSlot {
    pub fn new(worker: Result<worker::Worker, String>) -> Self {
        let (state, worker) = match worker {
            Ok(worker) => (VoiceState::Loading, Some(worker)),
            Err(error) => (VoiceState::Failed(error), None),
        };
        Self {
            state,
            worker,
            elapsed: None,
            ready: false,
            context_required: false,
            error: None,
            progress: None,
            revision: 0,
            plan: RenderPlan::default(),
            solo: BTreeSet::new(),
            waiting: false,
            stats: CacheStats::default(),
            failures: Vec::new(),
            live: None,
            audio: None,
            owners: Vec::new(),
        }
    }
}
impl Runtime {
    pub(super) fn poll_voices(&mut self) {
        let keys: Vec<_> = self.voices.keys().cloned().collect();
        let mut changed = false;
        for key in keys {
            loop {
                let event = self
                    .voices
                    .get(&key)
                    .and_then(|v| v.worker.as_ref())
                    .map(|w| w.rx.try_recv());
                match event {
                    Some(Ok(event)) => {
                        self.receive_voice(&key, event);
                        changed = true;
                    }
                    Some(Err(TryRecvError::Disconnected)) => {
                        let v = self.voices.get_mut(&key).unwrap();
                        v.worker = None;
                        v.waiting = false;
                        if matches!(v.state, VoiceState::Loading) {
                            v.state = VoiceState::Failed("声库加载线程意外终止。".into());
                        } else if v.error.is_none() && !matches!(v.state, VoiceState::Failed(_)) {
                            v.error = Some("合成线程意外终止。".into());
                        }
                        v.stats.queued = 0;
                        v.stats.running = 0;
                        v.progress = None;
                        changed = true;
                        break;
                    }
                    _ => break,
                }
            }
        }
        if changed {
            self.aggregate_voices();
        }
    }
    fn receive_voice(&mut self, key: &VoiceSelection, event: worker::Event) {
        let Some(v) = self.voices.get_mut(key) else {
            return;
        };
        match event {
            worker::Event::Voice { state, elapsed } => {
                v.state = state;
                v.elapsed = Some(elapsed);
                self.last_revision = None;
            }
            worker::Event::Ready => {
                v.ready = true;
                v.context_required = false;
            }
            worker::Event::ContextRequired { error } => {
                v.context_required = true;
                v.error = Some(error);
            }
            worker::Event::Error { revision, error }
                if revision.is_none_or(|r| r == v.revision) =>
            {
                v.error = Some(error)
            }
            worker::Event::Progress {
                revision,
                owner,
                stage,
            } if revision == v.revision => {
                v.progress = Some(format!("音符组 {} · {stage}", owner.0));
            }
            worker::Event::Publication {
                revision,
                audio,
                owners,
                visuals,
            } if revision == v.revision => {
                v.audio = audio;
                v.owners = owners;
                self.complete_visuals(visuals);
            }
            worker::Event::VisualPatches { revision, patches } if revision == v.revision => {
                self.patch_visuals(patches)
            }
            worker::Event::Phonemes {
                revision,
                snapshots,
            } if revision == v.revision => self.complete_phonemes(snapshots),
            worker::Event::Source { revision, source } if revision == v.revision => {
                let unchanged = v
                    .live
                    .as_ref()
                    .zip(source.as_ref())
                    .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
                v.live = source;
                if !unchanged {
                    self.refresh_playback();
                }
            }
            worker::Event::Status {
                revision,
                stats,
                failures,
            } if revision == v.revision => {
                v.stats = stats;
                v.waiting = false;
                v.failures = failures;
                if stats.running == 0 {
                    v.progress = None;
                }
            }
            _ => {}
        }
    }
    pub(super) fn aggregate_voices(&mut self) {
        self.stats = CacheStats::default();
        self.failures = self
            .binding_errors
            .iter()
            .map(|(&id, e)| (id, e.clone()))
            .collect();
        self.error = self
            .project_error
            .clone()
            .or_else(|| self.binding_errors.values().next().cloned());
        self.progress = None;
        self.export_owners.clear();
        for (key, voice) in &self.voices {
            self.stats.queued += voice.stats.queued;
            self.stats.running += voice.stats.running;
            self.stats.ready += voice.stats.ready;
            self.stats.failed += voice.stats.failed;
            self.stats.current_render_bytes += voice.stats.current_render_bytes;
            self.stats.discarded_completions += voice.stats.discarded_completions;
            self.failures.extend(voice.failures.clone());
            self.export_owners.extend(voice.owners.clone());
            let name = self
                .library
                .entry(key)
                .map_or(key.database_id.as_str(), |e| e.display_name.as_str());
            let error = match &voice.state {
                VoiceState::Failed(e) => Some(e.as_str()),
                _ => voice.error.as_deref(),
            };
            if let Some(error) = error {
                self.error.get_or_insert_with(|| format!("{name}：{error}"));
                if !voice.ready {
                    for owner in voice.plan.requests.keys() {
                        self.failures.push((*owner, format!("{name}：{error}")));
                        self.stats.failed += 1;
                    }
                }
            }
            if self.progress.is_none()
                && let Some(progress) = &voice.progress
            {
                self.progress = Some(format!("{name} · {progress}"));
            }
        }
        self.stats.failed += self.binding_errors.len();
    }
    pub(super) fn clear_voices(&mut self) {
        // A confirmed import owns its singer and must deliver its result even
        // if resource settings are refreshed while the file is being saved.
        if !self
            .context_import
            .dialog
            .as_ref()
            .is_some_and(|dialog| dialog.phase == super::ContextImportPhase::Importing)
        {
            self.context_import = super::ContextImport::default();
        }
        self.voices.clear();
        self.phonemes.clear();
        self.export_owners.clear();
        self.stats = CacheStats::default();
        self.error = None;
        self.failures.clear();
        self.progress = None;
        self.last_revision = None;
        self.demand_block = None;
        self.request_serial = self.request_serial.wrapping_add(1);
        self.playback.clear();
    }
}
