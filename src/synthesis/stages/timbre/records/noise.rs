//! Both timbre streams use an identical seed-event protocol with distinct salts.

use crate::synthesis::{
    inference::noise::{
        NormalNoise,
        seeds::{SeedSource, spread},
    },
    voice::{Result, VoiceError},
};

use super::{RecordsConfig, RecordsInput};

pub(super) fn generate(
    config: &RecordsConfig,
    input: &RecordsInput<'_>,
    first_frame: usize,
    width: usize,
    scale: f32,
    salt: &str,
) -> Result<Vec<f32>> {
    let lengths: Vec<_> = input.notes.iter().map(|n| n.frames).collect();
    let seeds: Vec<_> = input
        .notes
        .iter()
        .map(|n| n.timbre_seed.unwrap_or(0))
        .collect();
    let (seeds, lengths) = if config.spread_note_seeds {
        let mask: Vec<_> = input
            .notes
            .iter()
            .map(|n| n.kind == 1 && n.timbre_seed.is_none())
            .collect();
        spread(&seeds, &lengths, &mask, 100).map_err(|e| VoiceError::new(e.to_string()))?
    } else {
        (seeds, lengths)
    };
    let mut note_seed =
        SeedSource::new(&seeds, &lengths).map_err(|e| VoiceError::new(e.to_string()))?;
    let tokens: Vec<_> = input.phones.iter().map(|p| p.token).collect();
    let lengths: Vec<_> = input.phones.iter().map(|p| p.frames).collect();
    let mut text_seed = SeedSource::context(&tokens, &lengths, salt, 0)
        .map_err(|e| VoiceError::new(e.to_string()))?;
    let total = input.midi_pitch.len();
    let count = ((total - first_frame) / config.stride)
        .checked_mul(width)
        .ok_or_else(|| VoiceError::new("Acoustic noise matrix exceeds addressable memory"))?;
    let mut output = Vec::with_capacity(count);
    let mut normal = NormalNoise::default();
    let mut current_note = 0;
    let mut current_text = 0;
    // Noise must be replayed from zero even when feature preparation starts at
    // a later aligned frame. Resets also discard the cached normal partner.
    for frame in 0..total {
        let note_event = note_seed.next_event();
        let text_event = text_seed.next_event();
        if let Some(seed) = note_event {
            current_note = seed;
        }
        if let Some(seed) = text_event {
            current_text = seed;
        }
        if note_event.is_some() || text_event.is_some() {
            normal.reseed(current_note ^ current_text);
        }
        let retain = frame >= first_frame && frame % config.stride == config.stride - 1;
        for _ in 0..width {
            let sample = normal.next_sample() * scale;
            if retain {
                output.push(sample);
            }
        }
    }
    Ok(output)
}
