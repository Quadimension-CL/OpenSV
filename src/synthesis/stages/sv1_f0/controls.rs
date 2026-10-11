//! Default singing/rest controls from the two DDS editor adapters.

use super::Contract;

use crate::synthesis::{
    inference::noise::seeds::SeedSource,
    stages::{
        f0::{TimedPhone, expression::neutral_singing_controls, frontend::FrontendNote},
        sv1_noise::Normal,
    },
    voice::{Result, VoiceError},
};

pub(super) struct Controls {
    pub positions: Vec<[f32; 3]>,
    pub noise: Vec<f32>,
}

#[derive(Clone, Copy)]
pub(super) struct Defaults {
    /// Base noise temperature; SV2 compatibility uses 1.
    pub temperature: f32,
    /// Additional temperature multiplier when the source note flag is set.
    pub flagged_temperature_scale: f32,
    /// Base position scaling; SV2 compatibility uses 1.
    pub position_scale: f32,
}

pub(super) fn prepare(
    notes: &[FrontendNote],
    note_frames: &[usize],
    phones: &[TimedPhone],
    phone_frames: &[usize],
    defaults: Defaults,
    contract: Contract,
) -> Result<Controls> {
    if notes.len() != note_frames.len()
        || phones.len() != phone_frames.len()
        || !defaults.temperature.is_finite()
        || !defaults.flagged_temperature_scale.is_finite()
        || !defaults.position_scale.is_finite()
        || notes.iter().any(|note| note.kind > 1)
    {
        return Err(VoiceError::new("Invalid version 1 F0 default controls"));
    }
    let total = note_frames
        .iter()
        .try_fold(0_usize, |sum, &count| sum.checked_add(count))
        .filter(|&total| total <= i32::MAX as usize)
        .ok_or_else(|| VoiceError::new("Version 1 F0 frame count overflow"))?;
    let mut temperature = Vec::with_capacity(total);
    let mut position_a = vec![f32::from_bits(0x3f7e_b852); total];
    let mut position_c = Vec::with_capacity(total);
    for (note, &count) in notes.iter().zip(note_frames) {
        let (note_temperature, value) = match contract {
            Contract::Compatible => {
                let controls = neutral_singing_controls(note.kind, note.flag);
                let mut temperature = if note.flag {
                    f64::from(defaults.temperature) * f64::from(defaults.flagged_temperature_scale)
                } else {
                    f64::from(defaults.temperature)
                };
                if let Some(controls) = controls {
                    temperature *= f64::from(controls.expression + controls.expression);
                }
                (
                    temperature as f32,
                    controls.map_or(0.0, |controls| controls.position_c),
                )
            }
            Contract::Native { enhancement } => {
                // The NOFS special-mode flag is separate from the connection
                // flag in FrontendNote.flag. Ordinary notes have expression .5,
                // so its multiplier is one; synthetic rests have no expression
                // and keep position zero.
                (
                    defaults.temperature,
                    if note.kind == 0 { enhancement } else { 0.0 },
                )
            }
        };
        temperature.extend(std::iter::repeat_n(note_temperature, count));
        position_c.extend(std::iter::repeat_n(value, count));
    }
    smooth(&mut temperature);
    smooth(&mut position_a);
    smooth(&mut position_c);
    let positions = temperature
        .iter()
        .zip(position_a)
        .zip(position_c)
        .map(|((&temperature, a), c)| {
            let position_b = match contract {
                Contract::Compatible => {
                    // The compatibility mapping requires f64 arithmetic.
                    let scale = if f64::from(temperature) > 0.8 {
                        ((1.2 - f64::from(temperature)) / 0.4).max(0.0)
                    } else {
                        1.0
                    };
                    (f64::from(defaults.position_scale) * scale) as f32
                }
                Contract::Native { .. } => {
                    // Keep the NOFS mapping's f32 multiply/add order.
                    if temperature > 0.8 {
                        defaults.position_scale * (temperature * -2.5 + 3.0).max(0.0)
                    } else {
                        defaults.position_scale
                    }
                }
            };
            [a, position_b, c]
        })
        .collect();
    let tokens: Vec<&str> = phones.iter().map(|phone| phone.token.as_str()).collect();
    let mut phone_seeds = SeedSource::context(&tokens, phone_frames, "magic", 0)
        .map_err(|error| VoiceError::new(error.to_string()))?;
    let mut note_seeds = SeedSource::new(&vec![0; notes.len()], note_frames)
        .map_err(|error| VoiceError::new(error.to_string()))?;
    let mut normal = Normal::new(123);
    let mut last_phone = 0;
    let mut last_note = 0;
    let noise = temperature
        .into_iter()
        .map(|temperature| {
            let phone = phone_seeds.next_event();
            let note = note_seeds.next_event();
            if let Some(seed) = phone {
                last_phone = seed;
            }
            if let Some(seed) = note {
                last_note = seed;
            }
            if phone.is_some() || note.is_some() {
                normal.reseed(last_phone ^ last_note);
            }
            let amplitude = if temperature < 0.5 {
                temperature * 2.0
            } else {
                1.0
            };
            normal.next() * amplitude
        })
        .collect();
    Ok(Controls { positions, noise })
}

fn smooth(values: &mut [f32]) {
    let previous_weight = 1.0_f32 - 0.95;
    for index in 1..values.len() {
        values[index] = previous_weight * values[index - 1] + 0.95 * values[index];
    }
    for index in (0..values.len().saturating_sub(1)).rev() {
        values[index] = previous_weight * values[index + 1] + 0.95 * values[index];
    }
}
