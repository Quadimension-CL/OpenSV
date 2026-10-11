//! Preview clicks use the same sample position as vocals and accompaniment.
//! Only the current/next beat is located; no project-length click PCM is built.
use super::live::LIVE_SAMPLE_RATE;
use crate::model::{QUARTER, TimeAxis};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

pub struct Metronome {
    pub(crate) time: TimeAxis,
    enabled: Arc<AtomicBool>,
    sounds: &'static ClickSounds,
}

impl Metronome {
    pub fn new(time: TimeAxis, enabled: Arc<AtomicBool>) -> Self {
        static SOUNDS: OnceLock<ClickSounds> = OnceLock::new();
        Self {
            time,
            enabled,
            sounds: SOUNDS.get_or_init(ClickSounds::new),
        }
    }

    fn beat_at(&self, position: f64) -> Beat {
        // Quantize beat onsets to the native sample grid. Looking half a sample
        // ahead prevents a rounded-down onset from being missed until next tick.
        let tick = self
            .time
            .position_at((position + 0.5) / f64::from(LIVE_SAMPLE_RATE));
        let (bar, start) = self.time.bar_at(tick);
        let meter = self.time.meter_at(bar);
        let step = QUARTER * 4 / i64::from(meter.denominator);
        let index = (tick - start) / step;
        let onset = start + index * step;
        let frame = |tick| (self.time.seconds_at(tick) * f64::from(LIVE_SAMPLE_RATE)).round();
        Beat {
            frame: frame(onset),
            next: frame(onset.saturating_add(step)),
            accent: index == 0,
        }
    }
}

struct Beat {
    frame: f64,
    next: f64,
    accent: bool,
}

pub(crate) struct MetronomeSampler<'a> {
    metronome: &'a Metronome,
    beat: Option<Beat>,
}

impl<'a> MetronomeSampler<'a> {
    pub fn new(metronome: &'a Metronome) -> Self {
        Self {
            metronome,
            beat: None,
        }
    }

    pub fn sample(&mut self, position: f64) -> f32 {
        if !self.metronome.enabled.load(Ordering::Relaxed) {
            return 0.0;
        }
        if self.beat.as_ref().is_none_or(|beat| position >= beat.next) {
            self.beat = Some(self.metronome.beat_at(position));
        }
        let beat = self.beat.as_ref().expect("beat was located");
        let elapsed = position - beat.frame;
        if elapsed < 0.0 {
            return 0.0;
        }
        let sound = if beat.accent {
            &self.metronome.sounds.strong
        } else {
            &self.metronome.sounds.weak
        };
        let index = elapsed.floor() as usize;
        let Some(&a) = sound.get(index) else {
            return 0.0;
        };
        let b = sound.get(index + 1).copied().unwrap_or(0.0);
        a + (b - a) * elapsed.fract() as f32
    }
}

struct ClickSounds {
    strong: Vec<f32>,
    weak: Vec<f32>,
}

impl ClickSounds {
    fn new() -> Self {
        Self {
            strong: click(1850.0, 0.24),
            weak: click(1250.0, 0.16),
        }
    }
}

fn click(frequency: f64, gain: f64) -> Vec<f32> {
    let frames = (f64::from(LIVE_SAMPLE_RATE) * 0.040) as usize;
    (0..frames)
        .map(|frame| {
            let seconds = frame as f64 / f64::from(LIVE_SAMPLE_RATE);
            let phase = std::f64::consts::TAU * frequency * seconds;
            let attack = (seconds / 0.0005).min(1.0);
            let tail = 1.0 - frame as f64 / frames as f64;
            let envelope = attack * (-seconds * 110.0).exp() * tail * tail;
            ((phase.sin() * 0.8 + (phase * 2.0).sin() * 0.2) * envelope * gain) as f32
        })
        .collect()
}
