use super::geometry::Mapping;
use crate::{
    app::App,
    model::{Automation, AutomationTarget, Parameter},
    ui::{state::Lane, theme::*, track_color::TrackColors},
};
use eframe::egui::*;
use std::collections::BTreeSet;

pub(super) fn show(app: &App, ui: &Ui, lane: &Lane, mapping: &Mapping, selected: &BTreeSet<i64>) {
    let colors = TrackColors::new(app.editor.current_track());
    let mut background = mapping.clone();
    background.target = lane.background.clone();
    axes(ui, mapping, false);
    axes(ui, &background, true);
    let painter = ui
        .painter()
        .with_clip_rect(mapping.canvas.intersect(ui.clip_rect()));
    let default = lane.foreground.default_value();
    for value in [Some(0.0), (default != 0.0).then_some(default)]
        .into_iter()
        .flatten()
    {
        let y = mapping.point((0, value)).y;
        if mapping.canvas.y_range().contains(y) {
            painter.hline(mapping.canvas.x_range(), y, Stroke::new(0.8, GRID));
        }
    }
    curve(
        ui,
        &background,
        app.editor.automation(&lane.background),
        MUTED.gamma_multiply(0.65),
    );
    curve(
        ui,
        mapping,
        app.editor.automation(&lane.foreground),
        colors.accent,
    );
    if let Some(automation) = app.editor.automation(&lane.foreground) {
        let painter = ui
            .painter()
            .with_clip_rect(mapping.clip().intersect(ui.clip_rect()));
        for &(time, value) in &automation.points {
            let point = mapping.point((time, value));
            if !mapping.clip().expand(5.0).contains(point) {
                continue;
            }
            let selected = selected.contains(&time);
            painter.circle_filled(point, if selected { 3.5 } else { 2.5 }, colors.accent);
            if selected {
                painter.circle_stroke(point, 3.5, Stroke::new(1.2, Color32::WHITE));
            }
        }
    }
}

fn curve(ui: &Ui, mapping: &Mapping, automation: Option<&Automation>, color: Color32) {
    let clip = mapping.clip().intersect(ui.clip_rect());
    if !clip.is_positive() {
        return;
    }
    let painter = ui.painter().with_clip_rect(clip);
    let default = mapping.target.default_value();
    let Some(automation) = automation.filter(|curve| !curve.points.is_empty()) else {
        painter.hline(
            clip.x_range(),
            mapping.point((0, default)).y,
            Stroke::new(1.2, color),
        );
        return;
    };
    let Ok(sampler) = automation.sampler(default) else {
        return;
    };
    let count = clip.width().ceil() as usize;
    let mut points = Vec::with_capacity(count + 1);
    for index in 0..=count {
        let x = (clip.left() + index as f32).min(clip.right());
        let (time, _) = mapping.value_at(pos2(x, clip.center().y));
        let Ok(value) = sampler.sample(time) else {
            return;
        };
        points.push(pos2(x, mapping.point((time, value)).y));
    }
    painter.add(Shape::line(points, Stroke::new(1.4, color)));
}

fn axes(ui: &Ui, mapping: &Mapping, right: bool) {
    let rect = mapping.canvas;
    let (low, high) = mapping.visible_range();
    if high <= low {
        return;
    }
    let step = nice_step((high - low) / 4.0);
    let first = (low / step).ceil() as i64;
    let last = (high / step).floor() as i64;
    let color = if right {
        MUTED.gamma_multiply(0.7)
    } else {
        MUTED
    };
    let painter = ui.painter();
    let x = if right {
        rect.right() + 4.0
    } else {
        rect.left() - 5.0
    };
    let align = if right {
        Align2::LEFT_CENTER
    } else {
        Align2::RIGHT_CENTER
    };
    for index in first..=last {
        let value = index as f64 * step;
        let y = mapping
            .point((0, value))
            .y
            .clamp(rect.top() + 7.0, rect.bottom() - 7.0);
        painter.text(
            pos2(x, y),
            align,
            axis_label(&mapping.target, value, step),
            FontId::proportional(10.0),
            color,
        );
    }
}

fn nice_step(value: f64) -> f64 {
    let power = 10.0_f64.powf(value.log10().floor());
    let fraction = value / power;
    power
        * if fraction <= 1.0 {
            1.0
        } else if fraction <= 2.0 {
            2.0
        } else if fraction <= 5.0 {
            5.0
        } else {
            10.0
        }
}

fn axis_label(target: &AutomationTarget, value: f64, step: f64) -> String {
    if matches!(target, AutomationTarget::Parameter(Parameter::Tension)) {
        if (value - 1.0).abs() < 1e-6 {
            return "紧张".into();
        }
        if (value + 1.0).abs() < 1e-6 {
            return "放松".into();
        }
    }
    let decimals = if step >= 1.0 {
        0
    } else {
        (-step.log10().floor()) as usize
    };
    let value = if value.abs() < step * 1e-6 {
        0.0
    } else {
        value
    };
    format!("{value:.decimals$}")
}
