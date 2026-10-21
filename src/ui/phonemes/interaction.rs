mod edit;

use super::*;
use std::sync::Arc;

pub(super) struct Capture {
    pub mapping: Mapping,
    pub original: Arc<[RenderedPhoneme]>,
    pub preview: Vec<RenderedPhoneme>,
    pub track: EntityId,
    pub reference: GroupRef,
    pub index: usize,
    pub handle: Handle,
    pub source: SourcePhone,
    pub attributes: PhonemeAttributes,
    pub origin: Pos2,
    pub frame: u64,
    pub changed: bool,
    pub dragged: bool,
}

pub(super) fn right_source(phones: &[RenderedPhoneme], index: usize) -> Option<usize> {
    let phone = phones.get(index)?;
    let next = phones.get(index + 1)?;
    (next.supports_timing && (phone.end_seconds - next.begin_seconds).abs() < 1e-7)
        .then_some(index + 1)
}

pub(super) fn hit(
    mapping: &Mapping,
    phones: &[RenderedPhoneme],
    pos: Pos2,
) -> Option<(usize, Handle)> {
    if !mapping.clip().contains(pos) {
        return None;
    }
    let boundary = phones
        .iter()
        .enumerate()
        .flat_map(|(index, phone)| {
            let rect = mapping.phone_rect(phone);
            [(rect.left(), Handle::Left), (rect.right(), Handle::Right)]
                .into_iter()
                .map(move |(x, handle)| (index, handle, (x - pos.x).abs()))
        })
        .filter(|(_, _, distance)| *distance < 3.0)
        .min_by(|left, right| left.2.total_cmp(&right.2));
    if let Some((index, handle, _)) = boundary {
        return Some((index, handle));
    }
    if pos.y <= mapping.content().top() {
        return None;
    }
    phones.iter().enumerate().find_map(|(index, phone)| {
        let rect = mapping.phone_rect(phone);
        (rect.left() <= pos.x && pos.x < rect.right())
            .then(|| {
                if phone.supports_impulse {
                    Some((index, Handle::Impulse))
                } else {
                    phone.strength.map(|_| (index, Handle::Strength))
                }
            })
            .flatten()
    })
}

fn blocked(app: &App, ctx: &Context) -> bool {
    !app.view.phonemes
        || app.piano_collapsed
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

pub(super) fn show(
    app: &mut App,
    ui: &Ui,
    response: &Response,
    mapping: &Mapping,
    phones: &[RenderedPhoneme],
) {
    if !ui.is_enabled() || !ui.input(|input| input.focused) || blocked(app, ui.ctx()) {
        return;
    }
    if response.double_clicked_by(PointerButton::Primary)
        && let Some(pos) = response.interact_pointer_pos()
        && let Some((index, handle)) = hit(mapping, phones, pos)
    {
        finish(app, true);
        if let Err(error) = reset(app, phones, index, handle) {
            app.error = Some(error);
        }
        ui.ctx().stop_dragging();
        return;
    }
    if let Some(pos) = response.hover_pos()
        && let Some((index, handle)) = hit(mapping, phones, pos)
    {
        let enabled = match handle {
            Handle::Left => phones[index].supports_timing,
            Handle::Right => right_source(phones, index).is_some(),
            Handle::Strength | Handle::Impulse => true,
        };
        ui.ctx().set_cursor_icon(if !enabled {
            CursorIcon::Default
        } else if matches!(handle, Handle::Left | Handle::Right) {
            CursorIcon::ResizeHorizontal
        } else if handle == Handle::Strength {
            CursorIcon::ResizeVertical
        } else {
            CursorIcon::Move
        });
        if !enabled {
            response.clone().on_hover_text(if handle == Handle::Right {
                "尾部边界随音符长度调整"
            } else {
                "当前声库暂不支持音素时值编辑"
            });
        }
    }
    for event in ui.input(|input| input.events.clone()) {
        match event {
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                ..
            } if !phoneme_dragging(app)
                && app.gesture.is_none()
                && app.group_gesture.is_none()
                && mapping.clip().contains(pos)
                && ui.clip_rect().contains(pos) =>
            {
                focus(app, ui.ctx());
                if let Some((index, handle)) = hit(mapping, phones, pos) {
                    begin(app, ui, mapping, phones, pos, index, handle);
                }
            }
            Event::PointerMoved(pos) if phoneme_dragging(app) => {
                extend(app, pos);
            }
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                ..
            } if phoneme_dragging(app) => {
                extend(app, pos);
                finish(app, false);
                ui.ctx().stop_dragging();
            }
            _ => {}
        }
    }
    if let Some(capture) = &mut app.phoneme_editing.capture {
        capture.frame = ui.ctx().cumulative_frame_nr();
        ui.ctx().set_dragged_id(response.id);
    }
}

