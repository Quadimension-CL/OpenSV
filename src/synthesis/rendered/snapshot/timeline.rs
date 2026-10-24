use super::{VisualFlash, VisualPatch, VisualPiece, VisualSnapshot, intersection, matching_input};
use crate::synthesis::{plan::OwnerInput, rendered::RenderVisuals};
use std::{ops::Range, sync::Arc, time::Instant};

#[derive(Clone, Debug, Default)]
pub struct VisualTimeline {
    input: Option<Arc<OwnerInput>>,
    pieces: Vec<VisualPiece>,
    flashes: Vec<VisualFlash>,
    completed: Option<Arc<RenderVisuals>>,
}

impl VisualTimeline {
    pub fn begin(&mut self, input: Arc<OwnerInput>) {
        if self
            .input
            .as_ref()
            .is_some_and(|old| matching_input(old, &input))
        {
            return;
        }
        let bounds = visible_range(&input);
        self.pieces.retain_mut(|piece| {
            if let Some(range) = intersection(&piece.range, &bounds) {
                piece.range = range;
                true
            } else {
                false
            }
        });
        self.flashes.clear();
        self.completed = None;
        self.input = Some(input);
    }

    pub fn apply(&mut self, patch: VisualPatch, now: Instant) {
        if !self.accepts(&patch.input) {
            return;
        }
        let Some(range) = intersection(&patch.range, &visible_range(&patch.input)) else {
            return;
        };
        self.completed = None;
        let mut pieces = Vec::with_capacity(self.pieces.len() + 2);
        for piece in self.pieces.drain(..) {
            if intersection(&piece.range, &range).is_none() {
                pieces.push(piece);
                continue;
            }
            if piece.range.start < range.start {
                pieces.push(VisualPiece {
                    range: piece.range.start..range.start,
                    snapshot: piece.snapshot.clone(),
                });
            }
            if range.end < piece.range.end {
                pieces.push(VisualPiece {
                    range: range.end..piece.range.end,
                    snapshot: piece.snapshot,
                });
            }
        }
        let has_data = patch.data.is_some();
        if let Some(data) = patch.data {
            pieces.push(VisualPiece {
                range: range.clone(),
                snapshot: VisualSnapshot {
                    data,
                    input: patch.input,
                },
            });
        }
        pieces.sort_unstable_by_key(|piece| piece.range.start);
        self.pieces = coalesce(pieces);
        if patch.highlight || !has_data {
            self.replace_flashes(&range, now);
        } else {

            self.flashes.retain(|flash| flash.intensity(now) > 0.0);
        }
        if patch.highlight && has_data {
            self.flashes.push(VisualFlash {
                range,
                started: now,
            });
        }
    }

    pub fn complete(&mut self, snapshot: VisualSnapshot) {
        if !self.accepts(&snapshot.input)
            || self
                .completed
                .as_ref()
                .is_some_and(|data| Arc::ptr_eq(data, &snapshot.data))
        {
            return;
        }
        self.completed = Some(snapshot.data.clone());
        self.pieces.clear();
        let range = visible_range(&snapshot.input);
        if range.start < range.end {
            self.pieces.push(VisualPiece { range, snapshot });
        }
    }

    pub fn pieces(&self) -> &[VisualPiece] {
        &self.pieces
    }

    pub fn intensity(&self, position: i64, now: Instant) -> f32 {
        self.flashes
            .iter()
            .filter(|flash| flash.range.contains(&position))
            .map(|flash| flash.intensity(now))
            .fold(0.0, f32::max)
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.flashes.iter().any(|flash| flash.intensity(now) > 0.0)
    }

    pub fn animating_in(&self, range: Range<i64>, now: Instant) -> bool {
        self.flashes
            .iter()
            .any(|flash| intersection(&flash.range, &range).is_some() && flash.intensity(now) > 0.0)
    }

    fn accepts(&mut self, input: &Arc<OwnerInput>) -> bool {
        if self.input.is_none() {
            self.begin(input.clone());
        }
        self.input
            .as_ref()
            .is_some_and(|current| matching_input(current, input))
    }

    fn replace_flashes(&mut self, range: &Range<i64>, now: Instant) {
        let mut flashes = Vec::with_capacity(self.flashes.len() + 1);
        for flash in self.flashes.drain(..) {
            if flash.intensity(now) == 0.0 {
                continue;
            }
            if intersection(&flash.range, range).is_none() {
                flashes.push(flash);
                continue;
            }
            if flash.range.start < range.start {
                flashes.push(VisualFlash {
                    range: flash.range.start..range.start,
                    started: flash.started,
                });
            }
            if range.end < flash.range.end {
                flashes.push(VisualFlash {
                    range: range.end..flash.range.end,
                    started: flash.started,
                });
            }
        }
        self.flashes = flashes;
    }
}

fn visible_range(input: &OwnerInput) -> Range<i64> {
    input.publication_range()
}

fn coalesce(pieces: Vec<VisualPiece>) -> Vec<VisualPiece> {
    let mut result: Vec<VisualPiece> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        if let Some(previous) = result.last_mut()
            && previous.range.end == piece.range.start
            && Arc::ptr_eq(&previous.snapshot.data, &piece.snapshot.data)
            && matching_input(&previous.snapshot.input, &piece.snapshot.input)
        {
            previous.range.end = piece.range.end;
        } else {
            result.push(piece);
        }
    }
    result
}
