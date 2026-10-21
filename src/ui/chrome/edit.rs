//! Edit menu commands follow the active piano tool's selection domain.
use crate::{
    app::App,
    ui::{lanes, phonemes, piano},
};
use eframe::egui::{Context, Event, ViewportCommand};

fn anchors(app: &App) -> bool {
    app.piano_tool == piano::Tool::PitchAnchor
}

pub(super) fn selected(app: &App) -> bool {
    if phonemes::phoneme_active(app) {
        false
    } else if lanes::parameter_active(app) {
        lanes::parameter_has_selection(app)
    } else if anchors(app) {
        piano::anchor_has_selection(app)
    } else {
        !app.editor.selected.is_empty()
    }
}

pub(crate) fn can_paste(app: &App) -> bool {
    if phonemes::phoneme_active(app) {
        false
    } else if lanes::parameter_active(app) {
        lanes::parameter_can_paste(app)
    } else if anchors(app) {
        piano::anchor_has_clipboard(app)
    } else {
        !app.editor.current_track().is_instrumental() && !app.editor.is_editing()
    }
}

pub(super) fn select_all(app: &mut App) {
    if phonemes::phoneme_active(app) {
        return;
    }
    if lanes::parameter_active(app) {
        lanes::parameter_select_all(app);
    } else if anchors(app) {
        piano::anchor_select_all(app);
    } else {
        app.editor.select_all_notes();
    }
}

pub(super) fn clear(app: &mut App) {
    if phonemes::phoneme_active(app) {
        phonemes::phoneme_deactivate(app);
    } else if lanes::parameter_active(app) {
        lanes::parameter_clear(app);
    } else if anchors(app) {
        piano::anchor_clear(app);
    } else {
        app.editor.selected.clear();
    }
}

pub(super) fn delete(app: &mut App) {
    if phonemes::phoneme_active(app) {
        return;
    }
    if lanes::parameter_active(app) {
        lanes::parameter_delete(app);
    } else if anchors(app) {
        piano::anchor_delete(app);
    } else {
        app.editor.delete_selected();
    }
}

pub(crate) fn copy(app: &mut App, ctx: &Context) {
    if phonemes::phoneme_active(app) {
        return;
    }
    if lanes::parameter_active(app) {
        lanes::parameter_copy(app, ctx);
    } else if anchors(app) {
        piano::anchor_copy(app);
    } else {
        copy_notes(app, ctx);
    }
}

pub(crate) fn cut(app: &mut App, ctx: &Context) {
    if phonemes::phoneme_active(app) {
        return;
    }
    if lanes::parameter_active(app) {
        lanes::parameter_cut(app, ctx);
    } else if anchors(app) {
        piano::anchor_cut(app);
    } else if copy_notes(app, ctx) {
        app.editor.delete_selected();
    }
}

pub(crate) fn paste(app: &mut App, ctx: &Context) {
    if phonemes::phoneme_active(app) {
        return;
    }
    if lanes::parameter_active(app) {
        lanes::parameter_paste(app, ctx);
    } else if anchors(app) {
        piano::anchor_paste(app);
    } else if can_paste(app) {
        // Menus and the platform shortcut read the same system clipboard.
        ctx.send_viewport_cmd(ViewportCommand::RequestPaste);
    }
}

fn copy_notes(app: &mut App, ctx: &Context) -> bool {
    match app.editor.copy() {
        Ok(Some(text)) => {
            ctx.copy_text(text);
            app.status = "已复制音符".into();
            true
        }
        Ok(None) => false,
        Err(error) => {
            app.status = error;
            false
        }
    }
}

/// Clipboard commands arrive as platform events, without C/X/V key events.
/// The caller leaves these events to text editors and dialogs while focused.
pub(super) fn clipboard_events(app: &mut App, ctx: &Context) -> bool {
    if anchors(app) && !lanes::parameter_active(app) && !phonemes::phoneme_active(app) {
        return false;
    }
    let events = ctx.input_mut(|input| {
        let mut commands = Vec::new();
        input.events.retain(|event| {
            if matches!(event, Event::Copy | Event::Cut | Event::Paste(_)) {
                commands.push(event.clone());
                false
            } else {
                true
            }
        });
        commands
    });
    let handled = !events.is_empty();
    if phonemes::phoneme_active(app) {
        return handled;
    }
    for event in events {
        match event {
            Event::Copy => copy(app, ctx),
            Event::Cut => cut(app, ctx),
            Event::Paste(text) => {
                if lanes::parameter_active(app) {
                    lanes::parameter_paste_text(app, &text);
                } else {
                    let at = app.editor.transport.position;
                    let result = app.editor.paste_text(&text, at);
                    report_paste(app, result);
                }
            }
            _ => unreachable!(),
        }
    }
    handled
}

pub(super) fn duplicate(app: &mut App) {
    if phonemes::phoneme_active(app) {
        return;
    }
    if lanes::parameter_active(app) {
        lanes::parameter_duplicate(app);
        return;
    }
    let result = app.editor.duplicate_selected();
    report_paste(app, result);
}

fn report_paste(app: &mut App, result: Result<usize, String>) {
    match result {
        Ok(0) => {}
        Ok(count) => app.status = format!("已粘贴 {count} 个音符"),
        Err(error) => app.status = error,
    }
}

pub(super) fn transpose(app: &mut App, up: bool) {
    if lanes::parameter_active(app) || phonemes::phoneme_active(app) {
        return;
    }
    if anchors(app) {
        piano::anchor_shift_octave(app, up);
    } else {
        app.editor.transpose(if up { 12 } else { -12 });
    }
}

pub(super) fn can_transpose(app: &App) -> bool {
    !lanes::parameter_active(app) && !phonemes::phoneme_active(app) && selected(app)
}