fn reset(
    app: &mut App,
    phones: &[RenderedPhoneme],
    index: usize,
    handle: Handle,
) -> Result<(), String> {
    if app.gesture.is_some() || app.group_gesture.is_some() || app.editor.is_editing() {
        return Ok(());
    }
    let target = match handle {
        Handle::Left if phones[index].supports_timing => Some(index),
        Handle::Right => right_source(phones, index),
        Handle::Left => None,
        _ => Some(index),
    };
    let Some(target) = target else {
        return Ok(());
    };
    let source = phones[target].source;
    if matches!(handle, Handle::Left | Handle::Right) {
        return app
            .editor
            .reset_phoneme_attributes(source, Some(PhonemeParameter::LeftOffset));
    }
    let mut attributes = app.editor.phoneme_attributes(source)?;
    attributes.strength = None;
    attributes.activity = None;
    attributes.position = None;
    if !app.editor.begin_phoneme_edit(source) {
        return Err("无法编辑当前音素".into());
    }
    if app.editor.update_phoneme_attributes(source, attributes) {
        app.editor.finish_phoneme_edit();
        Ok(())
    } else {
        app.editor.cancel_phoneme_edit();
        Err("无法重置当前音素的强度和爆破设置".into())
    }
}

fn begin(
    app: &mut App,
    ui: &Ui,
    mapping: &Mapping,
    phones: &[RenderedPhoneme],
    origin: Pos2,
    index: usize,
    handle: Handle,
) {
    let target = match handle {
        Handle::Right => {
            let Some(target) = right_source(phones, index) else {
                return;
            };
            target
        }
        Handle::Left if !phones[index].supports_timing => return,
        _ => index,
    };
    let source = phones[target].source;
    let Ok(attributes) = app.editor.phoneme_attributes(source) else {
        return;
    };
    let Some(reference) = app.editor.active_ref().cloned() else {
        return;
    };
    let fresh = app.editor.group().is_some_and(|group| {
        mapping.input.matches_project(
            &app.editor.project,
            app.editor.current_track(),
            &reference,
            group,
        )
    });
    if !mapping.same_reference(&reference) || !fresh || !app.editor.begin_phoneme_edit(source) {
        return;
    }
    app.phoneme_editing.capture = Some(Capture {
        mapping: mapping.clone(),
        original: phones.to_vec().into(),
        preview: phones.to_vec(),
        track: app.editor.track,
        reference,
        index,
        handle,
        source,
        attributes,
        origin,
        frame: ui.ctx().cumulative_frame_nr(),
        changed: false,
        dragged: false,
    });
}

fn extend(app: &mut App, pointer: Pos2) {
    let Some(mut capture) = app.phoneme_editing.capture.take() else {
        return;
    };
    if !app.editor.phoneme_edit_matches_selection() {
        app.editor.finish_phoneme_edit();
        return;
    }
    if !capture.dragged && pointer.distance_sq(capture.origin) < 1.0 {
        app.phoneme_editing.capture = Some(capture);
        return;
    }
    capture.dragged = true;
    match edit::apply(app, &mut capture, pointer) {
        Ok(()) => app.phoneme_editing.capture = Some(capture),
        Err(error) => {
            app.editor.cancel_phoneme_edit();
            app.error = Some(error);
        }
    }
}

pub(super) fn finish(app: &mut App, cancel: bool) {
    let Some(capture) = app.phoneme_editing.capture.take() else {
        return;
    };
    if cancel || !capture.changed {
        app.editor.cancel_phoneme_edit();
    } else {
        app.editor.finish_phoneme_edit();
    }
}

pub(super) fn end_frame(app: &mut App, ctx: &Context) {
    let Some(capture) = &app.phoneme_editing.capture else {
        if !phoneme_active(app) {
            app.phoneme_editing.focused = None;
        }
        return;
    };
    let cancel = !ctx.input(|input| input.focused);
    let end = blocked(app, ctx)
        || capture.frame != ctx.cumulative_frame_nr()
        || capture.track != app.editor.track
        || app.editor.active_ref() != Some(&capture.reference)
        || !app.editor.phoneme_edit_matches_selection()
        || !ctx.input(|input| input.pointer.primary_down());
    if cancel || end {
        finish(app, cancel);
        ctx.stop_dragging();
    }
}
