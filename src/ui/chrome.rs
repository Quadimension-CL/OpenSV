mod edit;
use super::state::Column;
use crate::{
    app::{App, FileAction, MidiImportMode},
    model::*,
};
use eframe::egui::*;

fn item(ui: &mut Ui, text: &str, shortcut: &str, enabled: bool) -> bool {
    let response = ui.add_enabled(
        enabled,
        Button::new(text)
            .shortcut_text(shortcut)
            .min_size(vec2(180.0, 22.0)),
    );
    if response.clicked() {
        ui.close();
        true
    } else {
        false
    }
}
pub fn menus(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    MenuBar::new().ui(ui, |ui| {
        ui.spacing_mut().button_padding = vec2(11.0, 4.0);
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.menu_button("文件", |ui| {
            if item(ui, "新建", "Ctrl+N", true) {
                app.request_file(FileAction::New, &ctx);
            }
            if item(ui, "打开…", "Ctrl+O", true) {
                app.request_file(FileAction::Open, &ctx);
            }
            ui.separator();
            if item(ui, "保存", "Ctrl+S", true) {
                app.save_project(false);
            }
            if item(ui, "另存为…", "Ctrl+Shift+S", true) {
                app.save_project(true);
            }
            ui.separator();
            if item(ui, "导入…", "Ctrl+Shift+I", true) {
                app.request_file(FileAction::ImportMidi, &ctx);
            }
            if item(ui, "作为音轨导入…", "", true) {
                app.begin_midi_import(MidiImportMode::Tracks, &ctx);
            }
            if item(ui, "导入音频…", "", true) {
                app.import_audio(None, &ctx);
            }
            item(ui, "导出音频", "", false);
        });
        ui.menu_button("编辑", |ui| {
            if item(
                ui,
                &format!("撤销 {}", app.editor.undo_label()),
                "Ctrl+Z",
                app.editor.can_undo(),
            ) {
                app.editor.undo();
            }
            if item(ui, "重做", "Ctrl+Y", app.editor.can_redo()) {
                app.editor.redo();
            }
            ui.separator();
            if item(ui, "全选", "Ctrl+A", true) {
                edit::select_all(app);
            }
            if item(ui, "取消选择", "Esc", true) {
                edit::clear(app);
            }
            let selected = edit::selected(app);
            if item(ui, "删除所选内容", "Delete", selected) {
                edit::delete(app);
            }
            ui.separator();
            if item(ui, "复制", "Ctrl+C", selected) {
                edit::copy(app);
            }
            if item(ui, "剪切", "Ctrl+X", selected) {
                edit::cut(app);
            }
            if item(ui, "粘贴", "Ctrl+V", edit::can_paste(app)) {
                edit::paste(app);
            }
        });
        ui.menu_button("查看", |ui| {
            ui.checkbox(&mut app.show_time_tracks, "显示曲速和拍号轨");
            if ui
                .checkbox(&mut app.view.arrangement_collapsed, "折叠编排")
                .clicked()
            {
                ui.close();
            }
            ui.checkbox(&mut app.view.arrangement_first, "编排位于上方");
            ui.separator();
            for column in Column::PANELS {
                let mut active = app.view.columns.contains(&column);
                if ui.checkbox(&mut active, column.label()).clicked() {
                    app.view.toggle(column);
                }
            }
            ui.separator();
            if item(ui, "设置", "", true) {
                app.settings = true;
            }
        });
        ui.menu_button("修改", |ui| {
            let selected = app.piano_tool != super::piano::Tool::PitchAnchor
                && !app.editor.selected.is_empty();
            if item(ui, "填入歌词…", "Ctrl+L", selected) {
                super::fill_lyrics::open(app);
            }
            if item(ui, "静音音符", "Ctrl+M", selected) {
                app.editor.mute_selected();
            }
            ui.separator();
            if item(ui, "升高八度", "Ctrl+↑", edit::selected(app)) {
                edit::transpose(app, true);
            }
            if item(ui, "降低八度", "Ctrl+↓", edit::selected(app)) {
                edit::transpose(app, false);
            }
            item(ui, "智能量化", "", false);
            item(ui, "解除组链接", "", false);
        });
        ui.menu_button("自动处理", |ui| {
            item(ui, "装饰所选音符", "", false);
            super::retakes::commands(app, ui);
            item(ui, "从音频提取音符", "", false);
            item(ui, "检测曲速", "", false);
        });
        ui.menu_button("项目", |ui| {
            if item(ui, "创建音轨", "", true) {
                app.editor.add_track();
            }
            if item(
                ui,
                "在走带处插入新组",
                "",
                !app.editor.current_track().is_instrumental(),
            ) {
                app.editor.add_group();
            }
            if item(ui, "创建伴奏轨", "", true) {
                let id = app.editor.add_instrumental_track();
                app.import_audio(Some(id), &ctx);
            }
            item(ui, "设置歌声", "", false);
        });
        ui.menu_button("走带控制", |ui| {
            if item(ui, "播放 / 暂停", "Space", true) {
                app.editor.transport.toggle();
            }
            if item(ui, "停止", "Ctrl+Space", true) {
                app.editor.transport.stop();
            }
            ui.checkbox(&mut app.editor.transport.looping, "循环播放所选音符");
            if item(ui, "跳到开头", "", true) {
                app.editor.transport.position = 0;
            }
            if item(ui, "跳到结尾", "Ctrl+End", true) {
                app.editor.transport.position = app.editor.project.end();
            }
        });
        ui.menu_button("脚本", |ui| {
            item(ui, "重新扫描", "", false);
            item(ui, "打开脚本文件夹", "", false);
        });
        ui.menu_button("其他", |ui| {
            if item(ui, "关于 RynthV", "", true) {
                app.about = true;
            }
        });
    });
}
fn paste(app: &mut App) {
    app.editor.paste(app.editor.transport.position);
}
pub fn shortcuts(app: &mut App, ctx: &Context) {
    if app.editor.has_performance_edit() {
        return;
    }
    if app.audio_drag.is_some() {
        return;
    }
    if app.view_pan.is_active() {
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            app.view_pan.cancel(ctx);
        }
        return;
    }
    if ctx.egui_wants_keyboard_input() || app.lyric.is_some() {
        return;
    }
    let command = |key, modifiers| ctx.input_mut(|i| i.consume_key(modifiers, key));
    let ctrl = Modifiers::CTRL;
    let shift = Modifiers::CTRL | Modifiers::SHIFT;
    if matches!(app.gesture, Some(super::piano::Gesture::Anchor(_)))
        && super::piano::anchor_keyboard(app, ctx)
    {
        return;
    }
    if app.gesture.is_some() || app.group_gesture.is_some() {
        if command(Key::Escape, Modifiers::NONE) {
            app.editor.cancel();
            app.gesture = None;
            app.group_gesture = None;
        }
        return;
    }
    if command(Key::S, shift) {
        app.save_project(true);
    } else if command(Key::S, ctrl) {
        app.save_project(false);
    }
    if command(Key::I, shift) {
        app.request_file(FileAction::ImportMidi, ctx);
        return;
    }
    if command(Key::N, ctrl) {
        app.request_file(FileAction::New, ctx);
    }
    if command(Key::O, ctrl) {
        app.request_file(FileAction::Open, ctx);
    }
    if command(Key::Z, shift) || command(Key::Y, ctrl) {
        app.editor.redo();
    } else if command(Key::Z, ctrl) {
        app.editor.undo();
    }
    if super::piano::anchor_keyboard(app, ctx) {
        return;
    }
    if command(Key::T, Modifiers::ALT | Modifiers::SHIFT) {
        super::retakes::prune(app);
        return;
    } else if command(Key::T, Modifiers::ALT) {
        super::retakes::create(app, RetakeComponent::All);
        return;
    }
    if command(Key::A, ctrl) {
        app.editor.select_all_notes();
    }
    if command(Key::C, ctrl) {
        app.editor.copy();
    }
    if command(Key::X, ctrl) {
        app.editor.copy();
        app.editor.delete_selected();
    }
    if command(Key::V, ctrl) {
        paste(app);
    }
    if command(Key::D, ctrl) {
        app.editor.copy();
        let at = app
            .editor
            .clipboard
            .iter()
            .map(|n| n.onset + n.duration)
            .max()
            .unwrap_or(0);
        app.editor
            .paste(at + app.editor.active_ref().map_or(0, |r| r.blick_offset));
    }
    if app.piano_tool != super::piano::Tool::PitchAnchor && command(Key::L, ctrl) {
        super::fill_lyrics::open(app);
        return;
    }
    if app.piano_tool != super::piano::Tool::PitchAnchor && command(Key::M, ctrl) {
        app.editor.mute_selected();
    }
    if command(Key::B, ctrl) {
        app.view.toggle(Column::Note);
    }
    if command(Key::ArrowUp, ctrl) {
        app.editor.transpose(12);
    }
    if command(Key::ArrowDown, ctrl) {
        app.editor.transpose(-12);
    }
    if command(Key::Delete, Modifiers::NONE) {
        app.editor.delete_selected();
    }
    if command(Key::Escape, Modifiers::NONE) {
        app.editor.selected.clear();
    }
    if command(Key::Num1, Modifiers::ALT) {
        app.piano_tool = super::piano::Tool::NoteSelect;
        app.status.clear();
    }
    if command(Key::Num2, Modifiers::ALT) {
        app.piano_tool = super::piano::Tool::NoteDraw;
        app.status.clear();
    }
    if command(Key::Num3, Modifiers::ALT) {
        app.commit_lyric();
        app.editor.selected.clear();
        app.piano_tool = super::piano::Tool::PitchAnchor;
        app.show_pitch = true;
        app.status.clear();
    }
    if command(Key::Num4, Modifiers::ALT) {
        app.commit_lyric();
        app.piano_tool = super::piano::Tool::PitchDraw;
        app.show_pitch = true;
        app.status.clear();
    }
    if command(Key::Space, ctrl) {
        app.editor.transport.stop();
    } else if command(Key::Space, Modifiers::NONE) {
        app.editor.transport.toggle();
    }
    if command(Key::End, ctrl) {
        app.editor.transport.position = app.editor.project.end();
    }
    for (key, direction) in [(Key::ArrowLeft, -1), (Key::ArrowRight, 1)] {
        if command(key, Modifiers::NONE) {
            app.editor.transport.position =
                (app.editor.transport.position + direction * ticks(app.view.step())).max(0);
        }
    }
}
