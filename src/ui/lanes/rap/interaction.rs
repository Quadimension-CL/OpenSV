use super::{Cell, geometry};
use crate::{app::App, editor::PerformanceParameter, model::EntityId, ui::piano::Gesture};
use eframe::egui::*;

pub struct RapGesture {
    cell: Cell,
    track: EntityId,
    lane: Id,
    origin: Pos2,
    tip: bool,
    frame: u64,
}
pub(super) fn show(app: &mut App, ui: &Ui, response: &Response, cells: &[Cell]) {
    let hit = |p: Pos2| {
        cells.iter().rev().find_map(|c| {
            if !c.editable {
                return None;
            }
            if c.point(1.0).distance(p) < 4.0 {
                return Some((c, true));
            }
            let end = c.point(1.0).x;
            if p.x < c.left || p.x > end || end <= c.left {
                return None;
            }
            let t = (p.x - c.left) / (end - c.left);
            ((c.point(t).y - p.y).abs() < 5.0).then_some((c, false))
        })
    };
    if ui.input(|i| i.pointer.hover_pos()).and_then(hit).is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
    }
    if response.drag_started_by(PointerButton::Primary)
        && app.gesture.is_none()
        && !app.editor.is_editing()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        && let Some((cell, tip)) = hit(origin)
    {
        app.editor.begin(if tip {
            crate::i18n::t("调整说唱语调")
        } else {
            crate::i18n::t("调整说唱音调")
        });
        app.gesture = Some(Gesture::Rap(Box::new(RapGesture {
            cell: cell.clone(),
            track: app.editor.track,
            lane: response.id,
            origin,
            tip,
            frame: ui.ctx().cumulative_frame_nr(),
        })));
    }
    if !matches!(&app.gesture,Some(Gesture::Rap(g)) if g.lane==response.id) {
        return;
    }
    if let Some(Gesture::Rap(mut g)) = app.gesture.take() {
        if app.editor.active_ref() != Some(&g.cell.reference) || app.editor.track != g.track {
            app.editor.cancel();
            return;
        }
        if let Some(p) = response.interact_pointer_pos() {
            let (parameter, value) = if g.tip {
                (
                    PerformanceParameter::RapIntonation,
                    (geometry::normalized_y(g.cell.area, p.y) - geometry::tone_level(g.cell.tone))
                        .clamp(-0.75, 0.75),
                )
            } else {
                (
                    PerformanceParameter::RapTone,
                    geometry::drag_tone(g.cell.area, g.cell.tone, g.origin.y, p.y),
                )
            };
            if !app.editor.set_note_rap_control(
                &g.cell.reference.group_id,
                g.cell.note.id,
                parameter,
                Some(value),
            ) {
                app.editor.cancel();
                return;
            }
        }
        g.frame = ui.ctx().cumulative_frame_nr();
        if response.drag_stopped_by(PointerButton::Primary) {
            app.editor.finish();
        } else {
            app.gesture = Some(Gesture::Rap(g));
        }
    }
}
pub fn end_frame(app: &mut App, ctx: &Context) {
    let Some(Gesture::Rap(g)) = &app.gesture else {
        return;
    };
    let cancel = g.frame != ctx.cumulative_frame_nr()
        || app.editor.active_ref() != Some(&g.cell.reference)
        || app.editor.track != g.track
        || ctx.input(|i| !i.focused);
    if cancel || ctx.input(|i| !i.pointer.primary_down()) {
        if cancel {
            app.editor.cancel();
        } else {
            app.editor.finish();
        }
        app.gesture = None;
        ctx.stop_dragging();
    }
}
