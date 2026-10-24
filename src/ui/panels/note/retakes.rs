use crate::{app::App, model::RetakeComponent, ui::retakes};
use eframe::egui::*;

pub(super) fn show(app: &mut App, ui: &mut Ui) {
    let reason = retakes::unavailable(app, RetakeComponent::All);
    let duration = retakes::unavailable(app, RetakeComponent::Duration);
    let clicked = ui
        .add_enabled_ui(reason.is_none(), |ui| {
            super::retake_buttons::show(ui, duration)
        })
        .inner;
    if let Some(component) = clicked {
        retakes::create(app, component);
    }
    ui.add_space(8.0);
    match app.editor.retake_selection() {
        Ok(Some(selection)) => retakes::selector(app, ui, &selection),
        Ok(None) => {
            ui.add_enabled(
                false,
                Button::new(crate::i18n::t("默认采样")).min_size(vec2(ui.available_width(), 28.0)),
            );
        }
        Err(error) => {
            ui.weak(crate::i18n::message(&error));
        }
    }
    if let Some(reason) = reason {
        ui.add_space(6.0);
        ui.weak(crate::i18n::message(&reason));
    }
}
