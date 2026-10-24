use super::{text, weak};
use crate::synthesis::runtime::{ContextImportDialog, ContextImportPhase, ContextImportSource};
use eframe::egui::*;
use zeroize::Zeroize;

pub(super) fn show(ui: &mut Ui, dialog: &mut ContextImportDialog) -> bool {
    match dialog.source {
        ContextImportSource::SessionFile => session_file(ui, dialog),
        ContextImportSource::File => file(ui, dialog),
        ContextImportSource::Manual => manual(ui, dialog),
        ContextImportSource::Online => online(ui, dialog),
    }
}

fn session_file(ui: &mut Ui, dialog: &mut ContextImportDialog) -> bool {
    text(
        ui,
        crate::i18n::t("加载 session。"),
    );
    weak(
        ui,
        crate::i18n::t("点击“导入”后保存此声库的上下文和所选 session 路径。加载声库时重新读取该文件并自动选择时长条件。"),
    );
    if let Some(path) = &dialog.session_path {
        let _ = path;
        ui.add(Label::new("session").wrap());
    } else {
        weak(ui, crate::i18n::t("尚未选择 session"));
    }
    let mut changed = false;
    if ui.button(crate::i18n::t("加载 session…")).clicked() {
        let mut picker = rfd::FileDialog::new().set_title(crate::i18n::t("加载 session"));
        if let Some(parent) = dialog.session_path.as_ref().and_then(|path| path.parent()) {
            picker = picker.set_directory(parent);
        }
        if let Some(path) = picker.pick_file() {
            dialog.session_path = Some(path);
            dialog.session_key.zeroize();
            dialog.session_key_required = false;
            changed = true;
        }
    }
    if dialog.session_key_required {
        text(ui, crate::i18n::t("session 自动解密失败。请输入 session 解密密钥后重试。"));
        weak(ui, crate::i18n::t("此密钥只用于解密 session，不是时长模型密钥；不会保存。"));
        changed |= ui.add(TextEdit::singleline(&mut dialog.session_key)
            .password(true).hint_text(crate::i18n::t("十进制整数或 0x 十六进制"))
            .desired_width(f32::INFINITY)).changed();
    }
    changed
}

fn file(ui: &mut Ui, dialog: &mut ContextImportDialog) -> bool {
    text(ui, crate::i18n::t("选择包含此声库及对应模型代际的上下文 JSON 文件。"));
    if let Some(path) = &dialog.file_path {
        ui.add(Label::new(path.display().to_string()).wrap());
    } else {
        weak(ui, crate::i18n::t("尚未选择文件"));
    }
    if !ui.button(crate::i18n::t("选择上下文文件…")).clicked() {
        return false;
    }
    let mut picker = rfd::FileDialog::new()
        .set_title(crate::i18n::t("选择模型上下文文件"))
        .add_filter(crate::i18n::t("模型上下文 JSON"), &["json"]);
    if let Some(parent) = dialog.file_path.as_ref().and_then(|path| path.parent()) {
        picker = picker.set_directory(parent);
    }
    if let Some(path) = picker.pick_file() {
        dialog.file_path = Some(path);
        return true;
    }
    false
}

fn manual(ui: &mut Ui, dialog: &mut ContextImportDialog) -> bool {
    text(ui, crate::i18n::t("填写此声库及对应模型代际的 identity 模型条件值。"));
    ui.add(
        TextEdit::singleline(&mut dialog.manual_identity)
            .desired_width(f32::INFINITY)
            .hint_text(crate::i18n::t("十进制整数或 0x 十六进制"))
            .password(true),
    )
    .changed()
}

fn online(ui: &mut Ui, dialog: &mut ContextImportDialog) -> bool {
    text(
        ui,
        crate::i18n::t("打开并登录 Synthesizer V Studio 2，选择其进程后导入。完成后可关闭原编辑器。"),
    );
    if dialog.phase == ContextImportPhase::Discovering {
        ui.horizontal(|ui| {
            ui.spinner();
            text(ui, crate::i18n::t("正在查找已打开的编辑器…"));
        });
        return false;
    }
    if dialog.sources.is_empty() {
        weak(ui, crate::i18n::t("未找到可用的编辑器进程。打开原编辑器后点击刷新。"));
        return false;
    }
    let mut changed = false;
    ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
        for source in &dialog.sources {
            changed |= ui
                .radio_value(
                    &mut dialog.selected_process,
                    Some(source.process_id),
                    format!("{} · PID {}", source.label, source.process_id),
                )
                .changed();
        }
    });
    changed
}
