use super::{language, parameters, selected_entry, selector, weak, widgets};
use crate::{
    app::App,
    synthesis::{runtime::VoiceState, settings::VoiceSelection},
    ui::theme::*,
};
use eframe::egui::*;

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.spacing_mut().item_spacing.y = 0.0;
    Frame::new().inner_margin(16).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 8.0;
        selector::selected_card(app, ui);
        weak(ui, "版本");
        version(app, ui);
        weak(ui, "预设");
        let preset = app
            .editor
            .active_ref()
            .and_then(|reference| reference.extra.get("voicePresetName"))
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.is_empty())
            .unwrap_or("默认");
        widgets::field(ui, preset, true, "歌声预设编辑尚未接入合成");
        status(app, ui);
    });
    section(ui, "语言", |ui| language::show(app, ui));
    section(ui, "唱法", |ui| parameters::styles(app, ui));
    section(ui, "参数", |ui| parameters::scalars(app, ui));
    section(ui, "调音", |ui| parameters::tuning(app, ui));
    section(ui, "调式音阶", |ui| {
        crate::ui::scale::controls::panel(app, ui);
    });
}

fn version(app: &mut App, ui: &mut Ui) {
    let Some(selected) = app.voice_selection() else {
        widgets::field(ui, "未选择歌声", true, "先选择歌声");
        return;
    };
    let mut revision = selected.revision;
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 28.0;
        ui.visuals_mut().widgets.inactive.bg_fill = BORDER;
        ui.visuals_mut().widgets.inactive.corner_radius = CornerRadius::same(4);
        ComboBox::from_id_salt("voice-revision")
            .width(ui.available_width())
            .selected_text(revision.to_string())
            .show_ui(ui, |ui| {
                if let Some(catalog) = app.synthesis.library.catalog() {
                    for entry in catalog
                        .entries
                        .iter()
                        .filter(|entry| entry.database_id == selected.database_id)
                    {
                        ui.add_enabled_ui(entry.availability.is_loadable(), |ui| {
                            ui.selectable_value(
                                &mut revision,
                                entry.revision,
                                entry.revision.to_string(),
                            );
                        })
                        .response
                        .on_disabled_hover_text("此版本尚未支持合成");
                    }
                }
            });
    });
    if revision != selected.revision {
        app.select_voice(VoiceSelection {
            database_id: selected.database_id,
            revision,
        });
    }
}

fn status(app: &mut App, ui: &mut Ui) {
    let failed = matches!(app.voice_state(), VoiceState::Failed(_))
        || app.synthesis.library.settings_error().is_some()
        || app.voice_error().is_some();
    if matches!(app.voice_state(), VoiceState::Loading) || app.synthesis.library.is_scanning() {
        ui.horizontal(|ui| {
            ui.spinner();
            weak(ui, "正在加载歌声…");
        });
    } else if failed {
        if ui
            .button(RichText::new("加载异常 · 管理歌声…").color(Color32::from_rgb(218, 142, 126)))
            .clicked()
        {
            app.voice_manager = true;
        }
    } else if app.synthesis.library.catalog_error().is_some() {
        if ui.button("目录读取异常 · 管理歌声…").clicked() {
            app.voice_manager = true;
        }
    } else if selected_entry(app).is_none() && ui.button("管理歌声…").clicked() {
        app.voice_manager = true;
    }
}

fn section(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui)) {
    let y = ui.next_widget_position().y;
    ui.painter().hline(
        ui.available_rect_before_wrap().x_range(),
        y,
        Stroke::new(1.0, EDGE),
    );
    Frame::new()
        .inner_margin(Margin {
            left: 16,
            right: 16,
            top: 12,
            bottom: 16,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 12.0;
            CollapsingHeader::new(RichText::new(title).font(section_title_font()))
                .id_salt(("voice-section", title))
                .default_open(true)
                .show_unindented(ui, |ui| {
                    ui.set_width(ui.available_width());
                    content(ui);
                });
        });
}
