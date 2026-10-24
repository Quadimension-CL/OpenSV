mod preset;
mod voice;

use preset::NotePreset;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use voice::Voice;

const HELD: u64 = 0x80;
const SERIAL_MASK: u64 = !0xff;
const VOICES: usize = 4;

#[derive(Clone, Default)]
pub struct PianoControl {
    state: Arc<ControlState>,
}

#[derive(Default)]
struct ControlState {
    event: AtomicU64,
    consumed: AtomicU64,
}

impl PianoControl {
    pub fn set_note(&self, note: Option<u8>) {
        if let Some(note) = note {
            let _ = self
                .state
                .event
                .try_update(Ordering::AcqRel, Ordering::Acquire, |previous| {
                    Some(
                        (previous.wrapping_add(0x100) & SERIAL_MASK)
                            | HELD
                            | u64::from(note.min(127)),
                    )
                });
        } else {
            self.state.event.fetch_and(!HELD, Ordering::AcqRel);
        }
    }
}

pub(super) struct PianoSynth {
    control: PianoControl,
    presets: Box<[NotePreset]>,
    voices: [Voice; VOICES],
    serial: u64,
    pressed: Option<usize>,
    attack_frames: u32,
    minimum_frames: u32,
    release_decay: f32,
}

impl PianoSynth {

    pub fn new(sample_rate: u32, control: PianoControl) -> Self {
        let sample_rate = f64::from(sample_rate);

        let serial = control.state.consumed.load(Ordering::Acquire);
        Self {
            control,
            presets: (0..=127)
                .map(|note| NotePreset::new(note, sample_rate))
                .collect(),
            voices: std::array::from_fn(|_| Voice::default()),
            serial,
            pressed: None,
            attack_frames: (sample_rate * 0.002).round().max(1.0) as u32,
            minimum_frames: (sample_rate * 0.055).round().max(1.0) as u32,
            release_decay: (-6.907_755 / (sample_rate * 0.12)).exp() as f32,
        }
    }

    pub fn begin_buffer(&mut self) {
        for voice in &mut self.voices {
            voice.prune_partials();
        }
        let event = self.control.state.event.load(Ordering::Acquire);
        let serial = event & SERIAL_MASK;
        if serial != self.serial {
            self.serial = serial;
            if let Some(pressed) = self.pressed.take() {
                self.voices[pressed].release();
            }
            let index = self
                .voices
                .iter()
                .position(|voice| !voice.active())
                .unwrap_or_else(|| {

                    self.voices
                        .iter()
                        .enumerate()
                        .max_by_key(|(_, voice)| voice.age())
                        .map_or(0, |(index, _)| index)
                });
            self.voices[index] =
                Voice::strike(&self.presets[(event & 0x7f) as usize], event as u32);
            self.pressed = Some(index);
            self.control.state.consumed.store(serial, Ordering::Release);
        }
        if event & HELD == 0
            && let Some(pressed) = self.pressed.take()
        {
            self.voices[pressed].release();
        }
    }

    pub fn sample(&mut self) -> f32 {
        self.voices
            .iter_mut()
            .map(|voice| voice.sample(self.attack_frames, self.minimum_frames, self.release_decay))
            .sum()
    }
}
