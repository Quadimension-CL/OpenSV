use super::{
    child,
    icons::{Icon, IconButton},
    label,
    piano::Gesture,
    state::DragPayload,
    theme::*,
    timeline::*,
};
use crate::{app::App, model::*};
use eframe::egui::*;

pub fn show(app: &mut App, ui: &mut Ui, id: u64) {
    let Some(index) = app.view.lanes.iter().position(|l| l.id == id) else {
        return;
    };
    let mut lane = app.view.lanes[index].clone();
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, PANEL);
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), 30.0));
    let mut header_ui = child(ui, header, ("lane-header", id));
    header_ui.horizontal(|ui| {
        let (icon_rect, drag) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::drag());
        Icon::Curve.paint(ui, icon_rect.shrink(4.0), MUTED);
        drag.on_hover_text("拖动以调整参数区域顺序")
            .dnd_set_drag_payload(DragPayload::Lane(id));
        parameter_combo(ui, ("foreground", id), &mut lane.foreground);
        if ui
            .add(
                IconButton::new(Icon::Swap, "交换前景与背景参数")
                    .size(vec2(24.0, 24.0))
                    .icon_size(14.0),
            )
            .on_hover_text("交换前景与背景参数")
            .clicked()
        {
            std::mem::swap(&mut lane.foreground, &mut lane.background);
        }
        parameter_combo(ui, ("background", id), &mut lane.background);
        ui.separator();
        let key = lane.foreground.key();
        let mode = app
            .editor
            .group()
            .and_then(|g| g.parameters.get(key))
            .map_or("cubic", |c| c.mode.as_str())
            .to_string();
        ComboBox::from_id_salt(("interpolation", id))
            .width(65.0)
            .selected_text(match mode.as_str() {
                "linear" => "线性",
                "cosine" => "余弦",
                _ => "三次样条",
            })
            .show_ui(ui, |ui| {
                for (value, text) in [
                    ("linear", "线性"),
                    ("cosine", "余弦"),
                    ("cubic", "三次样条"),
                ] {
                    if ui
                        .add_enabled(
                            app.editor.reference.is_some(),
                            Button::selectable(mode == value, text),
                        )
                        .clicked()
                    {
                        app.editor.begin("参数插值模式");
                        app.editor
                            .group_mut()
                            .unwrap()
                            .parameters
                            .entry(key.into())
                            .or_default()
                            .mode = value.into();
                        app.editor.finish();
                    }
                }
            });
        if mode == "cubic" {
            ui.label(RichText::new("控制点预览").size(10.0).color(MUTED))
                .on_hover_text("当前绘制控制多边形；原版三次插值求值尚未实现");
        }
    });
    app.view.lanes[index] = lane.clone();
    let canvas = Rect::from_min_max(pos2(rect.left() + GUTTER, header.bottom()), rect.max);
    if canvas.height() < 5.0 {
        return;
    }
    ui.painter().rect_filled(canvas, 0, CANVAS);
    let response = ui.interact(
        canvas,
        Id::new(("parameter-canvas", id)),
        Sense::click_and_drag(),
    );
    scroll(ui, &response, canvas, &mut app.view.piano);
    grid(
        ui,
        canvas,
        &app.view.piano,
        &app.editor.project.time,
        app.view.step(),
    );
    let (min, max) = lane.foreground.range();
    let y_for =
        |value: f64| canvas.bottom() - ((value - min) / (max - min)) as f32 * canvas.height();
    for value in [min, lane.foreground.default_value(), max] {
        let y = y_for(value).clamp(canvas.top() + 6.0, canvas.bottom() - 6.0);
        label(ui, pos2(rect.left() + 4.0, y), format!("{value:.0}"), MUTED);
        ui.painter()
            .hline(canvas.x_range(), y_for(value), Stroke::new(0.5, GRID));
    }
    let Some(reference) = app.editor.active_ref().cloned() else {
        return;
    };
    let offset = reference.blick_offset;
    let group_clip = super::group_geometry::clip_rect(&reference, canvas, &app.view.piano);
    for (parameter, color, editable) in [
        (lane.background, MUTED.gamma_multiply(0.5), false),
        (lane.foreground, GREEN, true),
    ] {
        let Some(curve) = app.editor.group().unwrap().parameters.get(parameter.key()) else {
            continue;
        };
        let (low, high) = parameter.range();
        let map = |(t, v): (i64, f64)| {
            pos2(
                x_at(canvas, &app.view.piano, beats(t + offset)),
                canvas.bottom() - ((v - low) / (high - low)) as f32 * canvas.height(),
            )
        };
        let painter = ui
            .painter()
            .with_clip_rect(group_clip.intersect(ui.clip_rect()));
        if curve.points.is_empty() {
            let y = map((0, parameter.default_value())).y;
            painter.hline(canvas.x_range(), y, Stroke::new(1.0, color));
        } else {
            let first = map(curve.points[0]);
            let last = map(*curve.points.last().unwrap());
            painter.hline(
                canvas.left()..=first.x.max(canvas.left()).min(canvas.right()),
                first.y,
                Stroke::new(1.0, color),
            );
            painter.hline(
                last.x.min(canvas.right()).max(canvas.left())..=canvas.right(),
                last.y,
                Stroke::new(1.0, color),
            );
            for pair in curve.points.windows(2) {
                let a = map(pair[0]);
                let b = map(pair[1]);
                if curve.mode == "cosine" {
                    let points: Vec<_> = (0..=24)
                        .map(|i| {
                            let t = i as f32 / 24.0;
                            pos2(
                                a.x + (b.x - a.x) * t,
                                a.y + (b.y - a.y) * (1.0 - (t * std::f32::consts::PI).cos()) * 0.5,
                            )
                        })
                        .collect();
                    painter.add(Shape::line(points, Stroke::new(1.2, color)));
                } else if curve.mode == "cubic" {
                    painter.extend(Shape::dashed_line(
                        &[a, b],
                        Stroke::new(1.0, color),
                        4.0,
                        3.0,
                    ));
                } else {
                    painter.line_segment([a, b], Stroke::new(1.2, color));
                }
            }
            if editable {
                for &point in &curve.points {
                    painter.circle_filled(map(point), 3.5, GREEN);
                }
            }
        }
    }
    let point_from = |p: Pos2, app: &App| {
        let time = ticks(beat_at(canvas, &app.view.piano, p.x))
            .clamp(reference.blick_absolute_begin, reference.visible_end() - 1)
            - offset;
        let value = (min + (canvas.bottom() - p.y) as f64 / canvas.height() as f64 * (max - min))
            .clamp(min, max);
        (time, value as f32 as f64)
    };
    let nearest = |p: Pos2, app: &App| {
        app.editor
            .group()
            .unwrap()
            .parameters
            .get(lane.foreground.key())
            .and_then(|curve| {
                curve
                    .points
                    .iter()
                    .find(|(t, v)| {
                        reference.contains(*t + offset)
                            && pos2(x_at(canvas, &app.view.piano, beats(*t + offset)), y_for(*v))
                                .distance(p)
                                < 8.0
                    })
                    .map(|p| p.0)
            })
    };
    if response.double_clicked()
        && let Some(p) = response.interact_pointer_pos()
        && group_clip.contains(p)
    {
        let (t, v) = point_from(p, app);
        app.editor.begin("添加参数控制点");
        app.editor
            .group_mut()
            .unwrap()
            .parameters
            .entry(lane.foreground.key().into())
            .or_default()
            .insert(t, v);
        app.editor.finish();
    }
    if response.drag_started_by(PointerButton::Primary)
        && let Some(p) = ui.input(|i| i.pointer.press_origin())
        && group_clip.contains(p)
        && let Some(time) = nearest(p, app)
    {
        app.editor.begin("移动参数控制点");
        app.gesture = Some(Gesture::Parameter { lane: id, time });
    }
    if response.dragged_by(PointerButton::Primary)
        && let Some(Gesture::Parameter { lane: active, time }) = app.gesture
        && active == id
        && let Some(p) = response.interact_pointer_pos()
        && group_clip.contains(p)
    {
        let (t, v) = point_from(p, app);
        let curve = app
            .editor
            .group_mut()
            .unwrap()
            .parameters
            .entry(lane.foreground.key().into())
            .or_default();
        curve.points.retain(|p| p.0 != time);
        curve.insert(t, v);
        app.gesture = Some(Gesture::Parameter { lane: id, time: t });
    }
    if response.drag_stopped_by(PointerButton::Primary) {
        app.editor.finish();
        app.gesture = None;
    }
    response.context_menu(|ui| {
        if ui.button("清空此参数的控制点").clicked() {
            app.editor.begin("清空参数");
            app.editor
                .group_mut()
                .unwrap()
                .parameters
                .entry(lane.foreground.key().into())
                .or_default()
                .points
                .clear();
            app.editor.finish();
            ui.close();
        }
    });
    playhead_line(ui, canvas, &app.view.piano, app.editor.transport.position);
}
fn parameter_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    parameter: &mut Parameter,
) {
    ComboBox::from_id_salt(id)
        .width(74.0)
        .selected_text(parameter.label())
        .show_ui(ui, |ui| {
            for p in Parameter::ALL {
                ui.selectable_value(parameter, p, p.label());
            }
        });
}
