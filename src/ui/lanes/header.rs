use crate::{
    app::App,
    model::{AutomationTarget, Interpolation, Parameter},
    ui::{
        icons::{Icon, IconButton},
        state::{DragPayload, Lane, ParameterTool},
        theme::*,
    },
};
use eframe::egui::*;

pub(super) fn show(app: &mut App, ui: &mut Ui, lane: &mut Lane) {
    ScrollArea::horizontal()
        .id_salt(("parameter-toolbar-scroll", lane.id))
        .scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysHidden)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| contents(app, ui, lane));
        });
}

fn contents(app: &mut App, ui: &mut Ui, lane: &mut Lane) {
    ui.spacing_mut().item_spacing.x = 3.0;
    let (rect, drag) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::drag());
    Icon::Curve.paint(ui, rect.shrink(5.0), MUTED);
    drag.on_hover_text("拖动以调整参数区域顺序")
        .dnd_set_drag_payload(DragPayload::Lane(lane.id));
    let mut styles = app
        .voice_inventory()
        .and_then(|voice| voice.configuration.as_ref())
        .map_or_else(Vec::new, |configuration| configuration.styles.clone());
    if let Some(group) = app.editor.group() {
        for name in group.vocal_modes.keys() {
            if !styles.contains(name) {
                styles.push(name.clone());
            }
        }
    }
    parameter_combo(ui, ("foreground", lane.id), &mut lane.foreground, &styles)
        .on_hover_text("设置要在前景中编辑的参数");
    if ui
        .add(
            IconButton::new(Icon::Swap, "交换前景与背景参数")
                .size(vec2(24.0, 24.0))
                .icon_size(14.0),
        )
        .clicked()
    {
        std::mem::swap(&mut lane.foreground, &mut lane.background);
    }
    parameter_combo(ui, ("background", lane.id), &mut lane.background, &styles)
        .on_hover_text("设置要在背景中显示的参数");
    ui.add_space(7.0);
    for (tool, glyph, description) in [
        (
            ParameterTool::ControlPoints,
            Glyph::ControlPoints,
            "控制点工具：添加、选择和移动控制点",
        ),
        (
            ParameterTool::Freehand,
            Glyph::Freehand,
            "自由绘制工具：手绘参数曲线；右键拖动擦除",
        ),
        (
            ParameterTool::Line,
            Glyph::Line,
            "直线工具：绘制直线，替换重叠范围内的控制点",
        ),
    ] {
        if icon_button(ui, glyph, lane.tool == tool, description).clicked() {
            lane.tool = tool;
        }
    }
    ui.add_space(7.0);
    let mode = app
        .editor
        .automation(&lane.foreground)
        .map_or("cubic", |curve| curve.mode.as_str())
        .to_owned();
    ui.add_enabled_ui(
        app.editor.active_ref().is_some() && !app.editor.is_editing(),
        |ui| {
            for (interpolation, glyph, description) in [
                (
                    Interpolation::Linear,
                    Glyph::Linear,
                    "线性插值：用直线连接控制点",
                ),
                (
                    Interpolation::Cosine,
                    Glyph::Cosine,
                    "余弦插值：用余弦曲线连接控制点",
                ),
                (
                    Interpolation::Cubic,
                    Glyph::Cubic,
                    "三次样条插值：平滑连接控制点",
                ),
            ] {
                if icon_button(ui, glyph, mode == interpolation.key(), description).clicked()
                    && mode != interpolation.key()
                {
                    app.commit_lyric();
                    app.editor.begin("参数插值模式");
                    match app
                        .editor
                        .set_automation_mode(&lane.foreground, interpolation)
                    {
                        Ok(_) => app.editor.finish(),
                        Err(error) => {
                            app.editor.cancel();
                            app.error = Some(error);
                        }
                    }
                }
            }
        },
    );
    ui.add_space(9.0);
    ui.label(RichText::new("缩放").color(MUTED));
    let mut percent = lane.zoom * 100.0;
    let zoom = ui
        .add(
            DragValue::new(&mut percent)
                .range(25.0..=1600.0)
                .speed(1.0)
                .suffix("%")
                .max_decimals(0),
        )
        .on_hover_text("垂直显示缩放；双击输入，右键恢复 100%");
    if zoom.changed() {
        lane.zoom = (percent / 100.0).clamp(0.25, 16.0);
    }
    if zoom.secondary_clicked() {
        lane.zoom = 1.0;
    }
}

fn parameter_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    target: &mut AutomationTarget,
    styles: &[String],
) -> Response {
    ComboBox::from_id_salt(id)
        .width(78.0)
        .selected_text(target.label())
        .show_ui(ui, |ui| {
            for parameter in Parameter::ALL {
                ui.selectable_value(
                    target,
                    AutomationTarget::Parameter(parameter),
                    parameter.label(),
                );
            }
            if !styles.is_empty() {
                ui.separator();
                ui.label(RichText::new("唱法").color(MUTED));
                for name in styles {
                    ui.selectable_value(target, AutomationTarget::VocalMode(name.clone()), name);
                }
            }
        })
        .response
}

#[derive(Clone, Copy)]
enum Glyph {
    ControlPoints,
    Freehand,
    Line,
    Linear,
    Cosine,
    Cubic,
}

fn icon_button(ui: &mut Ui, glyph: Glyph, selected: bool, description: &'static str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(27.0, 24.0), Sense::click());
    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::Button, ui.is_enabled(), selected, description)
    });
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            3,
            if selected {
                BUTTON_ACTIVE
            } else {
                BUTTON_HOVER
            },
        );
    }
    let color = if ui.is_enabled() {
        ICON_TOOL
    } else {
        MUTED.gamma_multiply(0.55)
    };
    let icon = rect.shrink2(vec2(5.0, 5.0));
    match glyph {
        Glyph::ControlPoints => Icon::PitchAnchor.paint(ui, icon, color),
        Glyph::Freehand => Icon::PitchDraw.paint(ui, icon, color),
        Glyph::Line => {
            let a = icon.left_bottom();
            let b = icon.right_top();
            ui.painter().line_segment([a, b], Stroke::new(1.4, color));
            for point in [a, b] {
                ui.painter().circle_filled(point, 2.0, color);
            }
        }
        Glyph::Linear | Glyph::Cosine | Glyph::Cubic => {
            let points = (0..=24)
                .map(|index| {
                    let t = index as f32 / 24.0;
                    let y = match glyph {
                        Glyph::Linear => 1.0 - (t * 2.0 - 1.0).abs(),
                        Glyph::Cosine => (1.0 - (t * std::f32::consts::TAU).cos()) * 0.5,
                        _ => (t * std::f32::consts::PI).sin(),
                    };
                    pos2(
                        icon.left() + icon.width() * t,
                        icon.bottom() - icon.height() * y,
                    )
                })
                .collect();
            ui.painter()
                .add(Shape::line(points, Stroke::new(1.3, color)));
        }
    }
    response.on_hover_text(description)
}
