use super::layout::Key;
use crate::ui::scale::ScaleGuide;
use eframe::egui::*;

const MARKER_WIDTH: f32 = 4.0;

pub(super) struct KeyboardDecoration {
    scale: ScaleGuide,
    accent: Color32,
}

impl KeyboardDecoration {
    pub fn new(scale: ScaleGuide, accent: Color32) -> Self {
        Self { scale, accent }
    }

    pub fn draw(&self, painter: &Painter, key: &Key, black: bool, rounding: CornerRadius) {
        let key_painter = painter.with_clip_rect(key.rect.intersect(painter.clip_rect()));
        if self.scale.enabled && self.scale.contains(key.pitch) {
            let marker = Rect::from_min_max(
                pos2(key.rect.right() - MARKER_WIDTH, key.rect.top()),
                key.rect.max,
            );

            key_painter
                .with_clip_rect(marker.intersect(key_painter.clip_rect()))
                .rect_filled(key.rect, rounding, self.accent);
        }
        let name = if self.scale.enabled {
            self.scale
                .is_tonic(key.pitch)
                .then(|| self.scale.tonic.short_label())
        } else {
            (key.pitch.rem_euclid(12) == 0).then_some("C")
        };
        if let Some(name) = name {
            self.label(&key_painter, key, name, black);
        }
    }

    fn label(&self, painter: &Painter, key: &Key, name: &str, black: bool) {
        let text = format!("{name}{}", key.pitch.div_euclid(12) - 1);
        let color = if black {
            Color32::from_gray(210)
        } else {
            Color32::from_rgb(68, 66, 72)
        };
        let right = key.rect.right() - if black { MARKER_WIDTH + 1.0 } else { 7.0 };
        let available = (right - key.rect.left() - 1.0).max(1.0);
        let mut galley = painter.layout_no_wrap(text.clone(), FontId::proportional(10.0), color);
        let fit = (available / galley.size().x)
            .min((key.rect.height() - 2.0).max(1.0) / galley.size().y)
            .min(1.0);
        if fit < 1.0 {
            galley = painter.layout_no_wrap(text, FontId::proportional(10.0 * fit), color);
        }
        let center_y = if black {
            key.rect.center().y
        } else {
            key.label_y
        };
        let top = (center_y - galley.size().y * 0.5).clamp(
            key.rect.top(),
            (key.rect.bottom() - galley.size().y).max(key.rect.top()),
        );
        painter.galley(pos2(right - galley.size().x, top), galley, color);
    }
}
