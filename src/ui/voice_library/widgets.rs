//! Read-only controls for synthesis options that the engine does not yet implement.

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

pub(super) fn style(ui: &mut Ui, name: &str, value: f32) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 100.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
        painter.rect_filled(rect, 0, STYLE_FILL);
        painter.text(
            rect.min + vec2(12.0, 10.0),
            Align2::LEFT_TOP,
            name,
            FontId::proportional(13.0),
            MUTED,
        );
        Icon::Plus.paint(
            ui,
            Rect::from_center_size(
                pos2(rect.right() - 17.0, rect.top() + 18.0),
                Vec2::splat(14.0),
            ),
            MUTED,
        );
        let center = pos2(rect.center().x, rect.top() + 57.0);
        for radius in [20.0, 25.0, 30.0] {
            let points: Vec<_> = (0..=60)
                .map(|step| {
                    let angle = (135.0 + step as f32 / 60.0 * 270.0).to_radians();
                    center + vec2(angle.cos(), angle.sin()) * radius
                })
                .collect();
            painter.add(Shape::line(points, Stroke::new(2.0, BUTTON)));
        }
        painter.circle(center, 16.0, POPUP_KNOB, Stroke::new(0.8, BUTTON_ACTIVE));
        let angle = (135.0 + value.clamp(0.0, 1.0) * 270.0).to_radians();
        painter.line_segment(
            [center, center + vec2(angle.cos(), angle.sin()) * 14.0],
            Stroke::new(2.2, TEXT),
        );
    }
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, false, name));
    response
        .on_hover_cursor(CursorIcon::NotAllowed)
        .on_hover_text(format!(
            "{name} · {:.0}%\n唱法调节尚未接入合成",
            value * 100.0
        ));
}

const POPUP_KNOB: Color32 = Color32::from_rgb(65, 63, 68);

pub(super) fn slider(ui: &mut Ui, label: &str) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 51.0), Sense::hover());
    let painter = ui.painter();
    painter.text(
        rect.min + vec2(0.0, 4.0),
        Align2::LEFT_TOP,
        label,
        FontId::proportional(12.0),
        MUTED,
    );
    let y = rect.top() + 32.0;
    painter.hline(rect.x_range(), y, Stroke::new(2.0, EDGE));
    let center = pos2(rect.center().x, y);
    painter.circle(center, 9.5, EDGE, Stroke::NONE);
    painter.circle(center, 8.5, POPUP_KNOB, Stroke::new(0.7, BUTTON_ACTIVE));
    painter.circle_filled(center, 3.0, TEXT);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, false, label));
    response
        .on_hover_cursor(CursorIcon::NotAllowed)
        .on_hover_text(format!("{label}调节尚未接入合成"));
}
