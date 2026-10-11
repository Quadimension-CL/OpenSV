//! Edit menu commands follow the active piano tool's selection domain.
use crate::{app::App, ui::piano};

fn anchors(app: &App) -> bool {
    app.piano_tool == piano::Tool::PitchAnchor
}

pub(super) fn selected(app: &App) -> bool {
    if anchors(app) {
        piano::anchor_has_selection(app)
    } else {
        !app.editor.selected.is_empty()
    }
}

pub(super) fn can_paste(app: &App) -> bool {
    if anchors(app) {
        piano::anchor_has_clipboard(app)
    } else {
        !app.editor.clipboard.is_empty()
    }
}

pub(super) fn select_all(app: &mut App) {
    if anchors(app) {
        piano::anchor_select_all(app);
    } else {
        app.editor.select_all_notes();
    }
}

pub(super) fn clear(app: &mut App) {
    if anchors(app) {
        piano::anchor_clear(app);
    } else {
        app.editor.selected.clear();
    }
}

pub(super) fn delete(app: &mut App) {
    if anchors(app) {
        piano::anchor_delete(app);
    } else {
        app.editor.delete_selected();
    }
}

pub(super) fn copy(app: &mut App) {
    if anchors(app) {
        piano::anchor_copy(app);
    } else {
        app.editor.copy();
    }
}

pub(super) fn cut(app: &mut App) {
    if anchors(app) {
        piano::anchor_cut(app);
    } else {
        app.editor.copy();
        app.editor.delete_selected();
    }
}

pub(super) fn paste(app: &mut App) {
    if anchors(app) {
        piano::anchor_paste(app);
    } else {
        app.editor.paste(app.editor.transport.position);
    }
}

pub(super) fn transpose(app: &mut App, up: bool) {
    if anchors(app) {
        piano::anchor_shift_octave(app, up);
    } else {
        app.editor.transpose(if up { 12 } else { -12 });
    }
}
