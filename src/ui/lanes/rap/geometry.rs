use eframe::egui::*;

pub(super) fn tone_level(tone: f32) -> f32 {
    let level = (tone + 1.0) * if tone < -1.0 { 0.25 } else { 0.5 };
    f64::from(level).clamp(-0.95, 0.95) as f32
}
pub(super) fn tone_y(area: Rect, tone: f32) -> f32 {
    area.top() + ((1.0 - f64::from(tone_level(tone))) * f64::from(area.height()) * 0.5) as f32
}
pub(super) fn normalized_y(area: Rect, y: f32) -> f32 {
    (1.0 - f64::from(((y - area.top()) * 2.0) / area.height())) as f32
}
pub(super) fn drag_tone(area: Rect, tone: f32, origin_y: f32, y: f32) -> f32 {
    let level = (normalized_y(area, y) + tone_level(tone)) - normalized_y(area, origin_y);
    (level * if level > 0.0 { 2.0 } else { 4.0 } - 1.0).clamp(-5.0, 1.0)
}
pub(super) fn point(area: Rect, left: f32, right: f32, tone: f32, intonation: f32, t: f32) -> Pos2 {
    pos2(
        left + (right - left) * t,
        tone_y(area, tone) - area.height() * 0.5 * intonation * t * t,
    )
}
pub(super) fn end(area: Rect, tone: f32, intonation: f32) -> f32 {
    let y = tone_y(area, tone);
    let delta = -area.height() * 0.5 * intonation;
    if delta > 0.0 {
        ((area.bottom() - y) / delta).sqrt().min(1.0)
    } else if delta < 0.0 {
        ((area.top() - y) / delta).sqrt().min(1.0)
    } else {
        1.0
    }
}
