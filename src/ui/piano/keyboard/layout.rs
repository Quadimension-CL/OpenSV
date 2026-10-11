use crate::ui::theme::{BLACK_ROW, GREEN};
use eframe::egui::*;

const WHITE_KEY: Color32 = Color32::from_rgb(147, 145, 155);

pub(in crate::ui::piano) fn white_index(pitch: i32) -> Option<i32> {
    match pitch.rem_euclid(12) {
        0 => Some(0),
        2 => Some(1),
        4 => Some(2),
        5 => Some(3),
        7 => Some(4),
        9 => Some(5),
        11 => Some(6),
        _ => None,
    }
}

struct Key {
    pitch: i32,
    rect: Rect,
    label_y: f32,
}

pub(super) struct KeyboardLayout {
    pub rect: Rect,
    white: Vec<Key>,
    black: Vec<Key>,
}

impl KeyboardLayout {
    pub fn new(all: Rect, canvas: Rect, top_pitch: f32, row_height: f32) -> Self {
        let rect = Rect::from_min_max(
            pos2(all.left(), canvas.top()),
            pos2(canvas.left(), canvas.bottom()),
        );
        let mut keyboard = Self {
            rect,
            white: Vec::new(),
            black: Vec::new(),
        };
        let white_height = row_height * 12.0 / 7.0;
        let count = (canvas.height() / row_height).ceil() as i32 + 2;
        for row in -2..count {
            let pitch = top_pitch.floor() as i32 - row;
            let y = canvas.top() + (top_pitch - pitch as f32) * row_height;
            let index = white_index(pitch);
            let key = if let Some(index) = index {
                // Equal-height white keys share their octave anchor for both
                // painting and audition, and meet the canvas without a gap.
                let octave_c = pitch.div_euclid(12) * 12;
                let bottom = canvas.top() + (top_pitch - octave_c as f32 + 1.0) * row_height
                    - index as f32 * white_height;
                Rect::from_min_max(
                    pos2(all.left(), bottom - white_height),
                    pos2(canvas.left(), bottom),
                )
            } else {
                Rect::from_min_size(pos2(all.left(), y + 0.5), vec2(22.0, row_height - 1.0))
            };
            let key = Key {
                pitch,
                rect: key,
                label_y: y + row_height * 0.5,
            };
            if index.is_some() {
                keyboard.white.push(key);
            } else {
                keyboard.black.push(key);
            }
        }
        keyboard
    }

    pub fn hit(&self, point: Pos2) -> Option<u8> {
        if !self.rect.contains(point) {
            return None;
        }
        self.black
            .iter()
            .chain(&self.white)
            .find(|key| key.rect.contains(point))
            .filter(|key| (0..=127).contains(&key.pitch))
            .map(|key| key.pitch as u8)
    }

    pub fn draw(&self, ui: &Ui, active: Option<u8>) {
        let painter = ui
            .painter()
            .with_clip_rect(self.rect.intersect(ui.clip_rect()));
        painter.rect_filled(self.rect, 0, WHITE_KEY);
        let fill = |key: &Key, normal| {
            if active.is_some_and(|pitch| i32::from(pitch) == key.pitch) {
                GREEN
            } else {
                normal
            }
        };
        for key in &self.white {
            painter.rect_filled(key.rect, 2, fill(key, WHITE_KEY));
            painter.rect_stroke(
                key.rect,
                2,
                Stroke::new(0.6, Color32::from_rgb(79, 77, 84)),
                StrokeKind::Inside,
            );
            if key.pitch.rem_euclid(12) == 0 {
                painter.text(
                    pos2(key.rect.right() - 7.0, key.label_y),
                    Align2::RIGHT_CENTER,
                    format!("C{}", key.pitch.div_euclid(12) - 1),
                    FontId::proportional(10.0),
                    Color32::from_rgb(68, 66, 72),
                );
            }
        }
        for key in &self.black {
            painter.rect_filled(
                key.rect,
                CornerRadius {
                    nw: 0,
                    ne: 2,
                    sw: 0,
                    se: 2,
                },
                fill(key, BLACK_ROW),
            );
        }
    }
}
