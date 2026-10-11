use crate::{
    app::App,
    ui::{group_geometry, track_color::TrackColors},
};
use eframe::egui::{Rect, Stroke, Ui};

/// Other tracks provide a visual reference only; hit testing still uses the
/// selected track's scene. Read project notes directly without cloning them.
pub(super) fn draw(app: &App, ui: &Ui, canvas: Rect) {
    for track in &app.editor.project.tracks {
        if track.id == app.editor.track
            || track.is_instrumental()
            || track
                .mixer
                .get("display")
                .and_then(serde_json::Value::as_bool)
                == Some(false)
        {
            continue;
        }
        let stroke = Stroke::new(1.0, TrackColors::new(track).accent.gamma_multiply(0.35));
        for reference in track.visible_references() {
            let clip = group_geometry::clip_rect(reference, canvas, &app.view.piano)
                .intersect(ui.clip_rect());
            if !clip.is_positive() {
                continue;
            }
            let Some(group) = app.editor.project.group(&reference.group_id) else {
                continue;
            };
            let painter = ui.painter().with_clip_rect(clip);
            for note in &group.notes {
                let y = canvas.top()
                    + (app.view.top_pitch - (note.pitch + reference.pitch_offset) as f32 + 0.5)
                        * app.view.row_height;
                if y < clip.top() || y > clip.bottom() {
                    continue;
                }
                if let Some(rect) = group_geometry::visible_note_rect(
                    note,
                    reference,
                    canvas,
                    &app.view.piano,
                    app.view.top_pitch,
                    app.view.row_height,
                ) {
                    painter.hline(rect.x_range(), y, stroke);
                }
            }
        }
    }
}
