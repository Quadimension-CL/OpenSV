mod create;
mod edits;

use super::{geometry::Mapping, *};
use std::collections::BTreeMap;

pub struct ParameterGesture {
    owner: Owner,
    mapping: Mapping,
    origin: Pos2,
    pointer: Pos2,
    original: Vec<(i64, f64)>,
    initial: BTreeSet<i64>,
    selected: BTreeSet<i64>,
    kind: Kind,
    button: PointerButton,
    frame: u64,
    transaction: bool,
}

enum Kind {
    Select {
        time_range: bool,
        additive: bool,
    },
    Move,
    Draw {
        line: bool,
        first: (i64, f64),
        last: (i64, f64),
        samples: BTreeMap<i64, f64>,
        range: (i64, i64),
    },
    Erase {
        range: (i64, i64),
        radius: i64,
    },
}

impl ParameterGesture {
    pub fn lane(&self) -> u64 {
        self.owner.lane
    }
    fn owns(&self, app: &App) -> bool {
        app.editor.track == self.owner.track
            && app.editor.active_ref() == Some(&self.mapping.reference)
            && app.view.lanes.iter().any(|lane| {
                lane.id == self.owner.lane && lane.visible && lane.foreground == self.owner.target
            })
            && (!self.transaction || app.editor.is_editing())
    }
}

fn blocked(app: &App, ctx: &Context) -> bool {
    app.piano_collapsed
        || app.pending.is_some()
        || app.error.is_some()
        || app.settings
        || app.voice_manager
        || app.about
        || app.rename.is_some()
        || app.time_edit.is_some()
        || app.fill_lyrics.is_some()
        || app.midi_import.is_some()
        || app.synthesis.context_import.dialog.is_some()
        || Popup::is_any_open(ctx)
        || app.view_pan.is_active()
}

pub(super) fn interaction(
    app: &mut App,
    ui: &Ui,
    response: &Response,
    lane: &Lane,
    mapping: &Mapping,
) {
    if !ui.is_enabled() || blocked(app, ui.ctx()) || !ui.input(|input| input.focused) {
        return;
    }
    if response.hovered() {
        ui.ctx()
            .set_cursor_icon(if lane.tool == ParameterTool::ControlPoints {
                CursorIcon::Default
            } else {
                CursorIcon::Crosshair
            });
    }
    if response.double_clicked()
        && lane.tool == ParameterTool::ControlPoints
        && let Some(pos) = response.interact_pointer_pos()
        && mapping.clip().contains(pos)
        && editable(app, &lane.foreground)
    {
        finish(app, ui.ctx(), false);
        create::add_point(app, lane, mapping, pos);
        return;
    }
    for event in ui.input(|input| input.events.clone()) {
        match event {
            Event::PointerButton {
                pos,
                button,
                pressed: true,
                modifiers,
            } if app.gesture.is_none()
                && app.group_gesture.is_none()
                && mapping.clip().contains(pos)
                && (response.hovered()
                    || response.is_pointer_button_down_on()
                    || response.clicked_by(button)
                    || response.dragged_by(button)) =>
            {
                if button == PointerButton::Primary
                    || (button == PointerButton::Secondary
                        && lane.tool != ParameterTool::ControlPoints)
                {
                    start(app, ui, lane, mapping, pos, button, modifiers);
                }
            }
            Event::PointerMoved(pos) if matches!(&app.gesture, Some(Gesture::Parameter(gesture)) if gesture.owner.lane == lane.id) =>
            {
                extend(app, pos);
            }
            Event::PointerButton {
                pos,
                button,
                pressed: false,
                ..
            } if matches!(&app.gesture, Some(Gesture::Parameter(gesture)) if gesture.owner.lane == lane.id && gesture.button == button) =>
            {
                extend(app, pos);
                finish(app, ui.ctx(), false);
            }
            _ => {}
        }
    }
    if let Some(Gesture::Parameter(gesture)) = &mut app.gesture
        && gesture.owner.lane == lane.id
    {
        gesture.frame = ui.ctx().cumulative_frame_nr();
        ui.ctx().set_dragged_id(response.id);
    }
}

