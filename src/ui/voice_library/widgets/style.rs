use super::{
    POPUP_KNOB, STYLE_FILL,
    numeric::{self, ControlResponse, Geometry, Spec},
};
use crate::ui::theme::*;
use eframe::egui::*;

pub(crate) fn style(
    ui: &mut Ui,
    name: &str,
    values: &mut [f64; 3],
    enabled_components: [bool; 3],
) -> ControlResponse {
    let id = ui.make_persistent_id(("vocal-mode", name));
    let expanded_id = id.with("expanded");
    let expanded = ui.data_mut(|data| data.get_temp::<bool>(expanded_id).unwrap_or(false));
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 100.0), Sense::hover());
    ui.painter().rect_filled(rect, 3, STYLE_FILL);
    ui.painter().text(
        rect.min + vec2(12.0, 10.0),
        Align2::LEFT_TOP,
        name,
        FontId::proportional(13.0),
        MUTED,
    );
    let toggle_rect = Rect::from_center_size(
        pos2(rect.right() - 17.0, rect.top() + 18.0),
        Vec2::splat(24.0),
    );
    let toggle = ui.interact(toggle_rect, id.with("expand"), Sense::click());
    let color = if toggle.hovered() { TEXT } else { MUTED };
    let center = toggle_rect.center();
    ui.painter().line_segment(
        [center - vec2(5.0, 0.0), center + vec2(5.0, 0.0)],
        Stroke::new(1.3, color),
    );
    if !expanded {
        ui.painter().line_segment(
            [center - vec2(0.0, 5.0), center + vec2(0.0, 5.0)],
            Stroke::new(1.3, color),
        );
    }
    if toggle.clicked() {
        ui.data_mut(|data| data.insert_temp(expanded_id, !expanded));
        ui.ctx().request_repaint();
    }
    toggle
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(if expanded {
            "收起独立唱法参数"
        } else {
            "分别调整音高、音色与发音"
        });
    if expanded {
        return components(ui, id, rect, values, enabled_components);
    }
    let center = pos2(rect.center().x, rect.top() + 57.0);
    let result = numeric::control(
        ui,
        id.with("master"),
        values,
        &enabled_components,
        Geometry {
            pointer: Rect::from_center_size(center, Vec2::splat(66.0)),
            number: Rect::from_center_size(pos2(center.x, rect.top() + 89.0), vec2(80.0, 20.0)),
            vertical: true,
            centered: true,
        },
        percentage(),
    );
    for ((value, enabled), radius) in values
        .iter()
        .zip(enabled_components)
        .zip([20.0, 25.0, 30.0])
    {
        arc(ui.painter(), center, radius, 1.0, BUTTON);
        if enabled && *value > 0.0 {
            arc(ui.painter(), center, radius, (*value / 150.0) as f32, MUTED);
        }
    }
    dial(
        ui.painter(),
        center,
        16.0,
        numeric::mean(values, &enabled_components),
        TEXT,
    );
    result
}

fn components(
    ui: &mut Ui,
    id: Id,
    rect: Rect,
    values: &mut [f64; 3],
    enabled: [bool; 3],
) -> ControlResponse {
    let mut result = ControlResponse::default();
    let width = (rect.width() - 12.0) / 3.0;
    for (index, label) in ["音高", "音色", "发音"].iter().enumerate() {
        let center = pos2(
            rect.left() + 6.0 + width * (index as f32 + 0.5),
            rect.top() + 48.0,
        );
        let pointer = Rect::from_center_size(center, vec2(width.min(40.0), 36.0));
        let number = Rect::from_center_size(
            pos2(center.x, rect.top() + 73.0),
            vec2((width - 4.0).min(70.0), 14.0),
        );
        let response = numeric::control(
            ui,
            id.with(index),
            std::slice::from_mut(&mut values[index]),
            &[enabled[index]],
            Geometry {
                pointer,
                number,
                vertical: true,
                centered: true,
            },
            percentage(),
        );
        arc(ui.painter(), center, 16.0, 1.0, BUTTON);
        if enabled[index] && values[index] > 0.0 {
            arc(
                ui.painter(),
                center,
                16.0,
                (values[index] / 150.0) as f32,
                MUTED,
            );
        }
        dial(
            ui.painter(),
            center,
            12.0,
            values[index],
            if enabled[index] { TEXT } else { MUTED },
        );
        ui.painter().text(
            pos2(center.x, rect.top() + 89.0),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(12.0),
            MUTED,
        );
        if !enabled[index] {
            ui.interact(
                pointer.union(number),
                id.with(("disabled", index)),
                Sense::hover(),
            )
            .on_hover_text("此声库引擎暂不支持独立调整该参数");
        }
        result.merge(response);
    }
    result
}

fn dial(painter: &Painter, center: Pos2, radius: f32, value: f64, color: Color32) {
    painter.circle(center, radius, POPUP_KNOB, Stroke::new(0.8, BUTTON_ACTIVE));
    let angle = (135.0 + (value / 150.0) as f32 * 270.0).to_radians();
    painter.line_segment(
        [
            center,
            center + vec2(angle.cos(), angle.sin()) * (radius - 2.0),
        ],
        Stroke::new(2.2, color),
    );
}

fn percentage() -> Spec<'static> {
    Spec {
        range: (0.0, 150.0),
        step: 5.0,
        fine_step: 1.0,
        decimals: 0,
        unit: "%",
        snap_input: false,
    }
}

fn arc(painter: &Painter, center: Pos2, radius: f32, amount: f32, color: Color32) {
    let points: Vec<_> = (0..=60)
        .map(|step| {
            let angle = (135.0 + step as f32 / 60.0 * 270.0 * amount.clamp(0.0, 1.0)).to_radians();
            center + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    painter.add(Shape::line(points, Stroke::new(2.0, color)));
}
