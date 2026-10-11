use super::{language, selected_entry, selector, weak, widgets};
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
    section(ui, "唱法", |ui| {
        let configuration = app
            .voice_inventory()
            .and_then(|voice| voice.configuration.as_ref());
        if let Some(config) = configuration {
            if config.styles.is_empty() {
                weak(ui, "此歌声没有可用的唱法");
            }
            ui.spacing_mut().item_spacing.y = 2.0;
            for style in &config.styles {
                let value = style_value(app, style);
                widgets::style(ui, style, value);
            }
        } else {
            weak(ui, "加载歌声后显示支持的唱法");
        }
    });
    section(ui, "参数", |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for label in ["响度", "张力", "气声", "性别", "声区转换"] {
            widgets::slider(ui, label);
        }
    });
    section(ui, "调音", |ui| {
        widgets::field(ui, "调性: C，A4 = 440 Hz", false, "调音设置尚未接入合成");
        ui.add_space(10.0);
        widgets::slider(ui, "音高转换");
        widgets::slider(ui, "调性转换");
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

fn style_value(app: &App, style: &str) -> f32 {
    let track = &app.editor.current_track().main_ref;
    let voice = app
        .editor
        .active_ref()
        .and_then(|r| r.extra.get("voice"))
        .filter(|voice| {
            voice
                .get("vocalModeInherited")
                .and_then(serde_json::Value::as_bool)
                == Some(false)
        })
        .or_else(|| track.extra.get("voice"));
    voice
        .and_then(|voice| voice.get("vocalModeParams"))
        .and_then(|params| params.get(style))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0) as f32
}
