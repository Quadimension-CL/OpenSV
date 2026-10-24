use crate::{app::App, editor::GroupEdit, model::*};
use eframe::egui::*;

pub struct GroupGesture {
    pub original: GroupRef,
    pub source_track: EntityId,
    pub origin: Pos2,
    pub kind: GroupEdit,
    pub beat_width: f32,
    pub arrangement: bool,
}
pub fn start(
    app: &mut App,
    original: &GroupRef,
    track: EntityId,
    origin: Pos2,
    kind: GroupEdit,
    beat_width: f32,
    arrangement: bool,
) {
    app.commit_lyric();
    app.editor.select_reference(track, original.id);
    app.editor.begin(match kind {
        GroupEdit::Move => crate::i18n::t("移动音符组"),
        _ => crate::i18n::t("调整音符组范围"),
    });
    app.group_gesture = Some(GroupGesture {
        original: original.clone(),
        source_track: track,
        origin,
        kind,
        beat_width,
        arrangement,
    });
}
pub fn update(app: &mut App, ctx: &Context, target_track: Option<EntityId>) {
    let Some(drag) = &app.group_gesture else {
        return;
    };
    let Some(pointer) = ctx.input(|i| i.pointer.interact_pos()) else {
        return;
    };
    let raw = ticks((pointer.x - drag.origin.x) as f64 / drag.beat_width as f64);
    let anchor = match drag.kind {
        GroupEdit::Right => drag.original.visible_end(),
        _ => drag.original.blick_absolute_begin,
    };
    let delta = if app.view.snap && !ctx.input(|i| i.modifiers.alt) {
        ticks(app.view.snap_beat(beats(anchor.saturating_add(raw)))) - anchor
    } else {
        raw
    };
    let target = if matches!(drag.kind, GroupEdit::Move) {
        target_track.unwrap_or(app.editor.track)
    } else {
        drag.source_track
    };
    let minimum = if app.view.snap {
        ticks(app.view.step())
    } else {
        ticks(0.01)
    };
    app.editor
        .edit_group(&drag.original, target, drag.kind, delta, minimum);
}
pub fn finish(app: &mut App) {
    app.editor.finish();
    app.group_gesture = None;
}
pub fn context_menu(app: &mut App, ui: &mut Ui, reference: &GroupRef) {
    if ui.button(crate::i18n::t("重命名音符组…")).clicked() {
        if let Some(group) = app.editor.project.group(&reference.group_id) {
            app.rename = Some(super::rename::Rename::group(reference.id, &group.name));
        }
        ui.close();
    }
    let ordinary = app
        .editor
        .current_track()
        .groups
        .iter()
        .any(|r| r.id == reference.id);
    if ui
        .add_enabled(ordinary, Button::new(crate::i18n::t("删除音符组")))
        .clicked()
    {
        app.editor.delete_group(reference.id);
        ui.close();
    }
}
