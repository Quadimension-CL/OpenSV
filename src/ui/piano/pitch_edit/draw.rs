use crate::{
    app::App,
    model::{PitchControl, beats, ticks},
    ui::{
        group_geometry::clip_rect,
        piano::scene::SceneGroup,
        timeline::{beat_at, x_at},
        track_color::TrackColors,
    },
};
use eframe::egui::*;

pub(in crate::ui::piano) fn draw(app: &App, ui: &Ui, canvas: Rect, groups: &[SceneGroup]) {
    if !app.show_pitch {
        return;
    }
    let colors = TrackColors::new(app.editor.current_track());
    for group in groups {
        let reference = &group.reference;
        let Some(model) = app.editor.project.group(&reference.group_id) else {
            continue;
        };
        let clip = clip_rect(reference, canvas, &app.view.piano).intersect(ui.clip_rect());
        if !clip.is_positive() {
            continue;
        }
        let painter = ui.painter().with_clip_rect(clip);
        let left = ticks(beat_at(canvas, &app.view.piano, clip.left()))
            .saturating_sub(reference.blick_offset);
        let right = ticks(beat_at(canvas, &app.view.piano, clip.right()))
            .saturating_sub(reference.blick_offset);
        let point = |time: i64, pitch: f64| {
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
        };
        let active = app.editor.reference == Some(reference.id);
        let stroke = Stroke::new(
            1.5,
            if active {
                colors.accent
            } else {
                colors.accent.gamma_multiply(0.5)
            },
        );
        for control in &model.pitch_controls {
            let PitchControl::Curve(curve) = control else {
                continue;
            };
            let Some((start, end)) = curve.range() else {
                continue;
            };
            let start = start.max(left);
            let end = end.min(right);
            if start >= end {
                continue;
            }
            let mut points = Vec::new();
            if let Some(pitch) = curve.sample(start as f64) {
                points.push(point(start, pitch));
            }
            points.extend(
                curve
                    .absolute_points()
                    .filter(|(time, _)| *time > start && *time < end)
                    .map(|(time, pitch)| point(time, pitch)),
            );
            if let Some(pitch) = curve.sample(end as f64) {
                points.push(point(end, pitch));
            }
            if points.len() >= 2 {
                painter.add(Shape::line(points, stroke));
            }
        }
    }
}
