mod fields;

use super::{error_label, text, weak};
use crate::{
    app::App,
    synthesis::{
        runtime::{ContextImportPhase, ContextImportSource},
        voice::{VoiceFamily, session_identity},
    },
};
use eframe::egui::*;

pub(super) fn controls(
    app: &mut App,
    ui: &mut Ui,
    selection: &crate::synthesis::settings::VoiceSelection,
) {
    if app
        .synthesis
        .library
        .entry(selection)
        .is_some_and(|entry| entry.family == Some(VoiceFamily::Sv1))
    {
        return;
    }
    if let Some(result) = &app.synthesis.context_import.feedback {
        match result {
            Ok(message) => text(ui, message),
            Err(error) => error_label(ui, error),
        }
    }
    let can_import = app
        .synthesis
        .inventory(selection)
        .is_some_and(|voice| voice.configuration.is_some());
    if can_import
        && ui
            .button("导入模型上下文…")
            .on_hover_text("为当前歌声导入上下文文件，或手动填写模型条件值")
            .clicked()
    {
        app.synthesis.open_context_import(selection.clone());
    }
}

pub(super) fn dialog(app: &mut App, ctx: &Context) {
    let Some(dialog) = &mut app.synthesis.context_import.dialog else {
        return;
    };
    let mut refresh = false;
    let mut import = false;
    let mut cancel = false;
    let mut edited = false;
    let mut source = dialog.source;
    let importing = dialog.phase == ContextImportPhase::Importing;
    let modal = Modal::new(Id::new("voice-model-context-import")).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.spacing_mut().item_spacing.y = 8.0;
        text(ui, RichText::new("导入模型上下文").strong().size(16.0));
        text(ui, format!("当前声库：{}", dialog.voice_name));
        weak(
            ui,
            if dialog.model_version == 1 {
                "版本 1 兼容歌声"
            } else {
                "版本 2 歌声"
            },
        );
        ui.add_enabled_ui(!importing, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut source, ContextImportSource::File, "上下文文件");
                ui.selectable_value(&mut source, ContextImportSource::Manual, "手动输入");
                ui.add_enabled_ui(session_identity::supports_online_import(), |ui| {
                    ui.selectable_value(&mut source, ContextImportSource::Online, "从原版获取")
                        .on_disabled_hover_text(
                            "从原版获取仅支持 Windows x64；当前平台可导入文件或手动输入",
                        );
                });
            });
        });
        ui.separator();
        ui.add_enabled_ui(!importing, |ui| {
            edited = fields::show(ui, dialog);
        });
        if edited {
            dialog.error = None;
        }
        if importing {
            ui.horizontal(|ui| {
                ui.spinner();
                text(ui, "正在导入，完成后自动重新加载此歌声…");
            });
        } else {
            weak(ui, "仅更新此歌声的上下文，完成后自动重新加载。");
        }
        if let Some(error) = &dialog.error {
            error_label(ui, error);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if dialog.source == ContextImportSource::Online {
                refresh = ui
                    .add_enabled(
                        dialog.phase == ContextImportPhase::Ready,
                        Button::new("刷新进程"),
                    )
                    .clicked();
            }
            cancel = ui.add_enabled(!importing, Button::new("取消")).clicked();
            import = ui
                .add_enabled(dialog.can_import(), Button::new("导入"))
                .clicked();
        });
    });
    cancel |= !importing
        && modal.is_top_modal
        && !modal.any_popup_open
        && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    let source_changed = source != dialog.source;
    if edited {
        app.synthesis.clear_context_import_error();
    }
    if cancel {
        app.synthesis.cancel_context_import();
    } else if source_changed {
        app.synthesis.set_context_import_source(source);
    } else if import {
        app.synthesis.import_context();
    } else if refresh {
        app.synthesis.refresh_context_sources();
    }
}