fn start(
    app: &mut App,
    ui: &Ui,
    lane: &Lane,
    mapping: &Mapping,
    pos: Pos2,
    button: PointerButton,
    modifiers: Modifiers,
) {
    crate::ui::phonemes::phoneme_deactivate(app);
    app.commit_lyric();
    if app.editor.is_editing() {
        return;
    }
    ui.ctx().memory_mut(|memory| {
        if let Some(id) = memory.focused() {
            memory.surrender_focus(id);
        }
    });
    focus(app, lane);
    let original = app
        .editor
        .automation(&lane.foreground)
        .map_or_else(Vec::new, |curve| curve.points.clone());
    let initial = app.parameter_lane.selected.clone();
    let point = mapping.value_at(pos);
    let kind = if lane.tool == ParameterTool::ControlPoints {
        if let Some(time) = mapping.nearest(&original, pos) {
            if modifiers.ctrl || modifiers.command {
                if !app.parameter_lane.selected.remove(&time) {
                    app.parameter_lane.selected.insert(time);
                }
                app.parameter_lane.anchor = Some(time);
                return;
            }
            if modifiers.shift {
                let anchor = app.parameter_lane.anchor.unwrap_or(time);
                app.parameter_lane
                    .selected
                    .extend(original.iter().filter_map(|point| {
                        ((anchor.min(time)..=anchor.max(time)).contains(&point.0))
                            .then_some(point.0)
                    }));
            } else if !app.parameter_lane.selected.contains(&time) {
                app.parameter_lane.selected = BTreeSet::from([time]);
            }
            app.parameter_lane.anchor = Some(time);
            if !editable(app, &lane.foreground) {
                return;
            }
            Kind::Move
        } else {
            if !modifiers.ctrl && !modifiers.command {
                app.parameter_lane.selected.clear();
            }
            Kind::Select {
                time_range: modifiers.shift,
                additive: modifiers.ctrl || modifiers.command,
            }
        }
    } else {
        if !editable(app, &lane.foreground) {
            return;
        }
        if button == PointerButton::Secondary {
            let radius = (4.0 * QUARTER as f64 / f64::from(mapping.viewport.beat_width))
                .round()
                .max(1.0) as i64;
            Kind::Erase {
                range: (point.0, point.0),
                radius,
            }
        } else {
            Kind::Draw {
                line: lane.tool == ParameterTool::Line,
                first: point,
                last: point,
                samples: BTreeMap::from([point]),
                range: (point.0, point.0),
            }
        }
    };
    let transaction = !matches!(kind, Kind::Select { .. });
    if transaction {
        app.editor.begin(match kind {
            Kind::Move => "移动参数控制点",
            Kind::Erase { .. } => "擦除参数控制点",
            _ => "绘制参数曲线",
        });
    }
    let selected = app.parameter_lane.selected.clone();
    app.gesture = Some(Gesture::Parameter(Box::new(ParameterGesture {
        owner: app.parameter_lane.owner.as_ref().unwrap().clone(),
        mapping: mapping.clone(),
        origin: pos,
        pointer: pos,
        original,
        initial,
        selected,
        kind,
        button,
        frame: ui.ctx().cumulative_frame_nr(),
        transaction,
    })));
    extend(app, pos);
}

fn extend(app: &mut App, pos: Pos2) {
    let Some(Gesture::Parameter(mut gesture)) = app.gesture.take() else {
        return;
    };
    if !gesture.owns(app) {
        app.gesture = Some(Gesture::Parameter(gesture));
        return;
    }
    gesture.pointer = pos;
    let result = edits::update(app, &mut gesture);
    app.gesture = Some(Gesture::Parameter(gesture));
    if let Err(error) = result {
        finish_without_context(app, true);
        app.error = Some(error);
    }
}

pub(super) fn finish_without_context(app: &mut App, cancel: bool) {
    if !matches!(app.gesture, Some(Gesture::Parameter(_))) {
        return;
    }
    let Some(Gesture::Parameter(gesture)) = app.gesture.take() else {
        return;
    };
    if gesture.transaction && app.editor.is_editing() {
        if cancel {
            app.editor.cancel();
        } else {
            app.editor.finish();
        }
    }
    if cancel {
        app.parameter_lane.selected = gesture.initial;
    }
    app.parameter_lane.anchor = app.parameter_lane.selected.first().copied();
}

pub(super) fn finish(app: &mut App, ctx: &Context, cancel: bool) {
    if !matches!(app.gesture, Some(Gesture::Parameter(_))) {
        return;
    }
    finish_without_context(app, cancel);
    ctx.stop_dragging();
}

pub(super) fn end_frame(app: &mut App, ctx: &Context) {
    let Some(Gesture::Parameter(gesture)) = &app.gesture else {
        return;
    };
    let cancel = !ctx.input(|input| input.focused);
    let end = !gesture.owns(app)
        || blocked(app, ctx)
        || gesture.frame != ctx.cumulative_frame_nr()
        || !app.parameter_lane.focused
        || ctx.input(|input| !input.pointer.button_down(gesture.button));
    if cancel || end {
        finish(app, ctx, cancel);
    }
}

pub(super) fn selection_overlay(app: &App, ui: &Ui, lane: u64, mapping: &Mapping) {
    let Some(Gesture::Parameter(gesture)) = &app.gesture else {
        return;
    };
    let Kind::Select { time_range, .. } = gesture.kind else {
        return;
    };
    if gesture.owner.lane != lane {
        return;
    }
    let mut area = Rect::from_two_pos(gesture.origin, gesture.pointer).intersect(mapping.clip());
    if time_range {
        area.min.y = mapping.canvas.top();
        area.max.y = mapping.canvas.bottom();
    }
    let color = super::super::track_color::TrackColors::new(app.editor.current_track()).accent;
    ui.painter()
        .rect_filled(area, 0, color.gamma_multiply(0.16));
    ui.painter()
        .rect_stroke(area, 0, Stroke::new(1.0, color), StrokeKind::Inside);
}
