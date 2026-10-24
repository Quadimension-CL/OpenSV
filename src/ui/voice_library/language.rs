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
        widgets::field(ui, crate::i18n::t("未加载歌声"), true, crate::i18n::t("加载声库后可选择其支持的语言"));
        return;
    };
    let Some(native) = Language::parse(&config.default_language) else {
        error_label(
            ui,
            crate::lformat!("无法识别声库默认语言：{}", config.default_language),
        );
        return;
    };
    let languages: Vec<_> = config
        .supported_languages
        .iter()
        .filter_map(|id| Language::parse(id))
        .collect();
    let pronunciation_supported = config.family == crate::synthesis::voice::VoiceFamily::Sv2;
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
        Ok(language) => crate::lformat!("{} (默认)", crate::i18n::t(language.label())),
        Err(_) => crate::i18n::t("默认语言不可用").into(),
    };
    let effective = Language::parse(&selected).or_else(|| default.as_ref().ok().copied());
    let selected_label = if selected.is_empty() {
        default_label.clone()
    } else {
        Language::parse(&selected)
            .map_or_else(|| selected.clone(), |language| crate::i18n::t(language.label()).to_owned())
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
                    ui.selectable_value(&mut selected, language.id().to_owned(), crate::i18n::t(language.label()))
                        .on_hover_text(input_hint(*language));
                }
            })
            .response
            .on_hover_text(if app.editor.active_ref().is_some() {
                crate::i18n::t("应用于当前音符组；默认项使用其继承的歌声语言")
            } else {
                crate::i18n::t("应用于当前音轨，新建音符组继承此设置")
            });
    });
    if selected != original {
        app.commit_lyric();
        if let Err(error) = app.editor.set_voice_language(Language::parse(&selected)) {
            app.error = Some(error);
        }
    }

    let pronunciation = match effective {
        Some(Language::English) => Some(("relaxedPronunciation", crate::i18n::t("使用松弛辅音"), true)),
        Some(Language::Spanish) => Some(("europeanPronunciation", crate::i18n::t("使用欧式发音"), false)),
        _ => None,
    };
    if pronunciation_supported && let Some((key,label,default)) = pronunciation {
        ui.add_space(6.0);
        match app.editor.pronunciation_option(key,default) {
            Ok((reference, mut enabled)) => {
                if ui.checkbox(&mut enabled, label).changed() {
                    app.commit_lyric();
                    if let Err(error) = app.editor.set_pronunciation_option(reference,key,enabled) {
                        app.error = Some(error);
                    }
                }
            }
            Err(error) => error_label(ui, error),
        }
    }
    ui.add_space(12.0);
    ui.label(RichText::new(crate::i18n::t("辅音")).color(TEXT));
    ui.spacing_mut().item_spacing.y=0.0;
    for spec in crate::synthesis::parameters::CONSONANT_CONTROLS {
        super::parameters::control(app,ui,spec);
    }
}

fn input_hint(language: Language) -> &'static str {
    match language {
        Language::Mandarin => crate::i18n::t("可输入汉字、拼音或显式音素；多音字可用拼音指定读音"),
        Language::English => crate::i18n::t("支持发音词典覆盖的单词；生词可填写显式音素指定读音"),
        Language::Japanese => crate::i18n::t("可输入假名、罗马音或词典覆盖的汉字；多读音词可用假名指定"),
        Language::Spanish => crate::i18n::t("支持西班牙语自动发音和欧式发音；可填写显式音素指定读音"),
        Language::Cantonese => crate::i18n::t("可输入粤拼或词典覆盖的汉字；多读音字可用粤拼指定"),
        Language::Korean => crate::i18n::t("当前支持按音节分隔的罗马音或显式音素；韩文整句自动注音尚未接入"),
        Language::French | Language::German | Language::Portuguese => crate::i18n::t("支持发音词典覆盖的单词；可填写显式音素指定读音"),
    }
}
