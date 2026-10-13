use super::{ControlResponse, Drag, Geometry, Spec, State, assign, mean};
use eframe::egui::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn pointer(
    ui: &Ui,
    response: &Response,
    values: &mut [f64],
    enabled: &[bool],
    geometry: &Geometry,
    spec: &Spec<'_>,
    state: &mut State,
    result: &mut ControlResponse,
) {
    if response.drag_started() {
        let origin = ui.input(|input| input.pointer.press_origin());
        if let Some(origin) = origin {
            state.drag = Some(Drag {
                origin,
                values: values.to_vec(),
            });
        }
        response.request_focus();
    }
    if state.drag.is_some() && ui.input(|input| input.key_pressed(Key::Escape)) {
        values.copy_from_slice(&state.drag.take().unwrap().values);
        ui.ctx().stop_dragging();
        result.cancelled = true;
        return;
    }
    let fine = ui.input(|input| input.modifiers.shift);
    let step = if fine { spec.fine_step } else { spec.step };
    if let Some(position) = response.interact_pointer_pos() {
        if let Some(drag) = &state.drag {
            let origin = mean(&drag.values, enabled);
            let delta = if geometry.vertical {
                (drag.origin.y - position.y) as f64 * if fine { 0.2 } else { 1.0 }
            } else if fine {
                (position.x - drag.origin.x) as f64 / geometry.pointer.width() as f64
                    * (spec.range.1 - spec.range.0)
                    * 0.1
            } else {
                absolute(position.x, geometry.pointer, spec.range) - origin
            };
            let delta = (delta / step).round() * step;
            for ((value, initial), enabled) in values.iter_mut().zip(&drag.values).zip(enabled) {
                if *enabled {
                    *value = (initial + delta).clamp(spec.range.0, spec.range.1);
                }
            }
        } else if response.clicked() && !geometry.vertical {
            let value = (absolute(position.x, geometry.pointer, spec.range) / step).round() * step;
            assign(values, enabled, value.clamp(spec.range.0, spec.range.1));
            response.request_focus();
            result.finished = true;
        }
    }
    if response.has_focus() {
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                response.id,
                EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            )
        });
        let delta = ui.input(|input| {
            i32::from(input.key_pressed(Key::ArrowUp) || input.key_pressed(Key::ArrowRight))
                - i32::from(input.key_pressed(Key::ArrowDown) || input.key_pressed(Key::ArrowLeft))
        }) as f64
            * step;
        if delta != 0.0 {
            for (value, enabled) in values.iter_mut().zip(enabled) {
                if *enabled {
                    *value = (*value + delta).clamp(spec.range.0, spec.range.1);
                }
            }
            result.finished = true;
        }
        if ui.input(|input| input.key_pressed(Key::Home)) {
            assign(values, enabled, 0.0_f64.clamp(spec.range.0, spec.range.1));
            result.finished = true;
        }
    }
    if state.drag.is_some()
        && (response.drag_stopped() || ui.input(|input| !input.pointer.primary_down()))
    {
        state.drag = None;
        result.finished = true;
    }
}

fn absolute(x: f32, rect: Rect, range: (f64, f64)) -> f64 {
    range.0 + ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * (range.1 - range.0)
}
