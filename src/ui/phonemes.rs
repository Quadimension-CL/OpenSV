mod geometry;
mod interaction;
mod menu;
mod paint;

use crate::{
    app::App,
    model::*,
    synthesis::{
        plan::OwnerId,
        rendered::{PhonemeSnapshot, RenderedPhoneme},
    },
    ui::{theme::*, timeline::*},
};
use eframe::egui::*;
use geometry::Mapping;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Handle {
    Left,
    Right,
    Strength,
    Impulse,
}

#[derive(Default)]
pub struct PhonemeEditing {
    capture: Option<interaction::Capture>,
    focused: Option<(EntityId, EntityId)>,
}

pub fn phoneme_active(app: &App) -> bool {
    app.view.phonemes
        && !app.piano_collapsed
        && app
            .phoneme_editing
            .focused
            .is_some_and(|(track, reference)| {
                track == app.editor.track && app.editor.reference == Some(reference)
            })
}

pub fn phoneme_dragging(app: &App) -> bool {
    app.phoneme_editing.capture.is_some()
}

pub fn phoneme_cancel(app: &mut App, ctx: &Context) {
    interaction::finish(app, true);
    ctx.stop_dragging();
}

pub fn phoneme_deactivate(app: &mut App) {
    interaction::finish(app, false);
    app.phoneme_editing.focused = None;
}

pub fn end_phoneme_frame(app: &mut App, ctx: &Context) {
    interaction::end_frame(app, ctx);
}

fn focus(app: &mut App, ctx: &Context) {
    app.commit_lyric();
    super::lanes::parameter_deactivate(app);
    super::piano::anchor_clear(app);
    ctx.memory_mut(|memory| {
        if let Some(id) = memory.focused() {
            memory.surrender_focus(id);
        }
    });
    app.phoneme_editing.focused = app
        .editor
        .reference
        .map(|reference| (app.editor.track, reference));
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, PANEL);
    let canvas = Rect::from_min_max(pos2(rect.left() + GUTTER, rect.top()), rect.max);
    if canvas.width() < 4.0 || canvas.height() < 42.0 {
        return;
    }
    ui.painter().text(
        pos2(rect.left() + GUTTER * 0.5, rect.top() + 16.0),
        Align2::CENTER_CENTER,
        "æ",
        FontId::proportional(18.0),
        TEXT,
    );
    ui.painter().rect_filled(canvas, 0, CANVAS);
    let response = ui.interact(
        canvas,
        Id::new("phoneme-timing-canvas"),
        Sense::click_and_drag(),
    );
    if !phoneme_dragging(app) {
        scroll(ui, &response, canvas, &mut app.view.piano);
        let delta = app.view_pan.drag(
            ui,
            super::view_pan::Target::Phonemes,
            canvas,
            app.gesture.is_none() && app.group_gesture.is_none() && !app.editor.is_editing(),
        );
        app.view.piano.start = (app.view.piano.start
            - f64::from(delta.x) / f64::from(app.view.piano.beat_width))
        .max(0.0);
    }
    grid(
        ui,
        canvas,
        &app.view.piano,
        &app.editor.project.time,
        app.view.step(),
    );
    if ui.rect_contains_pointer(canvas)
        && ui.input(|input| input.pointer.any_pressed())
        && app.gesture.is_none()
        && app.group_gesture.is_none()
    {
        focus(app, ui.ctx());
    }
    let captured = app
        .phoneme_editing
        .capture
        .as_ref()
        .map(|capture| (capture.mapping.clone(), capture.original.clone()));
    let Some((mapping, phones)) = captured.or_else(|| {
        snapshot(app).map(|snapshot| {
            (
                Mapping {
                    canvas,
                    input: snapshot.input.clone(),
                    viewport: app.view.piano.clone(),
                },
                snapshot.data.phones.clone(),
            )
        })
    }) else {
        ui.painter().text(
            canvas.center(),
            Align2::CENTER_CENTER,
            "音素时值 · 等待当前音符组合成准备结果",
            FontId::proportional(12.0),
            MUTED,
        );
        return;
    };
    interaction::show(app, ui, &response, &mapping, &phones);
    let displayed = app
        .phoneme_editing
        .capture
        .as_ref()
        .map_or(phones.as_ref(), |capture| capture.preview.as_slice());
    let hovered = app
        .phoneme_editing
        .capture
        .as_ref()
        .map(|capture| (capture.index, capture.handle))
        .or_else(|| {
            response
                .hover_pos()
                .and_then(|pos| interaction::hit(&mapping, displayed, pos))
        });
    paint::show(app, ui, &mapping, displayed, hovered);
    menu::show(app, ui, &response, &mapping, &phones);
    playhead_line(ui, canvas, &app.view.piano, app.editor.transport.position);
    if response.hovered() {
        app.status = "拖动音素块调整强度 · 拖动边界调整时值 · 双击重置".into();
    }
}

fn snapshot(app: &App) -> Option<&PhonemeSnapshot> {
    let reference = app.editor.active_ref()?;
    let snapshot = app.synthesis.phonemes(OwnerId(reference.id))?;
    let group = app.editor.group()?;
    snapshot
        .input
        .matches_project(
            &app.editor.project,
            app.editor.current_track(),
            reference,
            group,
        )
        .then_some(snapshot)
}
