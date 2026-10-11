use super::Extra;
use serde::{Deserialize, Serialize};

pub const QUARTER: i64 = 705_600_000;
pub fn beats(ticks: i64) -> f64 {
    ticks as f64 / QUARTER as f64
}
pub fn ticks(beats: f64) -> i64 {
    (beats * QUARTER as f64).round() as i64
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tempo {
    pub position: i64,
    pub bpm: f64,
    #[serde(flatten)]
    pub extra: Extra,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meter {
    pub index: i32,
    pub numerator: u32,
    pub denominator: u32,
    #[serde(flatten)]
    pub extra: Extra,
}
impl Meter {
    pub fn bar_length(&self) -> i64 {
        QUARTER * 4 / self.denominator as i64 * self.numerator as i64
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeAxis {
    pub meter: Vec<Meter>,
    pub tempo: Vec<Tempo>,
    pub start_time_seconds: f64,
    #[serde(flatten)]
    pub extra: Extra,
}
impl Default for TimeAxis {
    fn default() -> Self {
        Self {
            meter: vec![Meter {
                index: 0,
                numerator: 4,
                denominator: 4,
                extra: Extra::new(),
            }],
            tempo: vec![Tempo {
                position: 0,
                bpm: 120.0,
                extra: Extra::new(),
            }],
            start_time_seconds: 0.0,
            extra: Extra::new(),
        }
    }
}
impl TimeAxis {
    pub fn meter_at(&self, index: i32) -> &Meter {
        self.meter
            .iter()
            .rev()
            .find(|m| m.index <= index)
            .expect("time axis begins at bar zero")
    }
    pub fn bar_position(&self, index: i32) -> i64 {
        let index = index.max(0);
        let mut position = 0_i64;
        for (i, meter) in self.meter.iter().enumerate() {
            if meter.index >= index {
                break;
            }
            let end = self.meter.get(i + 1).map_or(index, |m| m.index.min(index));
            position = position
                .saturating_add(((end - meter.index) as i64).saturating_mul(meter.bar_length()));
        }
        position
    }
    /// Returns the containing bar's index and absolute start in blicks.
    pub fn bar_at(&self, position: i64) -> (i32, i64) {
        let position = position.max(0);
        let mut start = 0_i64;
        for (i, meter) in self.meter.iter().enumerate() {
            let length = meter.bar_length();
            if let Some(next) = self.meter.get(i + 1) {
                let end = start
                    .saturating_add(((next.index - meter.index) as i64).saturating_mul(length));
                if position >= end {
                    start = end;
                    continue;
                }
            }
            let bars = ((position - start) / length).min((i32::MAX - meter.index) as i64);
            return (meter.index + bars as i32, start + bars * length);
        }
        unreachable!("time axis has an initial meter")
    }
    pub fn bpm_at(&self, position: i64) -> f64 {
        self.tempo
            .iter()
            .rev()
            .find(|t| t.position <= position)
            .map_or(120.0, |t| t.bpm)
    }
    pub fn seconds_at(&self, position: i64) -> f64 {
        let mut seconds = self.start_time_seconds;
        for (i, tempo) in self.tempo.iter().enumerate() {
            let end = self
                .tempo
                .get(i + 1)
                .map_or(position, |t| t.position.min(position));
            if position <= tempo.position {
                break;
            }
            seconds += beats(end - tempo.position) * 60.0 / tempo.bpm;
        }
        seconds
    }
    pub fn position_at(&self, seconds: f64) -> i64 {
        let mut left = (seconds - self.start_time_seconds).max(0.0);
        for (i, tempo) in self.tempo.iter().enumerate() {
            if let Some(next) = self.tempo.get(i + 1) {
                let duration = beats(next.position - tempo.position) * 60.0 / tempo.bpm;
                if left >= duration {
                    left -= duration;
                    continue;
                }
            }
            // Seconds-to-blick conversion truncates toward zero.
            return tempo.position + (left * tempo.bpm / 60.0 * QUARTER as f64) as i64;
        }
        0
    }
    pub fn bars(&self, until: f64) -> Vec<(f64, i32, u32, u32)> {
        let mut result = Vec::new();
        let (mut beat, mut index, mut current) = (0.0, 0, &self.meter[0]);
        while beat <= until && result.len() < 10_000 {
            if let Some(m) = self.meter.iter().find(|m| m.index == index) {
                current = m;
            }
            result.push((beat, index, current.numerator, current.denominator));
            beat += current.numerator as f64 * 4.0 / current.denominator as f64;
            index += 1;
        }
        result
    }
}
