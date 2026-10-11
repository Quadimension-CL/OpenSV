//! A captured freehand stroke edits one group and one undo transaction.
mod draw;

pub(super) use draw::draw;

use super::{Gesture, Tool};
use crate::{
    app::App,
    model::{EntityId, GroupRef, PitchControl, pitch, ticks},
    ui::{group_geometry::clip_rect, timeline::beat_at},
};
use eframe::egui::*;
use std::collections::BTreeMap;

pub struct PitchGesture {
    track: EntityId,
    reference: GroupRef,
    original: Vec<PitchControl>,
    points: BTreeMap<i64, f64>,
    last: (i64, f64),
    erase_range: (i64, i64),
    erase_radius: i64,
    button: PointerButton,
    frame: u64,
}

impl PitchGesture {
    fn owns_transaction(&self, app: &App) -> bool {
        app.editor.is_editing()
            && app.editor.track == self.track
            && app.editor.active_ref() == Some(&self.reference)
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

pub(super) fn interaction(app: &mut App, ui: &Ui, response: &Response, canvas: Rect) {
    let enabled = ui.is_enabled() && ui.input(|i| i.focused) && !blocked(app, ui.ctx());
    if !enabled {
        return;
    }
    if response.hovered() || matches!(app.gesture, Some(Gesture::Pitch(_))) {
        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
    }
    let events = ui.input(|i| i.events.clone());
    let mut dirty = false;
    for event in events {
        match event {
            Event::PointerButton {
                pos,
                button,
                pressed: true,
                ..
            } if matches!(button, PointerButton::Primary | PointerButton::Secondary)
                && app.gesture.is_none()
                && canvas.contains(pos)
                && (response.contains_pointer()
                    || response.is_pointer_button_down_on()
                    || response.clicked_by(button)
                    || response.dragged_by(button)) =>
            {
                dirty |= start(app, ui.ctx(), canvas, pos, button);
            }
            Event::PointerMoved(pos) => {
                dirty |= extend(app, canvas, pos);
            }
            Event::PointerButton {
                pos,
                button,
                pressed: false,
                ..
            } if matches!(&app.gesture, Some(Gesture::Pitch(g)) if g.button == button) => {
                dirty |= extend(app, canvas, pos);
                if dirty {
                    apply(app);
                    dirty = false;
                }
                finish(app, ui.ctx(), false);
            }
            _ => {}
        }
    }
    if dirty {
        apply(app);
    }
    if let Some(Gesture::Pitch(gesture)) = &mut app.gesture {
        gesture.frame = ui.ctx().cumulative_frame_nr();
        ui.ctx().set_dragged_id(response.id);
    }
}

fn start(app: &mut App, ctx: &Context, canvas: Rect, pos: Pos2, button: PointerButton) -> bool {
    app.commit_lyric();
    if app.editor.is_editing() {
        return false;
    }
    let Some(reference) = app.editor.active_ref().cloned() else {
        app.status = "请先点击音符组标题，再绘制音高曲线".into();
        return false;
    };
    let global = ticks(beat_at(canvas, &app.view.piano, pos.x));
    if !reference.contains(global) {
        app.status = "请在当前音符组范围内绘制音高，点击标题可切换音符组".into();
        return false;
    }
    let Some(group) = app.editor.group() else {
        return false;
    };
    let original = group.pitch_controls.clone();
    let point = sample(app, canvas, &reference, pos);
    let pixel = ticks(1.0 / app.view.piano.beat_width as f64).max(1);
    let begin = reference
        .blick_absolute_begin
        .saturating_sub(reference.blick_offset);
    let end = reference
        .visible_end()
        .saturating_sub(reference.blick_offset);
    let left = point.0.saturating_sub(pixel).max(begin);
    let right = point.0.saturating_add(pixel).min(end);
    if left >= right {
        return false;
    }
    app.editor.begin(if button == PointerButton::Primary {
        "手绘音高"
    } else {
        "擦除手绘音高"
    });
    app.gesture = Some(Gesture::Pitch(Box::new(PitchGesture {
        track: app.editor.track,
        reference,
        original,
        points: BTreeMap::from([(left, point.1), (right, point.1)]),
        last: point,
        erase_range: (left, right),
        erase_radius: pixel.saturating_mul(3),
        button,
        frame: ctx.cumulative_frame_nr(),
    })));
    app.show_pitch = true;
    app.status.clear();
    true
}

fn sample(app: &App, canvas: Rect, reference: &GroupRef, pos: Pos2) -> (i64, f64) {
    let pos = clip_rect(reference, canvas, &app.view.piano).clamp(pos);
    let time = ticks(beat_at(canvas, &app.view.piano, pos.x))
        .clamp(reference.blick_absolute_begin, reference.visible_end())
        .saturating_sub(reference.blick_offset);
    let midi = (app.view.top_pitch + 0.5 - (pos.y - canvas.top()) / app.view.row_height)
        .clamp(0.0, 127.0) as f64
        - reference.pitch_offset as f64;
    (time, midi)
}

fn extend(app: &mut App, canvas: Rect, pos: Pos2) -> bool {
    if !matches!(&app.gesture, Some(Gesture::Pitch(_))) {
        return false;
    }
    let Some(Gesture::Pitch(mut gesture)) = app.gesture.take() else {
        return false;
    };
    if !gesture.owns_transaction(app) {
        return false;
    }
    let point = sample(app, canvas, &gesture.reference, pos);
    let changed = point != gesture.last;
    if changed {
        let left = point.0.min(gesture.last.0);
        let right = point.0.max(gesture.last.0);
        gesture
            .points
            .retain(|&time, _| time < left || time > right);
        gesture.points.insert(gesture.last.0, gesture.last.1);
        gesture.points.insert(point.0, point.1);
        gesture.erase_range.0 = gesture.erase_range.0.min(left);
        gesture.erase_range.1 = gesture.erase_range.1.max(right);
        gesture.last = point;
    }
    app.gesture = Some(Gesture::Pitch(gesture));
    changed
}

fn apply(app: &mut App) {
    if !matches!(&app.gesture, Some(Gesture::Pitch(_))) {
        return;
    }
    let Some(Gesture::Pitch(gesture)) = app.gesture.take() else {
        return;
    };
    if !gesture.owns_transaction(app) {
        return;
    }
    if let Some(group) = app.editor.group_mut() {
        group.pitch_controls.clone_from(&gesture.original);
        if gesture.button == PointerButton::Primary {
            let points: Vec<_> = gesture
                .points
                .iter()
                .map(|(&time, &value)| (time, value))
                .collect();
            pitch::replace_curve(&mut group.pitch_controls, &points);
        } else {
            let begin = gesture
                .reference
                .blick_absolute_begin
                .saturating_sub(gesture.reference.blick_offset);
            let end = gesture
                .reference
                .visible_end()
                .saturating_sub(gesture.reference.blick_offset);
            pitch::erase_curves(
                &mut group.pitch_controls,
                gesture
                    .erase_range
                    .0
                    .saturating_sub(gesture.erase_radius)
                    .max(begin),
                gesture
                    .erase_range
                    .1
                    .saturating_add(gesture.erase_radius)
                    .min(end),
            );
        }
    }
    app.gesture = Some(Gesture::Pitch(gesture));
}

fn finish(app: &mut App, ctx: &Context, cancel: bool) {
    if !matches!(&app.gesture, Some(Gesture::Pitch(_))) {
        return;
    }
    let Some(Gesture::Pitch(gesture)) = app.gesture.take() else {
        return;
    };
    if gesture.owns_transaction(app) {
        if cancel {
            app.editor.cancel();
        } else {
            app.editor.finish();
        }
    }
    if ctx.dragged_id() == Some(Id::new("piano-canvas")) {
        ctx.stop_dragging();
    }
    ctx.request_repaint();
}

/// Close the transaction even if the canvas disappeared or the pointer left the app.
pub fn end_pitch_frame(app: &mut App, ctx: &Context) {
    let Some(Gesture::Pitch(gesture)) = &app.gesture else {
        return;
    };
    let cancel = !gesture.owns_transaction(app)
        || app.piano_tool != Tool::PitchDraw
        || gesture.frame != ctx.cumulative_frame_nr()
        || blocked(app, ctx)
        || ctx.input(|i| !i.focused || !i.pointer.has_pointer());
    if cancel || ctx.input(|i| !i.pointer.button_down(gesture.button)) {
        finish(app, ctx, cancel);
    }
}
