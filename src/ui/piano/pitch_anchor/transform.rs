use crate::model::{GroupRef, PitchControl};
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub(super) struct Bounds {
    pub left: i64,
    pub right: i64,
    pub bottom: f64,
    pub top: f64,
}

pub(super) enum Transform {
    Move { time: i64, pitch: f64 },
    Left(i64),
    Right(i64),
    Bottom(f64),
    Top(f64),
}

pub(super) fn bounds(controls: &[PitchControl], selected: &BTreeSet<String>) -> Option<Bounds> {
    controls
        .iter()
        .filter(|control| id(control).is_some_and(|id| selected.contains(id)))
        .filter_map(control_bounds)
        .reduce(|a, b| Bounds {
            left: a.left.min(b.left),
            right: a.right.max(b.right),
            bottom: a.bottom.min(b.bottom),
            top: a.top.max(b.top),
        })
}

fn id(control: &PitchControl) -> Option<&String> {
    match control {
        PitchControl::Point(point) => Some(&point.id),
        PitchControl::Curve(curve) => Some(&curve.id),
        PitchControl::Unsupported(_) => None,
    }
}

fn control_bounds(control: &PitchControl) -> Option<Bounds> {
    match control {
        PitchControl::Point(point) if point.valid() => Some(Bounds {
            left: point.pos,
            right: point.pos,
            bottom: point.pitch,
            top: point.pitch,
        }),
        PitchControl::Curve(curve) if curve.valid() => {
            let (left, right) = curve.range()?;
            let (bottom, top) = curve.absolute_points().fold(
                (f64::INFINITY, f64::NEG_INFINITY),
                |(low, high), (_, pitch)| (low.min(pitch), high.max(pitch)),
            );
            Some(Bounds {
                left,
                right,
                bottom,
                top,
            })
        }
        _ => None,
    }
}

pub(super) fn apply(
    original: &[PitchControl],
    selected: &BTreeSet<String>,
    reference: &GroupRef,
    transform: Transform,
) -> Vec<PitchControl> {
    let begin = reference
        .blick_absolute_begin
        .saturating_sub(reference.blick_offset);
    let end = reference
        .visible_end()
        .saturating_sub(reference.blick_offset);
    let selected: BTreeSet<_> = original
        .iter()
        .filter(|control| {
            control_bounds(control).is_some_and(|b| b.left >= begin && b.right <= end)
        })
        .filter_map(id)
        .filter(|id| selected.contains(*id))
        .cloned()
        .collect();
    let Some(area) = bounds(original, &selected) else {
        return original.to_vec();
    };
    let Some(mapping) = Mapping::new(original, &selected, area, reference, transform) else {
        return original.to_vec();
    };
    original
        .iter()
        .map(|control| {
            if id(control).is_some_and(|id| selected.contains(id)) {
                mapping.control(control)
            } else {
                Some(control.clone())
            }
        })

        .collect::<Option<Vec<_>>>()
        .unwrap_or_else(|| original.to_vec())
}

struct Mapping {
    time_origin: i64,
    time_scale: f64,
    time_shift: i64,
    pitch_origin: f64,
    pitch_scale: f64,
    pitch_shift: f64,
}

