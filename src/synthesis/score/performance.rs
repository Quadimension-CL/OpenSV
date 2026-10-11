//! Note performance controls sampled on the owner's musical grid, then mapped
//! to seconds independently of phoneme splitting and model boundary records.

use crate::{
    model::{NotePerformance, QUARTER},
    synthesis::{
        plan::{GlobalBlick, OwnerInput},
        voice::{Result, VoiceError},
    },
};

const GRID: i64 = QUARTER / 64;

#[derive(Clone, Debug)]
struct ControlPoint {
    seconds: f64,
    value: f32,
}

#[derive(Clone, Debug)]
pub struct PreparedPerformanceControls {
    vibrato: Vec<ControlPoint>,
}

impl PreparedPerformanceControls {
    pub fn new(input: &OwnerInput) -> Result<Option<Self>> {
        let mut notes = Vec::new();
        for note in &input.notes {
            if super::options::muted(&note.attributes)? {
                continue;
            }
            let end = note
                .onset
                .0
                .checked_add(note.duration)
                .filter(|end| *end > note.onset.0)
                .ok_or_else(|| VoiceError::new("Invalid performance control note range"))?;
            let performance = NotePerformance::from_parts(&note.musical_type, &note.attributes)
                .map_err(VoiceError::new)?;
            notes.push((note.onset.0, end, performance.vibrato));
        }
        notes.sort_by_key(|note| note.0);
        let Some(&(begin, _, _)) = notes.first() else {
            return Ok(None);
        };
        if input.tempo.points.is_empty() {
            return Err(VoiceError::new("Missing performance control tempo mapping"));
        }
        let end = notes.iter().map(|note| note.1).max().unwrap();
        let count = end
            .checked_sub(begin)
            .and_then(|span| usize::try_from(span / GRID).ok())
            .ok_or_else(|| VoiceError::new("Performance control range overflows"))?;
        let mut modulation = Vec::new();
        modulation
            .try_reserve_exact(count)
            .map_err(|_| VoiceError::new("Performance control range is too large"))?;
        modulation.resize(count, 0.0_f64);
        for (onset, end, value) in notes {
            // The note bounds use integer division on the original note grid.
            // Gaps retain the neutral modulation, and the end is exclusive.
            let first = ((onset - begin) / GRID) as usize;
            let stop = ((end - begin) / GRID) as usize;
            modulation[first..stop].fill(value - 1.0);
        }
        let mut vibrato = Vec::new();
        vibrato
            .try_reserve_exact(count)
            .map_err(|_| VoiceError::new("Performance control range is too large"))?;
        for (index, modulation) in modulation.into_iter().enumerate() {
            let position = begin
                .checked_add(index as i64 * GRID)
                .and_then(|local| local.checked_add(input.content_offset.0))
                .ok_or_else(|| VoiceError::new("Performance control position overflows"))?;
            let seconds = input.tempo.seconds_at(GlobalBlick(position));
            if !seconds.is_finite()
                || vibrato
                    .last()
                    .is_some_and(|previous: &ControlPoint| previous.seconds >= seconds)
            {
                return Err(VoiceError::new("Invalid performance control tempo mapping"));
            }
            // Parameter envelopes are validated separately. Their supported
            // neutral value is one; retain the f64 offset arithmetic here.
            vibrato.push(ControlPoint {
                seconds,
                value: (modulation + 1.0) as f32,
            });
        }
        Ok(Some(Self { vibrato }))
    }

    pub fn memory_bytes(&self) -> usize {
        self.vibrato.capacity() * std::mem::size_of::<ControlPoint>()
    }

    pub fn sample(&self, origin: f64, step: f64, count: usize) -> Result<Vec<f32>> {
        if !origin.is_finite()
            || !step.is_finite()
            || step <= 0.0
            || (count > 0 && !(origin + (count - 1) as f64 * step).is_finite())
        {
            return Err(VoiceError::new("Invalid performance control frame clock"));
        }
        let mut result = Vec::with_capacity(count);
        let mut right = self.vibrato.partition_point(|point| point.seconds < origin);
        for frame in 0..count {
            let seconds = origin + frame as f64 * step;
            while right < self.vibrato.len() && self.vibrato[right].seconds < seconds {
                right += 1;
            }
            let value = if self.vibrato.is_empty() {
                // A span shorter than one musical-grid step has no points.
                0.0
            } else if right == 0 {
                self.vibrato[0].value
            } else if right == self.vibrato.len() {
                self.vibrato[right - 1].value
            } else {
                let left = &self.vibrato[right - 1];
                let right = &self.vibrato[right];
                let difference = f64::from(right.value - left.value);
                (f64::from(left.value)
                    + difference * (seconds - left.seconds) / (right.seconds - left.seconds))
                    as f32
            };
            result.push(value);
        }
        Ok(result)
    }
}
