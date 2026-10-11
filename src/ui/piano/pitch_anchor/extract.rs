//! Derive editable candidates only from completed, current synthesis geometry.
//! This runs once on box-selection release, never while moving the pointer.

use crate::{
    app::App,
    model::{GroupRef, PitchControl, PitchPoint, QUARTER},
    synthesis::{
        plan::{GlobalBlick, OwnerId},
        rendered::VisualPiece,
    },
};

const VERTICAL_TOLERANCE: f64 = 0.2;
const HORIZONTAL_TOLERANCE: i64 = QUARTER / 32;
const COMPARISON_STEP: i64 = QUARTER / 512;
const EXCLUSION_TIME: i64 = QUARTER / 16;
// Bounded slices and subdivision depth avoid quadratic work on a long/noisy F0.
const SAMPLE_BATCH: usize = 8192;
const MAX_DEPTH: usize = 24;

#[derive(Clone, Copy)]
struct Sample {
    time: i64,
    pitch: f64,
}

#[derive(Clone, Copy)]
struct Bounds {
    time: (i64, i64),
    pitch: (f64, f64),
}

impl Bounds {
    fn contains(self, point: Sample) -> bool {
        point.time >= self.time.0
            && point.time <= self.time.1
            && point.pitch >= self.pitch.0
            && point.pitch <= self.pitch.1
    }
}

/// All arguments and returned points use group-local time and pitch.
pub(super) fn extract(
    app: &App,
    reference: &GroupRef,
    time: (i64, i64),
    pitch: (f64, f64),
) -> Vec<PitchPoint> {
    let mut result = Vec::new();
    if time.0 > time.1 || !pitch.0.is_finite() || !pitch.1.is_finite() || pitch.0 > pitch.1 {
        return result;
    }
    let owner = OwnerId(reference.id);
    let Some(request) = app.synthesis.planner.plan().requests.get(&owner) else {
        return result;
    };
    let Some(group) = app.editor.project.group(&reference.group_id) else {
        return result;
    };
    if app.editor.active_ref() != Some(reference)
        || !request.input.matches_project(
            &app.editor.project,
            app.editor.current_track(),
            reference,
            group,
        )
    {
        return result;
    }
    let Some(visuals) = app.synthesis.visuals(owner) else {
        return result;
    };
    let (Some(left), Some(right)) = (
        time.0.checked_add(reference.blick_offset),
        time.1.checked_add(reference.blick_offset),
    ) else {
        return result;
    };
    // Keep neighboring shape when reducing the selected interval, rather than
    // creating artificial points exactly on each edge of the selection box.
    let read_time = (
        left.saturating_sub(QUARTER)
            .max(reference.blick_absolute_begin),
        right.saturating_add(QUARTER).min(reference.visible_end()),
    );
    let bounds = Bounds { time, pitch };
    let exclusions: Vec<_> = super::controls(app).iter().filter_map(exclusion).collect();
    for piece in visuals.pieces() {
        // A timeline may retain old blocks while the new input is rendering.
        // They are useful to display, but must never become editable points.
        if *piece.snapshot.input != *request.input {
            continue;
        }
        let interval = (
            read_time.0.max(piece.range.start),
            read_time.1.min(piece.range.end),
        );
        if interval.0 >= interval.1 {
            continue;
        }
        collect(piece, interval, bounds, &exclusions, &mut result);
    }
    result.sort_unstable_by_key(|point| point.pos);
    result.dedup_by(|right, left| right.pos == left.pos);
    result
}

fn exclusion(control: &PitchControl) -> Option<Bounds> {
    let (time, pitch) = match control {
        PitchControl::Point(point) if point.valid() => {
            ((point.pos, point.pos), (point.pitch, point.pitch))
        }
        PitchControl::Curve(curve) if curve.valid() => {
            let range = curve.range()?;
            let pitches = curve.absolute_points().fold(
                (f64::INFINITY, f64::NEG_INFINITY),
                |(low, high), (_, pitch)| (low.min(pitch), high.max(pitch)),
            );
            (range, pitches)
        }
        _ => return None,
    };
    Some(Bounds {
        time: (
            time.0.saturating_sub(EXCLUSION_TIME),
            time.1.saturating_add(EXCLUSION_TIME),
        ),
        pitch: (pitch.0 - 0.25, pitch.1 + 0.25),
    })
}

