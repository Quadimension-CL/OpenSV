mod artwork;
mod context;
mod language;
mod management;
mod panel;
mod paths;
mod selector;
mod widgets;

use super::theme::*;
use crate::{
    app::App,
    synthesis::voice::{VoiceCatalogEntry, VoiceFamily},
};
use eframe::egui::*;

pub use management::dialogs;
pub use panel::show;
pub use paths::settings;
pub use selector::popup;

fn selected_entry(app: &App) -> Option<&VoiceCatalogEntry> {
    app.synthesis.library.entry(&app.voice_selection()?)
}

fn family_name(entry: &VoiceCatalogEntry) -> &'static str {
    match entry.family {
        Some(VoiceFamily::Sv1) => "SV1 原生",
        Some(VoiceFamily::Sv2Compatible) => "SV2 · 版本 1 兼容",
        Some(VoiceFamily::Sv2) => "SV2",
        None => "未知格式",
    }
}

fn text(ui: &mut Ui, value: impl Into<WidgetText>) {
    ui.add(Label::new(value).selectable(false).wrap());
}

fn weak(ui: &mut Ui, value: impl Into<String>) {
    text(ui, RichText::new(value).color(MUTED));
}

fn error_label(ui: &mut Ui, value: impl Into<String>) {
    text(
        ui,
        RichText::new(value).color(Color32::from_rgb(218, 142, 126)),
    );
}

fn language_name(language: &str) -> &str {
    match language {
        "mandarin" | "mandarin-xsampa" | "zh" | "ZH" => "普通话",
        "english" | "english-arpabet" | "en" | "EN" => "英语",
        "japanese" | "japanese-romaji" | "ja" | "JA" => "日语",
        "spanish" | "spanish-xsampa" | "es" | "ES" => "西班牙语",
        "cantonese" | "cantonese-xsampa" | "yue" | "YUE" => "粤语",
        "korean" | "korean-xsampa" | "ko" | "KO" => "韩语",
        other => other,
    }
}
