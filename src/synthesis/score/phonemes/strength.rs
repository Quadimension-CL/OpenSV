use super::{PhoneControl, PreparedPhrase};
use crate::synthesis::stages::duration::DurationResult;

fn control<'a>(
    phrase: &'a PreparedPhrase,
    duration: &DurationResult,
    index: usize,
) -> Option<&'a PhoneControl> {
    let phone = &duration.durations[index];
    phrase.phone_sources[phone.record_index][phone.phone_index]
        .and_then(|source| phrase.phoneme_controls.get(&source))
}

pub fn activity(phrase: &PreparedPhrase, duration: &DurationResult, index: usize) -> Option<f64> {
    let raw = *duration.prediction.impulse_values.get(index)?;
    if raw <= 0.0 {
        return None;
    }
    let specified = control(phrase, duration, index).and_then(|c| c.activity);
    let offset = phrase.voice_parameters.consonant_strength;
    if specified.is_none() && offset == 0.0 {
        return Some(raw);
    }

    Some(f64::from(
        ((specified.map_or(raw, f64::from) + f64::from(offset)) as f32).clamp(0.0, 1.0),
    ))
}

pub struct StrengthEnvelope {
    knots: Vec<(f64, f32)>,
}
impl StrengthEnvelope {
    pub fn new(phrase: &PreparedPhrase, duration: &DurationResult) -> Option<Self> {
        let mut knots: Vec<(f64, f32)> = Vec::with_capacity(duration.durations.len() * 2);
        let mut start = phrase.start_seconds;
        let mut nonzero = false;
        for (index, phone) in duration.durations.iter().enumerate() {
            let end = start + phone.duration_seconds;

            if phrase.phone_sources[phone.record_index][phone.phone_index].is_none() {
                start = end;
                continue;
            }

            if let Some(previous_end) = knots.last_mut() {
                previous_end.0 = start - 0.001;
            }
            let mut strength = control(phrase, duration, index)
                .and_then(|c| c.strength)
                .unwrap_or(0.0);

            let raw = duration
                .prediction
                .impulse_values
                .get(index)
                .copied()
                .unwrap_or(0.0);
            if raw > 0.0 {
                let desired = (control(phrase, duration, index)
                    .and_then(|c| c.activity)
                    .unwrap_or(raw as f32)
                    + phrase.voice_parameters.consonant_strength)
                    .clamp(0.0, 1.0);
                if f64::from(desired) < raw {
                    strength = (((f64::from(desired) + 0.1) / (raw + 0.1)).ln()
                        + f64::from(strength)) as f32;
                }
            }
            nonzero |= strength != 0.0;

            knots.push((start + 0.001, strength));
            knots.push((end - 0.001, strength));
            start = end;
        }
        if !nonzero {
            return None;
        }
        knots.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
        Some(Self { knots })
    }

    pub fn sample(&self, seconds: f64) -> f32 {
        let right = self.knots.partition_point(|&(t, _)| t < seconds);
        let value = if right == 0 {
            self.knots[0].1
        } else if right == self.knots.len() {
            self.knots[right - 1].1
        } else {
            let (left, a) = self.knots[right - 1];
            let (end, b) = self.knots[right];
            if end == left {
                a
            } else {
                (f64::from(b - a) * (seconds - left) / (end - left) + f64::from(a)) as f32
            }
        };
        if f64::from(value) > -0.01 && f64::from(value) < 0.01 {
            0.0
        } else {
            value * 0.25
        }
    }
}