impl Mapping {
    fn new(
        controls: &[PitchControl],
        selected: &BTreeSet<String>,
        area: Bounds,
        reference: &GroupRef,
        transform: Transform,
    ) -> Option<Self> {
        let begin = reference
            .blick_absolute_begin
            .saturating_sub(reference.blick_offset);
        let end = reference
            .visible_end()
            .saturating_sub(reference.blick_offset);
        let low = -f64::from(reference.pitch_offset);
        let high = 127.0 - f64::from(reference.pitch_offset);
        if begin > end || area.bottom < low || area.top > high {
            return None;
        }
        let mut mapping = Self {
            time_origin: 0,
            time_scale: 1.0,
            time_shift: 0,
            pitch_origin: 0.0,
            pitch_scale: 1.0,
            pitch_shift: 0.0,
        };
        match transform {
            Transform::Move { time, pitch } if pitch.is_finite() => {
                mapping.time_shift = time.clamp(
                    begin.saturating_sub(area.left),
                    end.saturating_sub(area.right),
                );
                mapping.pitch_shift = pitch.clamp(low - area.bottom, high - area.top);
            }
            Transform::Left(time) if area.left < area.right => {
                let minimum = minimum_width(controls, selected, area);
                let left = time.clamp(begin, area.right.saturating_sub(minimum));
                mapping.time_origin = area.right;
                mapping.time_scale = span(left, area.right) / span(area.left, area.right);
            }
            Transform::Right(time) if area.left < area.right => {
                let minimum = minimum_width(controls, selected, area);
                let right = time.clamp(area.left.saturating_add(minimum), end);
                mapping.time_origin = area.left;
                mapping.time_scale = span(area.left, right) / span(area.left, area.right);
            }
            Transform::Bottom(pitch) if pitch.is_finite() && area.top > area.bottom => {
                let center = (area.bottom + area.top) * 0.5;
                mapping.scale_pitch(area, low, high, center - pitch);
            }
            Transform::Top(pitch) if pitch.is_finite() && area.top > area.bottom => {
                let center = (area.bottom + area.top) * 0.5;
                mapping.scale_pitch(area, low, high, pitch - center);
            }
            _ => return None,
        }
        Some(mapping)
    }

    fn scale_pitch(&mut self, area: Bounds, low: f64, high: f64, radius: f64) {
        self.pitch_origin = (area.bottom + area.top) * 0.5;
        let maximum = (self.pitch_origin - low).min(high - self.pitch_origin);
        let radius = radius.clamp(1e-6_f64.min(maximum), maximum);
        self.pitch_scale = radius * 2.0 / (area.top - area.bottom);
    }

    fn time(&self, time: i64) -> Option<i64> {
        let offset = i128::from(time) - i128::from(self.time_origin);
        let scaled = if self.time_scale == 1.0 {
            offset
        } else {
            (offset as f64 * self.time_scale).round() as i128
        };
        (i128::from(self.time_origin) + scaled + i128::from(self.time_shift))
            .try_into()
            .ok()
    }

    fn pitch(&self, pitch: f64) -> f64 {
        (pitch - self.pitch_origin) * self.pitch_scale + self.pitch_origin + self.pitch_shift
    }

    fn control(&self, original: &PitchControl) -> Option<PitchControl> {
        let mut control = original.clone();
        match &mut control {
            PitchControl::Point(point) => {
                point.pos = self.time(point.pos)?;
                point.pitch = self.pitch(point.pitch);
                point.valid().then_some(control)
            }
            PitchControl::Curve(curve) => {
                let origin = curve.pos;
                curve.pos = self.time(origin)?;
                curve.pitch = self.pitch(curve.pitch);
                for (time, pitch) in &mut curve.points {
                    *time = self
                        .time(origin.checked_add(*time)?)?
                        .checked_sub(curve.pos)?;
                    *pitch *= self.pitch_scale;
                }
                curve.valid().then_some(control)
            }
            PitchControl::Unsupported(_) => Some(control),
        }
    }
}

fn span(left: i64, right: i64) -> f64 {
    (i128::from(right) - i128::from(left)) as f64
}

fn minimum_width(controls: &[PitchControl], selected: &BTreeSet<String>, area: Bounds) -> i64 {
    let step = controls
        .iter()
        .filter_map(|control| match control {
            PitchControl::Curve(curve) if selected.contains(&curve.id) => Some(curve),
            _ => None,
        })
        .flat_map(|curve| {
            curve
                .points
                .windows(2)
                .map(|pair| span(pair[0].0, pair[1].0))
        })
        .fold(f64::INFINITY, f64::min);
    (span(area.left, area.right) / step).ceil().max(1.0) as i64
}
