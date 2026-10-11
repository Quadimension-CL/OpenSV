use crate::app::App;
use eframe::egui::*;

pub fn render_status(app: &mut App, ui: &mut Ui) {
    ui.label(RichText::new("歌声合成").strong());
    ui.separator();
    ui.label(app.synthesis.status());
    if let Some(progress) = &app.synthesis.progress {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(progress);
        });
    }
    let stats = app.synthesis.stats;
    ui.label(format!(
        "已完成 {} 组 · 等待 {} 组 · 失败 {} 组",
        stats.ready, stats.queued, stats.failed
    ));
    if let Some(error) = &app.synthesis.error {
        ui.colored_label(Color32::from_rgb(218, 142, 126), error);
    }
    for (selection, name) in app.synthesis.context_import_targets() {
        if ui.button(format!("为 {name} 导入模型上下文…")).clicked() {
            app.synthesis.open_context_import(selection);
        }
    }
    if !app.synthesis.failures.is_empty() {
        ui.collapsing("失败详情", |ui| {
            for (owner, error) in &app.synthesis.failures {
                ui.label(format!("音符组 {}：{error}", owner.0));
            }
        });
    }
    if (stats.failed > 0 || app.synthesis.error.is_some()) && ui.button("重试合成").clicked() {
        app.synthesis.retry();
    }
    if let Some(error) = app.synthesis.playback_error() {
        ui.colored_label(
            Color32::from_rgb(218, 142, 126),
            format!("播放设备：{error}"),
        );
    }
    ui.add_space(10.0);
    if let Some(duration) = app.synthesis.audio_duration() {
        ui.label(format!("当前混音 {:.2} 秒 · 48 kHz · 立体声", duration));
    }
    ui.collapsing("合成计划", |ui| {
        let summary = app.synthesis.planner.summary();
        ui.label(format!("{} 个音符组纳入当前计划", summary.pending_owners));
        ui.label(format!(
            "本次变化 {} 组 · 保留 {} 组请求",
            summary.changed_owners, summary.unchanged_owners
        ));
        ui.weak("优先准备播放位置的音频，其余部分在后台继续合成；无改动区段复用已有结果。");
    });
}
