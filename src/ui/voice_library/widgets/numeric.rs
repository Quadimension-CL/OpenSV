mod pointer;

use crate::ui::theme::*;
use eframe::egui::{
    text::{CCursor, CCursorRange},
    *,
};

#[derive(Clone, Copy, Default)]
pub(crate) struct ControlResponse {
    pub changed: bool,
    pub active: bool,
    pub finished: bool,
    pub cancelled: bool,
}

impl ControlResponse {
    pub(super) fn merge(&mut self, other: Self) {
        self.changed |= other.changed;
        self.active |= other.active;
        self.finished |= other.finished;
        self.cancelled |= other.cancelled;
    }
}

#[derive(Clone, Default)]
struct State {
    frame: u64,
    drag: Option<Drag>,
    text: Option<Text>,
}

#[derive(Clone)]
struct Drag {
    origin: Pos2,
    values: Vec<f64>,
}

#[derive(Clone)]
struct Text {
    value: String,
    focus: bool,
    invalid: bool,
}

pub(super) struct Spec<'a> {
    pub range: (f64, f64),
    pub step: f64,
    pub fine_step: f64,
    pub decimals: usize,
    pub unit: &'a str,
    pub snap_input: bool,
}

pub(super) struct Geometry {
    pub pointer: Rect,
    pub number: Rect,
    pub vertical: bool,
    pub centered: bool,
}

pub(super) fn mean(values: &[f64], enabled: &[bool]) -> f64 {
    let mut sum = 0.0;
    let mut count = 0;
    for (&value, &enabled) in values.iter().zip(enabled) {
        if enabled {
            sum += value;
            count += 1;
        }
    }
    if count == 0 { 0.0 } else { sum / count as f64 }
}

pub(super) fn control(
    ui: &mut Ui,
    id: Id,
    values: &mut [f64],
    enabled: &[bool],
    geometry: Geometry,
    spec: Spec<'_>,
) -> ControlResponse {
    let state_id = id.with("state");
    let mut state = ui.data_mut(|data| data.get_temp::<State>(state_id).unwrap_or_default());
    let before = values.to_vec();
    let mut result = ControlResponse::default();
    let frame = ui.ctx().cumulative_frame_nr();
    if state.frame.saturating_add(1) < frame || !ui.input(|input| input.focused) {
        result.cancelled = state.drag.is_some() || state.text.is_some();
        state = State::default();
    }
    let response = ui.interact(geometry.pointer, id, Sense::click_and_drag());
    let number = ui.interact(geometry.number, id.with("number"), Sense::click());
    let enabled_here = ui.is_enabled() && enabled.iter().any(|&enabled| enabled);
    if enabled_here {
        if number.double_clicked() && state.text.is_none() {
            state.text = Some(Text {
                value: format!("{:.*}", spec.decimals, mean(values, enabled)),
                focus: true,
                invalid: false,
            });
        }
        if state.text.is_none() {
            pointer::pointer(
                ui,
                &response,
                values,
                enabled,
                &geometry,
                &spec,
                &mut state,
                &mut result,
            );
        }
        if response.secondary_clicked() || number.secondary_clicked() {
            assign(values, enabled, 0.0_f64.clamp(spec.range.0, spec.range.1));
            state = State::default();
            result.finished = true;
        }
    } else {
        result.cancelled |= state.drag.is_some() || state.text.is_some();
        state = State::default();
    }
    if let Some(mut text) = state.text.take() {
        let input = ui.put(
            geometry.number,
            TextEdit::singleline(&mut text.value)
                .id(id.with("input"))
                .font(FontId::proportional(12.0))
                .horizontal_align(if geometry.centered {
                    Align::Center
                } else {
                    Align::Max
                })
                .vertical_align(Align::Center)
                .margin(vec2(3.0, 0.0)),
        );
        if text.focus {
            input.request_focus();
            if let Some(mut edit) = TextEdit::load_state(ui.ctx(), input.id) {
                edit.cursor.set_char_range(Some(CCursorRange::two(
                    CCursor::new(0),
                    CCursor::new(text.value.chars().count()),
                )));
                edit.store(ui.ctx(), input.id);
            }
            text.focus = false;
        }
        if input.changed() {
            text.invalid = false;
        }
        let escape = ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
        let enter = ui.input(|input| input.key_pressed(Key::Enter));
        if escape {
            input.surrender_focus();
            result.cancelled = true;
        } else if enter || input.lost_focus() {
            if let Ok(value) = text.value.trim().parse::<f64>()
                && value.is_finite()
            {
                let value = if spec.snap_input {
                    (value / spec.step).round() * spec.step
                } else {
                    value
                };
                assign(values, enabled, value.clamp(spec.range.0, spec.range.1));
                input.surrender_focus();
                result.finished = true;
            } else if enter {
                text.invalid = true;
                input.request_focus();
                state.text = Some(text);
            } else {
                result.cancelled = true;
            }
        } else {
            state.text = Some(text);
        }
        if state.text.as_ref().is_some_and(|text| text.invalid) {
            ui.painter().rect_stroke(
                geometry.number,
                2,
                Stroke::new(1.0, Color32::from_rgb(218, 142, 126)),
                StrokeKind::Inside,
            );
        }
    } else if response.hovered() || number.hovered() || state.drag.is_some() {
        let text = format!("{:.*}{}", spec.decimals, mean(values, enabled), spec.unit);
        let (position, align) = if geometry.centered {
            (geometry.number.center(), Align2::CENTER_CENTER)
        } else {
            (geometry.number.right_center(), Align2::RIGHT_CENTER)
        };
        ui.painter()
            .text(position, align, text, FontId::proportional(12.0), TEXT);
    }
    result.changed = before != values;
    result.active = state.drag.is_some() || state.text.is_some();
    response.widget_info(|| WidgetInfo::slider(enabled_here, mean(values, enabled), ""));
    response.on_hover_cursor(if enabled_here {
        CursorIcon::ResizeHorizontal
    } else {
        CursorIcon::NotAllowed
    });
    number
        .on_hover_cursor(if enabled_here {
            CursorIcon::Text
        } else {
            CursorIcon::NotAllowed
        })
        .on_hover_text("双击输入数值；右键恢复默认值");
    state.frame = frame;
    ui.data_mut(|data| data.insert_temp(state_id, state));
    result
}

fn assign(values: &mut [f64], enabled: &[bool], value: f64) {
    for (target, enabled) in values.iter_mut().zip(enabled) {
        if *enabled {
            *target = value;
        }
    }
}
