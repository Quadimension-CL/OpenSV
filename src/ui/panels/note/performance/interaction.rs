use crate::{app::App, editor::Editor, editor::PerformanceParameter};
use eframe::egui::*;

#[derive(Clone, Copy)]
struct Gesture {
    widget: Id,
    frame: u64,
}

fn gesture_id() -> Id {
    Id::new("note-performance-gesture")
}

pub(super) fn blocked(app: &App) -> bool {
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
}

pub(super) fn apply(
    app: &mut App,
    ctx: &Context,
    response: &Response,
    parameter: PerformanceParameter,
    update: impl FnOnce(&mut Editor),
) {
    let mut gesture = ctx.data_mut(|data| data.get_temp::<Gesture>(gesture_id()));
    if response.changed() {
        if !app.editor.has_performance_edit() {
            app.commit_lyric();
            if !app.editor.begin_performance_edit(parameter) {
                return;
            }
            gesture = Some(Gesture {
                widget: response.id,
                frame: ctx.cumulative_frame_nr(),
            });
        }
        if gesture.is_some_and(|gesture| gesture.widget == response.id) {
            update(&mut app.editor);
            app.status.clear();
        }
    }
    if let Some(mut gesture) = gesture.filter(|gesture| gesture.widget == response.id) {
        gesture.frame = ctx.cumulative_frame_nr();
        ctx.data_mut(|data| data.insert_temp(gesture_id(), gesture));
    }
}

pub(crate) fn end_frame(app: &mut App, ctx: &Context) {
    let gesture = ctx.data_mut(|data| data.get_temp::<Gesture>(gesture_id()));
    let Some(gesture) = gesture else {
        return;
    };
    let cancel = ctx.input(|input| input.key_pressed(Key::Escape));
    let finished = !app.editor.has_performance_edit()
        || !app.editor.performance_edit_matches_selection()
        || gesture.frame != ctx.cumulative_frame_nr()
        || blocked(app)
        || Popup::is_any_open(ctx)
        || ctx.input(|input| {
            !input.focused
                || !input.pointer.has_pointer()
                || !input.pointer.button_down(PointerButton::Primary)
        });
    if cancel || finished {
        if cancel {
            app.editor.cancel_performance_edit();
        } else {
            app.editor.finish_performance_edit();
        }
        ctx.data_mut(|data| data.remove::<Gesture>(gesture_id()));
        if ctx.dragged_id() == Some(gesture.widget) {
            ctx.stop_dragging();
        }
        ctx.request_repaint();
    }
}
