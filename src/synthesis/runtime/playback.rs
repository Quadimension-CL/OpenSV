mod worker;

use crate::synthesis::audio::{LoopControl, PianoControl, PlaybackControl, PlaybackSource};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread,
};

#[derive(Clone, Default)]
pub(super) struct Snapshot {
    pub ready: bool,
    pub playing: bool,
    pub buffering: bool,
    pub position_seconds: Option<f64>,
    pub error: Option<String>,
    serial: u64,
    control: Option<PlaybackControl>,
    audition_error: Option<String>,
}

enum Command {
    Transport(u64, Transport),
    Audition,
    Quit,
}

enum Transport {
    Load(Arc<PlaybackSource>, f64, bool),
    Control { playing: bool, seek: Option<f64> },
    Clear,
}

/// Device setup and source retirement stay off the UI thread. The device stream
/// persists while sources and their independently ready blocks are replaced.
pub(super) struct Playback {
    tx: Sender<Command>,
    state: Arc<Mutex<Snapshot>>,
    latest: Arc<AtomicU64>,
    piano: PianoControl,
    looping: LoopControl,
}

impl Playback {
    pub fn new(repaint: eframe::egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let latest = Arc::new(AtomicU64::new(0));
        let piano = PianoControl::default();
        let pending = Arc::clone(&latest);
        let published = Arc::clone(&state);
        let output_piano = piano.clone();
        let looping = LoopControl::default();
        let output_looping = looping.clone();
        if let Err(error) = thread::Builder::new()
            .name("audio-control".into())
            .spawn(move || {
                worker::run(
                    rx,
                    published,
                    pending,
                    output_piano,
                    output_looping,
                    repaint,
                )
            })
        {
            state.lock().expect("new playback state").error =
                Some(format!("无法启动音频线程：{error}"));
        }
        Self {
            tx,
            state,
            latest,
            piano,
            looping,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self
            .state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default();
        if snapshot.serial != self.latest.load(Ordering::Acquire) {
            snapshot.ready = false;
            snapshot.playing = false;
            snapshot.buffering = false;
        } else if snapshot.ready
            && let Some(control) = &snapshot.control
        {
            snapshot.position_seconds = Some(control.position_seconds());
            snapshot.playing = snapshot.error.is_none() && control.is_playing();
            snapshot.buffering = snapshot.error.is_none() && control.buffering();
        }
        snapshot.error = snapshot.error.take().or(snapshot.audition_error.take());
        snapshot
    }

    pub fn load_source(&self, audio: Arc<PlaybackSource>, seconds: f64, playing: bool) {
        self.send(Transport::Load(audio, seconds, playing));
    }

    pub fn set_loop_range(&self, range: Option<(f64, f64)>) -> Result<(), String> {
        self.looping
            .set_range(range)
            .map_err(|error| error.to_string())
    }

    pub fn audition_key(&self, note: Option<u8>) {
        // This shared control survives device initialization. A release or a
        // newer key cannot be overwritten by a queued note-on, and audition
        // never supersedes the independent project transport command serial.
        self.piano.set_note(note);
        if note.is_some() && self.tx.send(Command::Audition).is_err() {
            self.thread_stopped();
        }
    }

    pub fn control(&self, playing: bool, seek: Option<f64>) {
        // An installed source only needs atomic controls. Avoid the command
        // queue and avoid replaying a seek after the callback already advanced.
        if let Ok(mut state) = self.state.lock()
            && state.ready
            && state.serial == self.latest.load(Ordering::Acquire)
            && let Some(control) = &state.control
        {
            state.error = control
                .request(playing, seek)
                .err()
                .map(|error| error.to_string());
            return;
        }
        self.send(Transport::Control { playing, seek });
    }

    pub fn clear(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.ready = false;
            state.playing = false;
            state.buffering = false;
            state.position_seconds = None;
        }
        self.send(Transport::Clear);
    }

    fn send(&self, command: Transport) {
        let serial = self.latest.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        if let Ok(mut state) = self.state.lock() {
            if matches!(&command, Transport::Clear | Transport::Load(..)) {
                if let Some(control) = &state.control {
                    control.pause();
                }
                state.ready = false;
                state.buffering = false;
            }
            match &command {
                Transport::Load(_, seconds, playing) => {
                    state.position_seconds = Some(*seconds);
                    state.playing = *playing;
                }
                Transport::Control { playing, seek } => {
                    if let Some(control) = &state.control {
                        control.pause();
                    }
                    state.playing = *playing;
                    if let Some(seconds) = seek {
                        state.position_seconds = Some(*seconds);
                    }
                }
                _ => {}
            }
        }
        if self.tx.send(Command::Transport(serial, command)).is_err() {
            self.thread_stopped();
        }
    }

    fn thread_stopped(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.error = Some("音频输出线程已停止。".into());
            state.ready = false;
        }
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.piano.set_note(None);
        self.latest.fetch_add(1, Ordering::AcqRel);
        if let Ok(state) = self.state.lock()
            && let Some(control) = &state.control
        {
            control.pause();
        }
        let _ = self.tx.send(Command::Quit);
    }
}
