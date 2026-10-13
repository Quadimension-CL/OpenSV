use super::super::widgets::ControlResponse;
use crate::{
    app::App,
    editor::{Editor, VoiceEditParameter},
};
use eframe::egui::*;

#[derive(Clone, Copy)]
struct Gesture {
    widget: Id,
    frame: u64,
    active: bool,
}

fn gesture_id() -> Id {
    Id::new("voice-parameter-gesture")
}

fn blocked(app: &App) -> bool {
    app.pending.is_some()
        || app.error.is_some()
        || app.settings
        || app.voice_manager
        || app.about
        || app.rename.is_some()
        || app.time_edit.is_some()
        || app.bpm_edit.is_some()
        || app.fill_lyrics.is_some()
        || app.midi_import.is_some()
        || app.synthesis.context_import.dialog.is_some()
        || app.view_pan.is_active()
        || app.audio_drag.is_some()
}

pub(super) fn editable(app: &App, ui: &Ui) -> bool {
    !blocked(app)
        && app.gesture.is_none()
        && app.group_gesture.is_none()
        && (!app.editor.is_editing() || app.editor.has_voice_edit())
        && ui.input(|input| input.focused)
}

pub(super) fn prepare(app: &mut App, ctx: &Context) {
    if app.editor.has_voice_edit() && !app.editor.voice_edit_matches_selection() {
        finish(app, ctx, false);
    }
}

pub(super) fn apply(
    app: &mut App,
    ctx: &Context,
    widget: Id,
    response: ControlResponse,
    parameter: VoiceEditParameter,
    update: impl FnOnce(&mut Editor) -> bool,
) {
    let mut gesture = ctx.data_mut(|data| data.get_temp::<Gesture>(gesture_id()));
    if response.cancelled {
        if gesture.is_some_and(|gesture| gesture.widget == widget) {
            finish(app, ctx, true);
        }
        return;
    }
    if response.changed {
        if gesture.is_some_and(|gesture| gesture.widget != widget) {
            finish(app, ctx, false);
            gesture = None;
        }
        if !app.editor.has_voice_edit() {
            app.commit_lyric();
            if !app.editor.begin_voice_edit(parameter) {
                return;
            }
            gesture = Some(Gesture {
                widget,
                frame: ctx.cumulative_frame_nr(),
                active: response.active,
            });
        }
        if gesture.is_some_and(|gesture| gesture.widget == widget) {
            if !update(&mut app.editor) {
                finish(app, ctx, true);
                app.error = Some("无法更新当前歌声参数，请重新选择音符组后重试。".into());
                return;
            }
            app.status.clear();
        }
    }
    if let Some(mut gesture) = gesture.filter(|gesture| gesture.widget == widget) {
        gesture.frame = ctx.cumulative_frame_nr();
        gesture.active = response.active;
        ctx.data_mut(|data| data.insert_temp(gesture_id(), gesture));
        if response.finished {
            finish(app, ctx, false);
        }
    }
}

/// Close the transaction even when its panel or editing target disappears.
pub(crate) fn end_frame(app: &mut App, ctx: &Context) {
    let gesture = ctx.data_mut(|data| data.get_temp::<Gesture>(gesture_id()));
    let Some(gesture) = gesture else {
        if app.editor.has_voice_edit() {
            app.editor.finish_voice_edit();
        }
        return;
    };
    let cancel = ctx.input(|input| input.key_pressed(Key::Escape));
    let finished = !app.editor.has_voice_edit()
        || !app.editor.voice_edit_matches_selection()
        || gesture.frame != ctx.cumulative_frame_nr()
        || !gesture.active
        || blocked(app)
        || ctx.input(|input| !input.focused);
    if cancel || finished {
        finish(app, ctx, cancel);
    }
}

fn finish(app: &mut App, ctx: &Context, cancel: bool) {
    if cancel {
        app.editor.cancel_voice_edit();
    } else {
        app.editor.finish_voice_edit();
    }
    ctx.data_mut(|data| data.remove::<Gesture>(gesture_id()));
    ctx.request_repaint();
}
