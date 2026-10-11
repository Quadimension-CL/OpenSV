//! Read-only overlays of completed synthesis. Painting never scans whole PCM
//! buffers or changes note interaction regions.
mod pitch;
mod waveform;
mod waveform_flash;

use std::{
    ops::Range,
    time::{Duration, Instant},
};

use super::scene::SceneGroup;
use crate::{
    app::App,
    model::{beats, ticks},
    synthesis::plan::{GlobalBlick, OwnerId, OwnerInput, TempoMapping},
    ui::timeline,
};
use eframe::egui::*;

pub fn waveforms(app: &App, ui: &Ui, canvas: Rect, groups: &[SceneGroup]) {
    if !app.show_waveform {
        return;
    }
    let now = Instant::now();
    let mut animating = false;
    for group in groups {
        let Some(visuals) = app.synthesis.visuals(OwnerId(group.reference.id)) else {
            continue;
        };
        for piece in visuals.pieces() {
            let painter = painter(ui, canvas, app, &piece.snapshot.input, &piece.range);
            if painter.clip_rect().is_positive() {
                animating |=
                    waveform::draw(app, &painter, canvas, group, &piece.snapshot, visuals, now);
            }
        }
    }
    if animating {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

pub fn pitch(app: &App, ui: &Ui, canvas: Rect, groups: &[SceneGroup]) {
    if !app.show_pitch {
        return;
    }
    for group in groups {
        let Some(visuals) = app.synthesis.visuals(OwnerId(group.reference.id)) else {
            continue;
        };
        for piece in visuals.pieces() {
            let painter = painter(ui, canvas, app, &piece.snapshot.input, &piece.range);
            if painter.clip_rect().is_positive() {
                pitch::draw(app, &painter, canvas, group, &piece.snapshot);
            }
        }
    }
}

fn painter(ui: &Ui, canvas: Rect, app: &App, input: &OwnerInput, range: &Range<i64>) -> Painter {
    // A completed block retains its own placement while later blocks are still
    // rendering. In particular, moving notes must not move their old overlays.
    let left = timeline::x_at(
        canvas,
        &app.view.piano,
        beats(input.visible_begin.0.max(range.start)),
    );
    let right = input
        .visible_end
        .map_or(canvas.right(), |end| {
            timeline::x_at(canvas, &app.view.piano, beats(end.0))
        })
        .min(timeline::x_at(canvas, &app.view.piano, beats(range.end)));
    let bounds = Rect::from_min_max(pos2(left, canvas.top()), pos2(right, canvas.bottom()));
    ui.painter()
        .with_clip_rect(bounds.intersect(canvas).intersect(ui.clip_rect()))
}

fn position_at_x(app: &App, canvas: Rect, x: f32) -> i64 {
    ticks(timeline::beat_at(canvas, &app.view.piano, x))
}

fn seconds_at_x(app: &App, tempo: &TempoMapping, canvas: Rect, x: f32) -> f64 {
    tempo.seconds_at(GlobalBlick(ticks(timeline::beat_at(
        canvas,
        &app.view.piano,
        x,
    ))))
}

fn x_at_seconds(app: &App, tempo: &TempoMapping, canvas: Rect, seconds: f64) -> f32 {
    timeline::x_at(canvas, &app.view.piano, beats(tempo.position_at(seconds).0))
}

fn pitch_y(app: &App, canvas: Rect, midi: f32) -> f32 {
    canvas.top() + (app.view.top_pitch - midi + 0.5) * app.view.row_height
}
