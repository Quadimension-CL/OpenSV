use super::{PitchControl, PitchCurve, fresh_id};
use crate::model::Extra;
use std::collections::BTreeMap;

/// Replaces existing hand-drawn data in the stroke's horizontal range. Input
/// points use group-local blicks and absolute MIDI pitch, without instance offsets.
pub fn replace_curve(controls: &mut Vec<PitchControl>, points: &[(i64, f64)]) -> bool {
    let Some(curve) = from_absolute(points) else {
        return false;
    };
    let (start, end) = curve.range().unwrap();
    erase_curves(controls, start, end);
    // The new curve wins at shared endpoints when curves are sampled in reverse.
    controls.push(PitchControl::Curve(curve));
    true
}

/// Erases the interval's interior while retaining interpolated boundaries on
/// separate curves. Unsupported native controls are never altered.
pub fn erase_curves(controls: &mut Vec<PitchControl>, start: i64, end: i64) -> bool {
    let (start, end) = (start.min(end), start.max(end));
    if start == end {
        return false;
    }
    let mut changed = false;
    let mut kept = Vec::with_capacity(controls.len());
    for control in std::mem::take(controls) {
        let PitchControl::Curve(curve) = &control else {
            kept.push(control);
            continue;
        };
        let Some((first, last)) = curve.range() else {
            kept.push(control);
            continue;
        };
        if last <= start || first >= end {
            kept.push(control);
            continue;
        }
        changed = true;
        let left = (first < start)
            .then(|| clipped(curve, first, start))
            .flatten();
        let mut right = (last > end).then(|| clipped(curve, end, last)).flatten();
        if left.is_some()
            && let Some(right) = right.as_mut()
        {
            right.id = fresh_id();
        }
        kept.extend(left.into_iter().chain(right).map(PitchControl::Curve));
    }
    *controls = kept;
    changed
}

fn from_absolute(points: &[(i64, f64)]) -> Option<PitchCurve> {
    if points.iter().any(|p| !p.1.is_finite()) {
        return None;
    }
    // Repeated positions keep the most recent value, including vertical strokes.
    let ordered: BTreeMap<i64, f64> = points.iter().copied().collect();
    let (&pos, &pitch) = ordered.first_key_value()?;
    let points = ordered
        .into_iter()
        .map(|(time, value)| Some((time.checked_sub(pos)?, value - pitch)))
        .collect::<Option<Vec<_>>>()?;
    let curve = PitchCurve {
        pos,
        pitch,
        id: fresh_id(),
        points,
        extra: Extra::new(),
    };
    curve.valid().then_some(curve)
}

fn clipped(curve: &PitchCurve, start: i64, end: i64) -> Option<PitchCurve> {
    if start >= end {
        return None;
    }
    let first = start.checked_sub(curve.pos)?;
    let last = end.checked_sub(curve.pos)?;
    let mut clipped = curve.clone();
    clipped.points = vec![(first, curve.sample(start as f64)? - curve.pitch)];
    clipped.points.extend(
        curve
            .points
            .iter()
            .copied()
            .filter(|p| p.0 > first && p.0 < last),
    );
    clipped
        .points
        .push((last, curve.sample(end as f64)? - curve.pitch));
    clipped.valid().then_some(clipped)
}
