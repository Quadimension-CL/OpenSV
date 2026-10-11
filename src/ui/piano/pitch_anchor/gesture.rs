use super::{
    geometry::{self, Handle},
    id,
    transform::{self, Transform},
};
use crate::ui::{group_geometry::clip_rect, piano::Gesture};
use crate::{
    app::App,
    model::{EntityId, GroupRef, PitchControl, PitchPoint, ticks},
};
use eframe::egui::*;
use std::collections::BTreeSet;

enum Kind {
    Select,
    Transform {
        original: Vec<PitchControl>,
        handle: Option<Handle>,
        moved: bool,
    },
}

pub struct AnchorGesture {
    track: EntityId,
    reference: GroupRef,
    origin: Pos2,
    initial: BTreeSet<String>,
    initial_candidates: Vec<PitchPoint>,
    selected: BTreeSet<String>,
    kind: Kind,
    pub(super) frame: u64,
}

impl AnchorGesture {
    pub(super) fn owns(&self, app: &App) -> bool {
        app.editor.track == self.track
            && app.editor.active_ref() == Some(&self.reference)
            && (matches!(self.kind, Kind::Select) || app.editor.is_editing())
    }

    pub(super) fn selection_rect(&self, pointer: Pos2) -> Option<Rect> {
        matches!(self.kind, Kind::Select).then(|| Rect::from_two_pos(self.origin, pointer))
    }
}

pub(super) fn start(app: &mut App, ui: &Ui, canvas: Rect) {
    if app.gesture.is_some() || app.editor.is_editing() {
        return;
    }
    let Some(origin) = ui.input(|input| input.pointer.press_origin()) else {
        return;
    };
    let Some(reference) = app.editor.active_ref().cloned() else {
        return;
    };
    if !clip_rect(&reference, canvas, &app.view.piano).contains(origin) {
        return;
    }
    let initial = app.piano_anchor.ids.clone();
    let initial_candidates = app.piano_anchor.candidates.clone();
    let bounds = geometry::selected_rect(app, canvas, &reference);
    let handle = bounds.and_then(|rect| {
        geometry::handles(rect)
            .into_iter()
            .find(|(_, rect)| rect.expand(4.0).contains(origin))
            .map(|(handle, _)| handle)
    });
    let hit = geometry::hit(app, canvas, &reference, origin);
    let additive = ui.input(|input| input.modifiers.ctrl || input.modifiers.shift);
    let move_selection = handle.is_some()
        || hit.is_some()
        || (!additive && bounds.is_some_and(|rect| rect.contains(origin)));
    let kind = if move_selection {
        if handle.is_none()
            && let Some(hit) = hit
            && !app.piano_anchor.ids.contains(&hit)
        {
            if !additive {
                app.piano_anchor.ids.clear();
            }
            app.piano_anchor.ids.insert(hit);
        }
        let Some(group) = app.editor.group() else {
            return;
        };
        let mut original = group.pitch_controls.clone();
        original.extend(
            app.piano_anchor
                .candidates
                .iter()
                .filter(|point| app.piano_anchor.ids.contains(&point.id))
                .cloned()
                .map(PitchControl::Point),
        );
        if transform::bounds(&original, &app.piano_anchor.ids).is_none() {
            return;
        }
        app.editor.begin(if handle.is_some() {
            "缩放音高控制点"
        } else {
            "移动音高控制点"
        });
        Kind::Transform {
            original,
            handle,
            moved: false,
        }
    } else {
        if !additive {
            super::anchor_clear(app);
        }
        Kind::Select
    };
    app.gesture = Some(Gesture::Anchor(Box::new(AnchorGesture {
        track: app.editor.track,
        reference,
        origin,
        initial,
        initial_candidates,
        selected: app.piano_anchor.ids.clone(),
        kind,
        frame: ui.ctx().cumulative_frame_nr(),
    })));
    app.status.clear();
}

