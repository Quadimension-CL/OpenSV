mod clipboard;
mod commands;
mod geometry;
mod gesture;
mod header;
mod paint;
mod session;

pub use commands::{
    parameter_active, parameter_can_paste, parameter_clear, parameter_copy, parameter_cut,
    parameter_deactivate, parameter_delete, parameter_duplicate, parameter_has_selection,
    parameter_paste, parameter_paste_text, parameter_select_all,
};
pub use gesture::ParameterGesture;
use session::{Owner, active_lane, editable, editable_bounds, focus, reconcile};
pub use session::{ParameterSelection, parameter_keyboard};

use super::{
    child,
    piano::Gesture,
    state::{Lane, ParameterTool},
    theme::*,
    timeline::*,
};
use crate::{app::App, model::*};
use eframe::egui::*;
use geometry::Mapping;
use std::collections::BTreeSet;

pub fn show(app: &mut App, ui: &mut Ui, id: u64) {
    let Some(index) = app.view.lanes.iter().position(|lane| lane.id == id) else {
        return;
    };
    let mut lane = app.view.lanes[index].clone();
    let previous = lane.foreground.clone();
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, PANEL);
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), 32.0));
    header::show(app, &mut child(ui, header, ("lane-header", id)), &mut lane);
    if previous != lane.foreground {
        gesture::finish(app, ui.ctx(), false);
        if app
            .parameter_lane
            .owner
            .as_ref()
            .is_some_and(|owner| owner.lane == id)
        {
            app.parameter_lane.owner = None;
            parameter_clear(app);
        }
    }
    app.view.lanes[index] = lane.clone();
    reconcile(app);
    let canvas = Rect::from_min_max(
        pos2(rect.left() + GUTTER, header.bottom()),
        pos2(rect.right() - 42.0, rect.bottom()),
    );
    if canvas.height() < 5.0 || canvas.width() < 5.0 {
        return;
    }
    ui.painter().rect_filled(canvas, 0, CANVAS);
    let response = ui.interact(
        canvas,
        Id::new(("parameter-canvas", id)),
        Sense::click_and_drag(),
    );
    if !matches!(app.gesture, Some(Gesture::Parameter(_)))
        && !super::phonemes::phoneme_dragging(app)
    {
        scroll(ui, &response, canvas, &mut app.view.piano);
        let pan = app.view_pan.drag(
            ui,
            super::view_pan::Target::Parameter(id),
            canvas,
            app.gesture.is_none() && app.group_gesture.is_none(),
        );
        app.view.piano.start = (app.view.piano.start
            - f64::from(pan.x) / f64::from(app.view.piano.beat_width))
        .max(0.0);
    }
    grid(
        ui,
        canvas,
        &app.view.piano,
        &app.editor.project.time,
        app.view.step(),
    );
    let Some(reference) = app.editor.active_ref().cloned() else {
        return;
    };
    if reference.visible_end() <= reference.blick_absolute_begin {
        return;
    }
    let mapping = Mapping {
        canvas,
        reference,
        viewport: app.view.piano.clone(),
        target: lane.foreground.clone(),
        zoom: lane.zoom,
    };
    for target in [&lane.foreground, &lane.background] {
        if let Some(curve) = app.editor.automation(target)
            && let Err(error) = curve.validate()
        {
            app.error = Some(error);
            return;
        }
    }
    gesture::interaction(app, ui, &response, &lane, &mapping);
    let selected = if app
        .parameter_lane
        .owner
        .as_ref()
        .is_some_and(|owner| owner.lane == id && owner.target == lane.foreground)
    {
        app.parameter_lane.selected.clone()
    } else {
        BTreeSet::new()
    };
    paint::show(app, ui, &lane, &mapping, &selected);
    gesture::selection_overlay(app, ui, id, &mapping);
    playhead_line(ui, canvas, &app.view.piano, app.editor.transport.position);
    if response.secondary_clicked() && app.gesture.is_none() {
        super::phonemes::phoneme_deactivate(app);
        app.commit_lyric();
        ui.ctx().memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
        focus(app, &lane);
    }
    response.context_menu(|ui| {
        focus(app, &lane);
        let selected = parameter_has_selection(app);
        if ui
            .add_enabled(selected, Button::new("复制控制点"))
            .clicked()
        {
            parameter_copy(app, ui.ctx());
            ui.close();
        }
        if ui
            .add_enabled(selected, Button::new("剪切控制点"))
            .clicked()
        {
            parameter_cut(app, ui.ctx());
            ui.close();
        }
        if ui
            .add_enabled(parameter_can_paste(app), Button::new("粘贴控制点"))
            .clicked()
        {
            parameter_paste(app, ui.ctx());
            ui.close();
        }
        if ui
            .add_enabled(selected, Button::new("删除控制点"))
            .clicked()
        {
            parameter_delete(app);
            ui.close();
        }
        if ui.button("选择全部控制点").clicked() {
            parameter_select_all(app);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(!app.editor.is_editing(), Button::new("清空此参数的控制点"))
            .clicked()
        {
            app.editor.begin("清空参数控制点");
            match app
                .editor
                .replace_automation_points(&lane.foreground, Vec::new())
            {
                Ok(_) => app.editor.finish(),
                Err(error) => {
                    app.editor.cancel();
                    app.error = Some(error);
                }
            }
            parameter_clear(app);
            ui.close();
        }
    });
    if response.hovered() {
        app.status = match lane.tool {
            ParameterTool::ControlPoints => {
                "双击添加控制点 · 拖动移动 / 框选 · Ctrl+点击切换选择 · Shift+拖动选择时间范围"
            }
            ParameterTool::Freehand => {
                "左键拖动手绘曲线 · 右键拖动擦除控制点 · Esc 取消 · Ctrl+Z 撤销"
            }
            ParameterTool::Line => "左键拖动绘制直线 · 右键拖动擦除控制点 · Esc 取消 · Ctrl+Z 撤销",
        }
        .into();
        if !editable(app, &lane.foreground) {
            app.status = "当前声库不支持口型参数；已导入的曲线仍可查看与清空".into();
        }
    }
}

pub fn parameter_cancel(app: &mut App, ctx: &Context) {
    gesture::finish(app, ctx, true);
}

pub fn finish_parameter(app: &mut App, ctx: &Context) {
    gesture::finish(app, ctx, false);
}

pub fn end_parameter_frame(app: &mut App, ctx: &Context) {
    gesture::end_frame(app, ctx);
    reconcile(app);
}
