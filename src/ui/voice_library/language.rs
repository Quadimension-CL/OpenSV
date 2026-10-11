use super::{error_label, widgets};
use crate::{
    app::App,
    model::{Language, database_language},
    ui::theme::*,
};
use eframe::egui::*;

pub(super) fn show(app: &mut App, ui: &mut Ui) {
    let Some(config) = app
        .voice_inventory()
        .and_then(|voice| voice.configuration.as_ref())
    else {
        widgets::field(ui, "未加载歌声", true, "加载声库后可选择其支持的语言");
        return;
    };
    let Some(native) = Language::parse(&config.default_language) else {
        error_label(
            ui,
            format!("无法识别声库默认语言：{}", config.default_language),
        );
        return;
    };
    let languages: Vec<_> = config
        .supported_languages
        .iter()
        .filter_map(|id| Language::parse(id))
        .collect();
    let track = &app.editor.current_track().main_ref;
    let target = app.editor.active_ref().unwrap_or(track);
    let original = target
        .database
        .get("languageOverride")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let mut selected = original.to_owned();
    let inherited = if target.id != track.id {
        database_language(&track.database, native)
    } else {
        Ok(native)
    };
    let mut default_database = target.database.clone();
    if let Some(database) = default_database.as_object_mut() {
        database.insert("languageOverride".into(), "".into());
    }
    let default = inherited.and_then(|inherited| database_language(&default_database, inherited));
    let default_label = match &default {
        Ok(language) => format!("{}（默认）", language.label()),
        Err(_) => "默认语言不可用".into(),
    };
    let selected_label = if selected.is_empty() {
        default_label.clone()
    } else {
        Language::parse(&selected)
            .map_or_else(|| selected.clone(), |language| language.label().to_owned())
    };
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 28.0;
        ui.spacing_mut().button_padding = vec2(8.0, 5.0);
        ui.visuals_mut().widgets.inactive.bg_fill = BORDER;
        ui.visuals_mut().widgets.inactive.corner_radius = CornerRadius::same(4);
        ComboBox::from_id_salt("voice-language")
            .width(ui.available_width())
            .selected_text(selected_label)
            .show_ui(ui, |ui| {
                ui.spacing_mut().interact_size.y = 27.0;
                let default_supported = default
                    .as_ref()
                    .is_ok_and(|language| languages.contains(language));
                ui.add_enabled_ui(default_supported, |ui| {
                    ui.selectable_value(&mut selected, String::new(), &default_label);
                });
                for language in &languages {
                    ui.selectable_value(&mut selected, language.id().to_owned(), language.label())
                        .on_hover_text(input_hint(*language));
                }
            })
            .response
            .on_hover_text(if app.editor.active_ref().is_some() {
                "应用于当前音符组；默认项使用其继承的歌声语言"
            } else {
                "应用于当前音轨，新建音符组继承此设置"
            });
    });
    if selected != original {
        app.commit_lyric();
        if let Err(error) = app.editor.set_voice_language(Language::parse(&selected)) {
            app.error = Some(error);
        }
    }
}

fn input_hint(language: Language) -> &'static str {
    match language {
        Language::Mandarin => "可输入汉字、拼音或显式音素；多音字可用拼音指定读音",
        Language::English => "支持发音词典覆盖的单词；生词或多读音单词需填写显式音素",
        Language::Japanese => "可输入假名、罗马音或词典覆盖的汉字；多读音词可用假名指定",
        Language::Spanish => "支持词典覆盖的音节与字母组合；未覆盖的读音需填写显式音素",
        Language::Cantonese => "可输入粤拼或词典覆盖的汉字；多读音字可用粤拼指定",
        Language::Korean => "当前支持按音节分隔的罗马音或显式音素；韩文整句自动注音尚未接入",
    }
}