pub(super) fn update(app: &mut App, ui: &Ui, canvas: Rect) {
    if !matches!(app.gesture, Some(Gesture::Anchor(_))) {
        return;
    }
    let Some(Gesture::Anchor(mut gesture)) = app.gesture.take() else {
        return;
    };
    if !gesture.owns(app) {
        return;
    }
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        app.gesture = Some(Gesture::Anchor(gesture));
        return;
    };
    match &mut gesture.kind {
        Kind::Select => {
            let rect = Rect::from_two_pos(gesture.origin, pointer).intersect(clip_rect(
                &gesture.reference,
                canvas,
                &app.view.piano,
            ));
            let mut selected = gesture.selected.clone();
            if app.editor.group().is_some() {
                for control in &super::controls(app) {
                    if geometry::intersects(app, canvas, &gesture.reference, control, rect)
                        && let Some(id) = id(control)
                    {
                        selected.insert(id.to_owned());
                    }
                }
            }
            app.piano_anchor.ids = selected;
        }
        Kind::Transform {
            original,
            handle,
            moved,
        } => {
            let delta = pointer - gesture.origin;
            let time = ticks(delta.x as f64 / app.view.piano.beat_width as f64);
            let pitch = -(delta.y / app.view.row_height) as f64;
            if let Some(bounds) = transform::bounds(original, &gesture.selected) {
                let change = match handle {
                    None => Transform::Move { time, pitch },
                    Some(Handle::Left) => Transform::Left(bounds.left.saturating_add(time)),
                    Some(Handle::Right) => Transform::Right(bounds.right.saturating_add(time)),
                    Some(Handle::Top) => Transform::Top(bounds.top + pitch),
                    Some(Handle::Bottom) => Transform::Bottom(bounds.bottom + pitch),
                };
                let controls =
                    transform::apply(original, &gesture.selected, &gesture.reference, change);
                if *moved || controls != *original {
                    *moved = true;
                    if let Some(group) = app.editor.group_mut() {
                        group.pitch_controls = controls;
                    }
                    app.piano_anchor
                        .candidates
                        .retain(|point| !gesture.selected.contains(&point.id));
                }
            }
        }
    }
    gesture.frame = ui.ctx().cumulative_frame_nr();
    app.gesture = Some(Gesture::Anchor(gesture));
}

pub(super) fn finish(app: &mut App, ctx: &Context, cancel: bool) {
    if !matches!(app.gesture, Some(Gesture::Anchor(_))) {
        return;
    }
    let Some(Gesture::Anchor(gesture)) = app.gesture.take() else {
        return;
    };
    if gesture.owns(app) {
        if matches!(gesture.kind, Kind::Transform { .. }) {
            if cancel {
                app.editor.cancel();
            } else {
                app.editor.finish();
            }
        }
        if cancel {
            app.piano_anchor.ids = gesture.initial;
            app.piano_anchor.candidates = gesture.initial_candidates;
        }
    }
    if ctx.dragged_id() == Some(Id::new("piano-canvas")) {
        ctx.stop_dragging();
    }
    ctx.request_repaint();
}

pub(super) fn extract_selection(app: &mut App, ui: &Ui, canvas: Rect) {
    let Some(Gesture::Anchor(gesture)) = &app.gesture else {
        return;
    };
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return;
    };
    let Some(area) = gesture.selection_rect(pointer) else {
        return;
    };
    if !gesture.owns(app) || area.width() < 4.0 || area.height() < 4.0 {
        return;
    }
    let reference = gesture.reference.clone();
    let clip = area.intersect(clip_rect(&reference, canvas, &app.view.piano));
    if !clip.is_positive() {
        return;
    }
    let (start, low) = geometry::sample(app, canvas, &reference, clip.left_bottom());
    let (end, high) = geometry::sample(app, canvas, &reference, clip.right_top());
    let points = super::extract::extract(app, &reference, (start, end), (low, high));
    if points.is_empty() || !super::can_add_points(app) {
        return;
    }
    let ids = points
        .iter()
        .map(|point| point.id.clone())
        .collect::<Vec<_>>();
    app.piano_anchor.candidates.extend(points);
    app.piano_anchor.ids.extend(ids);
}
