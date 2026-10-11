use super::{editable, id, transform::Bounds};
use crate::{
    app::App,
    model::{GroupRef, PitchControl, beats, ticks},
    ui::{
        group_geometry::clip_rect,
        timeline::{beat_at, x_at},
    },
};
use eframe::egui::*;

pub(super) fn screen(app: &App, canvas: Rect, reference: &GroupRef, time: i64, pitch: f64) -> Pos2 {
    pos2(
        x_at(
            canvas,
            &app.view.piano,
            beats(time.saturating_add(reference.blick_offset)),
        ),
        canvas.top()
            + (app.view.top_pitch + 0.5 - (pitch + reference.pitch_offset as f64) as f32)
                * app.view.row_height,
    )
}

pub(super) fn sample(app: &App, canvas: Rect, reference: &GroupRef, pos: Pos2) -> (i64, f64) {
    let pos = clip_rect(reference, canvas, &app.view.piano).clamp(pos);
    let time = ticks(beat_at(canvas, &app.view.piano, pos.x))
        .clamp(reference.blick_absolute_begin, reference.visible_end())
        .saturating_sub(reference.blick_offset);
    let pitch = (app.view.top_pitch + 0.5 - (pos.y - canvas.top()) / app.view.row_height)
        .clamp(0.0, 127.0) as f64
        - reference.pitch_offset as f64;
    (time, pitch)
}

pub(super) fn rect(app: &App, canvas: Rect, reference: &GroupRef, bounds: Bounds) -> Rect {
    Rect::from_two_pos(
        screen(app, canvas, reference, bounds.left, bounds.top),
        screen(app, canvas, reference, bounds.right, bounds.bottom),
    )
    .expand(7.0)
}

#[derive(Clone, Copy)]
pub(super) enum Handle {
    Left,
    Right,
    Top,
    Bottom,
}

pub(super) fn handles(rect: Rect) -> Vec<(Handle, Rect)> {
    [
        (
            Handle::Left,
            Rect::from_center_size(rect.left_center(), vec2(4.0, 14.0)),
        ),
        (
            Handle::Right,
            Rect::from_center_size(rect.right_center(), vec2(4.0, 14.0)),
        ),
        (
            Handle::Top,
            Rect::from_center_size(rect.center_top(), vec2(14.0, 4.0)),
        ),
        (
            Handle::Bottom,
            Rect::from_center_size(rect.center_bottom(), vec2(14.0, 4.0)),
        ),
    ]
    .into_iter()
    .filter(|(handle, _)| match handle {
        Handle::Left | Handle::Right => rect.width() > 14.01,
        Handle::Top | Handle::Bottom => rect.height() > 14.01,
    })
    .collect()
}

pub(super) fn selected_rect(app: &App, canvas: Rect, reference: &GroupRef) -> Option<Rect> {
    let bounds = super::transform::bounds(&super::controls(app), &app.piano_anchor.ids)?;
    Some(rect(app, canvas, reference, bounds))
}

fn distance(pos: Pos2, left: Pos2, right: Pos2) -> f32 {
    let edge = right - left;
    let along = ((pos - left).dot(edge) / edge.length_sq().max(f32::EPSILON)).clamp(0.0, 1.0);
    pos.distance(left + edge * along)
}

pub(super) fn hit(app: &App, canvas: Rect, reference: &GroupRef, pos: Pos2) -> Option<String> {
    if !clip_rect(reference, canvas, &app.view.piano).contains(pos) {
        return None;
    }
    let controls = super::controls(app);
    // A point is more precise than the curve underneath it.
    for control in controls
        .iter()
        .rev()
        .filter(|control| editable(control, reference))
    {
        if let PitchControl::Point(point) = control
            && screen(app, canvas, reference, point.pos, point.pitch).distance(pos) <= 7.0
        {
            return Some(point.id.clone());
        }
    }
    controls
        .iter()
        .rev()
        .filter(|control| editable(control, reference))
        .find_map(|control| {
            let PitchControl::Curve(curve) = control else {
                return None;
            };
            let mut previous = None;
            for (time, pitch) in curve.absolute_points() {
                let point = screen(app, canvas, reference, time, pitch);
                if previous.is_some_and(|previous| distance(pos, previous, point) <= 5.0) {
                    return id(control).map(str::to_owned);
                }
                previous = Some(point);
            }
            None
        })
}

pub(super) fn intersects(
    app: &App,
    canvas: Rect,
    reference: &GroupRef,
    control: &PitchControl,
    rect: Rect,
) -> bool {
    if !editable(control, reference) {
        return false;
    }
    match control {
        PitchControl::Point(point) => {
            rect.contains(screen(app, canvas, reference, point.pos, point.pitch))
        }
        PitchControl::Curve(curve) => {
            let mut previous = None;
            for (time, pitch) in curve.absolute_points() {
                let point = screen(app, canvas, reference, time, pitch);
                if rect.contains(point)
                    || previous.is_some_and(|previous| {
                        rect.intersects(Rect::from_two_pos(previous, point))
                            && rect.intersects_ray(previous, point - previous)
                    })
                {
                    return true;
                }
                previous = Some(point);
            }
            false
        }
        PitchControl::Unsupported(_) => false,
    }
}
