use crate::ui::theme::*;
use eframe::egui::*;

pub fn draw(ui: &Ui, canvas: Rect, top_pitch: f32, row_height: f32) {
    let painter = ui
        .painter()
        .with_clip_rect(canvas.intersect(ui.clip_rect()));
    let count = (canvas.height() / row_height).ceil() as i32 + 2;
    for row in -2..count {
        let pitch = top_pitch.floor() as i32 - row;
        let white = super::keyboard::white_index(pitch);
        let y = canvas.top() + (top_pitch - pitch as f32) * row_height;
        let r = Rect::from_min_max(pos2(canvas.left(), y), pos2(canvas.right(), y + row_height));
        painter.rect_filled(r, 0, if white.is_none() { BLACK_ROW } else { ROW });
        painter.hline(
            r.x_range(),
            y,
            Stroke::new(0.5, Color32::from_rgb(43, 42, 45)),
        );
    }
}