fn collect(
    piece: &VisualPiece,
    interval: (i64, i64),
    bounds: Bounds,
    exclusions: &[Bounds],
    output: &mut Vec<PitchPoint>,
) {
    let input = &piece.snapshot.input;
    let track = &piece.snapshot.data.pitch;
    if !track.start_seconds.is_finite()
        || !track.frame_seconds.is_finite()
        || track.frame_seconds <= 0.0
    {
        return;
    }
    let frame_at = |position| {
        ((input.tempo.seconds_at(GlobalBlick(position)) - track.start_seconds)
            / track.frame_seconds)
            .ceil()
            .max(0.0) as usize
    };
    let from = frame_at(interval.0).saturating_sub(1);
    let to = frame_at(interval.1)
        .saturating_add(1)
        .min(track.values.len());
    let mut samples = Vec::with_capacity(SAMPLE_BATCH.min(to.saturating_sub(from)));
    for segment in &track.segments {
        // Piece boundaries, unvoiced intervals and non-finite samples must
        // remain separate; reducing across them invents contours over rests.
        for index in from.max(segment.start)..to.min(segment.end) {
            let seconds = track.start_seconds + index as f64 * track.frame_seconds;
            let position = input.tempo.position_at(seconds).0;
            if position < interval.0 || position >= interval.1 {
                continue;
            }
            let pitch = f64::from(track.values[index]) - f64::from(input.pitch_offset);
            let Some(time) = position.checked_sub(input.content_offset.0) else {
                emit(&mut samples, bounds, exclusions, output);
                continue;
            };
            if !pitch.is_finite() {
                emit(&mut samples, bounds, exclusions, output);
                continue;
            }
            if samples.last().is_some_and(|last| time <= last.time) {
                continue;
            }
            samples.push(Sample { time, pitch });
            if samples.len() == SAMPLE_BATCH {
                let last = *samples.last().expect("nonempty sample batch");
                emit(&mut samples, bounds, exclusions, output);
                samples.push(last);
            }
        }
        emit(&mut samples, bounds, exclusions, output);
    }
}

fn emit(
    samples: &mut Vec<Sample>,
    bounds: Bounds,
    exclusions: &[Bounds],
    output: &mut Vec<PitchPoint>,
) {
    if samples.is_empty() {
        return;
    }
    let mut keep = vec![false; samples.len()];
    keep[0] = true;
    keep[samples.len() - 1] = true;
    let mut pending = vec![(0, samples.len() - 1, 0)];
    while let Some((first, last, depth)) = pending.pop() {
        if first + 1 >= last {
            continue;
        }
        let mut worst = None;
        let mut error = VERTICAL_TOLERANCE;
        for index in first + 1..last {
            let residual = residual(samples[first], samples[last], samples[index]);
            if residual > error {
                worst = Some(index);
                error = residual;
            }
        }
        if let Some(index) = worst {
            if depth == MAX_DEPTH {
                // Keep the source points at the work bound; never replace an
                // unresolved oscillation with a fabricated straight contour.
                keep[first + 1..last].fill(true);
            } else {
                keep[index] = true;
                pending.push((index, last, depth + 1));
                pending.push((first, index, depth + 1));
            }
        }
    }
    for (sample, keep) in samples.iter().zip(keep) {
        if keep
            && bounds.contains(*sample)
            && !exclusions.iter().any(|excluded| excluded.contains(*sample))
        {
            output.push(PitchPoint::new(sample.time, sample.pitch));
        }
    }
    samples.clear();
}

// Reduce the curve linearly with a 0.2-semitone tolerance on the comparison
// grid; the reduction does not depend on a separate curve interpolator.
fn residual(left: Sample, right: Sample, point: Sample) -> f64 {
    let span = (i128::from(right.time) - i128::from(left.time)) as f64;
    let point_time = (i128::from(point.time) - i128::from(left.time)) as f64;
    let slope = (right.pitch - left.pitch) / span;
    let at_point = left.pitch + slope * point_time;
    if slope.abs() < f64::EPSILON {
        return (at_point - point.pitch).abs();
    }
    let step_pitch = slope * COMPARISON_STEP as f64;
    let limit = (HORIZONTAL_TOLERANCE / COMPARISON_STEP) as f64;
    let offset = ((point.pitch - at_point) / step_pitch)
        .round()
        .clamp(-limit, limit);
    // Limit sampling to this span; its endpoint values extend beyond it.
    let time = (point_time + offset * COMPARISON_STEP as f64).clamp(0.0, span);
    (left.pitch + slope * time - point.pitch).abs()
}
