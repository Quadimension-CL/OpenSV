//! Record-boundary noise for the duration sampler.

use std::collections::VecDeque;

use super::PhoneFeatures;
use crate::synthesis::{
    inference::noise::{NormalNoise, seeds::context_hash},
    voice::{Result, VoiceError},
};

struct SeedEntry {
    remaining: i32,
    seed: u32,
}

pub(super) fn generate(phones: &[PhoneFeatures], seeds: &[u32]) -> Result<Vec<f32>> {
    let mut lengths = vec![0_usize; seeds.len()];
    for phone in phones {
        let count = lengths
            .get_mut(phone.record_index)
            .ok_or_else(|| VoiceError::new("Duration noise phoneme references a missing record"))?;
        *count += 1;
    }

    let mut queue = VecDeque::with_capacity(seeds.len() + 1);
    // The initial zero-length entry makes the first phone a reseed boundary.
    queue.push_back(SeedEntry {
        remaining: 0,
        seed: 0,
    });
    let mut previous = String::new();
    let mut before_previous = String::new();
    for (&length, &seed) in lengths.iter().zip(seeds) {
        let remaining = i32::try_from(length)
            .map_err(|_| VoiceError::new("Duration record phone count exceeds signed range"))?;
        // A record's context token uses the prepared phone prefix of this
        // length. Keep that prefix stable instead of substituting its own span.
        let mut current = String::new();
        for phone in &phones[..length] {
            let id = i32::try_from(phone.unified_phone_index)
                .map_err(|_| VoiceError::new("Duration phone ID exceeds signed range"))?;
            current.push('/');
            current.push_str(&id.to_string());
        }
        let text = format!("{previous}_{before_previous}_{current}_{}0", seed as i32);
        queue.push_back(SeedEntry {
            remaining,
            seed: context_hash(text.as_bytes()),
        });
        before_previous = previous;
        previous = current;
    }

    let mut normal = NormalNoise::default();
    let mut output = Vec::with_capacity(phones.len());
    for _ in phones {
        // Consume one boundary per phone. Empty records and equal seeds must
        // retain their individual queue entries.
        if queue.front().is_some_and(|entry| entry.remaining == 0) {
            queue.pop_front();
            if let Some(entry) = queue.front() {
                normal.reseed(entry.seed);
            }
        }
        if let Some(entry) = queue.front_mut() {
            entry.remaining = entry.remaining.wrapping_sub(1);
        }
        output.push(normal.next_sample());
    }
    Ok(output)
}
