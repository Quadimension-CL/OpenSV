//! Voice controls shared by the singer panel.

mod numeric;
mod parameter;
mod style;

pub(super) use numeric::ControlResponse;
pub(super) use parameter::parameter_slider;
pub(super) use style::style;

use crate::ui::{icons::Icon, theme::*};
use eframe::egui::*;

const CONTROL_FILL: Color32 = Color32::from_rgb(58, 56, 59);
const STYLE_FILL: Color32 = Color32::from_rgb(50, 48, 51);

pub(super) fn field(ui: &mut Ui, value: &str, dropdown: bool, reason: &str) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    ui.painter().rect(
        rect,
        4,
        CONTROL_FILL,
        Stroke::new(0.6, BUTTON),
        StrokeKind::Inside,
    );
    let text_rect = Rect::from_min_max(
        rect.min + vec2(8.0, 0.0),
        rect.max - vec2(if dropdown { 24.0 } else { 8.0 }, 0.0),
    );
    ui.painter()
        .with_clip_rect(text_rect.intersect(ui.clip_rect()))
        .text(
            text_rect.left_center(),
            Align2::LEFT_CENTER,
            value,
            FontId::proportional(12.0),
            TEXT,
        );
    if dropdown {
        Icon::ChevronDown.paint(
            ui,
            Rect::from_center_size(rect.right_center() - vec2(17.0, 0.0), Vec2::splat(12.0)),
            MUTED,
        );
    }
    response.widget_info(|| WidgetInfo::labeled(WidgetType::ComboBox, false, value));
    response
        .on_hover_cursor(CursorIcon::NotAllowed)
        .on_hover_text(reason);
}

const POPUP_KNOB: Color32 = Color32::from_rgb(65, 63, 68);

pub(super) fn readout(ui: &mut Ui, value: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    ui.painter().rect(
        rect,
        4,
        CONTROL_FILL,
        Stroke::new(0.6, BUTTON),
        StrokeKind::Inside,
    );
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .text(
            rect.left_center() + vec2(8.0, 0.0),
            Align2::LEFT_CENTER,
            value,
            FontId::proportional(12.0),
            TEXT,
        );
}
