use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Owner {
    pub track: EntityId,
    pub reference: EntityId,
    pub group: String,
    pub lane: u64,
    pub target: AutomationTarget,
}

#[derive(Default)]
pub struct ParameterSelection {
    pub(super) owner: Option<Owner>,
    pub(super) selected: BTreeSet<i64>,
    pub(super) anchor: Option<i64>,
    pub(super) focused: bool,
}

pub(super) fn focus(app: &mut App, lane: &Lane) {
    crate::ui::phonemes::phoneme_deactivate(app);
    let Some(reference) = app.editor.active_ref() else {
        return;
    };
    let owner = Owner {
        track: app.editor.track,
        reference: reference.id,
        group: reference.group_id.clone(),
        lane: lane.id,
        target: lane.foreground.clone(),
    };
    if app.parameter_lane.owner.as_ref() != Some(&owner) {
        parameter_clear(app);
    }
    app.parameter_lane.owner = Some(owner);
    app.parameter_lane.focused = true;
    crate::ui::piano::anchor_clear(app);
}

pub(super) fn reconcile(app: &mut App) {
    let Some(owner) = &app.parameter_lane.owner else {
        return;
    };
    let valid =
        !app.piano_collapsed
            && app.editor.track == owner.track
            && app.editor.active_ref().is_some_and(|reference| {
                reference.id == owner.reference && reference.group_id == owner.group
            })
            && app.view.lanes.iter().any(|lane| {
                lane.id == owner.lane && lane.visible && lane.foreground == owner.target
            });
    if !valid {
        app.parameter_lane.owner = None;
        app.parameter_lane.focused = false;
        parameter_clear(app);
        return;
    }
    let bounds = editable_bounds(app).unwrap();
    let times: BTreeSet<_> = app
        .editor
        .automation(&owner.target)
        .into_iter()
        .flat_map(|curve| &curve.points)
        .filter(|point| (bounds.0..=bounds.1).contains(&point.0))
        .map(|point| point.0)
        .collect();
    app.parameter_lane
        .selected
        .retain(|time| times.contains(time));
    if app
        .parameter_lane
        .anchor
        .is_some_and(|time| !times.contains(&time))
    {
        app.parameter_lane.anchor = None;
    }
}

pub(super) fn active_lane(app: &App) -> Option<&Lane> {
    let owner = app.parameter_lane.owner.as_ref()?;
    app.view.lanes.iter().find(|lane| lane.id == owner.lane)
}

pub(super) fn editable_bounds(app: &App) -> Option<(i64, i64)> {
    let reference = app.editor.active_ref()?;
    Some((
        reference
            .blick_absolute_begin
            .saturating_sub(reference.blick_offset),
        reference
            .visible_end()
            .saturating_sub(reference.blick_offset)
            .saturating_sub(1),
    ))
}

pub(super) fn editable(app: &App, target: &AutomationTarget) -> bool {
    !matches!(target, AutomationTarget::Parameter(Parameter::Mouth))
        || app
            .voice_selection()
            .and_then(|key| {
                app.synthesis
                    .library
                    .entry(&key)
                    .and_then(|entry| entry.family)
            })
            .is_none_or(|family| family == crate::synthesis::voice::VoiceFamily::Sv2)
}

pub fn parameter_keyboard(app: &mut App, ctx: &Context) -> bool {
    reconcile(app);
    if !parameter_active(app)
        || app.gesture.is_some()
        || app.editor.is_editing()
        || ctx.egui_wants_keyboard_input()
    {
        return false;
    }
    for (key, tool) in [
        (Key::Num1, ParameterTool::ControlPoints),
        (Key::Num2, ParameterTool::Freehand),
        (Key::Num3, ParameterTool::Line),
    ] {
        if ctx.input_mut(|input| input.consume_key(Modifiers::ALT, key)) {
            if let Some(id) = app.parameter_lane.owner.as_ref().map(|owner| owner.lane)
                && let Some(lane) = app.view.lanes.iter_mut().find(|lane| lane.id == id)
            {
                lane.tool = tool;
            }
            return true;
        }
    }
    let Some(target) = app
        .parameter_lane
        .owner
        .as_ref()
        .map(|owner| owner.target.clone())
    else {
        return false;
    };
    if !editable(app, &target) {
        return false;
    }
    let (low, high) = target.range();
    for (key, time, value) in [
        (Key::ArrowLeft, -QUARTER / 16, 0.0),
        (Key::ArrowRight, QUARTER / 16, 0.0),
        (Key::ArrowUp, 0, (high - low) / 100.0),
        (Key::ArrowDown, 0, -(high - low) / 100.0),
    ] {
        let shift = ctx.input(|input| input.modifiers.shift);
        let modifiers = if shift {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        if ctx.input_mut(|input| input.consume_key(modifiers, key)) {
            let scale = if shift { 10 } else { 1 };
            commands::shift_selected(app, time * scale, value * scale as f64);
            return true;
        }
    }
    false
}
