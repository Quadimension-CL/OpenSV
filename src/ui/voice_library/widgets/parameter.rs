use super::{
    POPUP_KNOB,
    numeric::{self, ControlResponse, Geometry, Spec},
};
use crate::{model::VoiceParameter, ui::theme::*};
use eframe::egui::*;

pub(crate) fn parameter_slider(
    ui: &mut Ui,
    parameter: VoiceParameter,
    value: &mut f64,
) -> ControlResponse {
    let step = parameter.step();
    let tuning = VoiceParameter::TUNING.contains(&parameter);
    let fine_step = if tuning { step } else { step / 10.0 };
    let decimals = (-fine_step.log10()).ceil().clamp(0.0, 6.0) as usize;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 51.0), Sense::hover());
    slider_at(
        ui,
        ui.make_persistent_id(parameter.key()),
        rect,
        parameter.label(),
        value,
        Spec {
            range: parameter.range(),
            step,
            fine_step,
            decimals,
            unit: parameter.unit(),
            snap_input: tuning,
        },
    )
}

fn slider_at(
    ui: &mut Ui,
    id: Id,
    rect: Rect,
    label: &str,
    value: &mut f64,
    spec: Spec<'_>,
) -> ControlResponse {
    let bounds = Rect::from_min_max(
        pos2(rect.left() + 9.5, rect.top() + 22.0),
        pos2(rect.right() - 9.5, rect.top() + 42.0),
    );
    let range = spec.range;
    let result = numeric::control(
        ui,
        id,
        std::slice::from_mut(value),
        &[true],
        Geometry {
            pointer: bounds,
            number: Rect::from_min_max(
                pos2(rect.center().x, rect.top()),
                pos2(rect.right(), rect.top() + 20.0),
            ),
            vertical: false,
            centered: false,
        },
        spec,
    );
    let amount = ((*value - range.0) / (range.1 - range.0)).clamp(0.0, 1.0) as f32;
    let center = pos2(lerp(bounds.x_range(), amount), rect.top() + 32.0);
    let painter = ui.painter();
    painter.text(
        rect.min + vec2(0.0, 4.0),
        Align2::LEFT_TOP,
        label,
        FontId::proportional(12.0),
        MUTED,
    );
    painter.hline(bounds.x_range(), center.y, Stroke::new(2.0, EDGE));
    painter.hline(bounds.left()..=center.x, center.y, Stroke::new(2.0, MUTED));
    painter.circle(center, 9.5, EDGE, Stroke::NONE);
    painter.circle(center, 8.5, POPUP_KNOB, Stroke::new(0.7, BUTTON_ACTIVE));
    painter.circle_filled(center, 3.0, TEXT);
    result
}
