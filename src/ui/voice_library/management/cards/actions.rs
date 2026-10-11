use super::super::{super::paths, ACCENT, CARD, SURFACE, State, catalog};
use crate::{
    app::App,
    synthesis::{
        runtime::VoiceState,
        voice::{VoiceCatalogEntry, VoiceFamily},
    },
    ui::theme::*,
};
use eframe::egui::*;

pub(super) fn primary(
    app: &mut App,
    ui: &mut Ui,
    entry: &VoiceCatalogEntry,
    state: &mut State,
    current: bool,
    stable: bool,
) {
    let key = entry.selection();
    let voice = app.synthesis.voice_state(&key);
    let loading = matches!(voice, VoiceState::Loading);
    let needs_context =
        entry.family != Some(VoiceFamily::Sv1) && app.synthesis.needs_context_import(&key);
    let problem =
        matches!(voice, VoiceState::Loaded(_)) && app.synthesis.voice_error(&key).is_some();
    let loaded = matches!(voice, VoiceState::Loaded(_)) && !problem;
    let missing_dictionary = app.synthesis.dictionaries().is_none();
    let label = if !entry.availability.is_loadable() {
        "不可用"
    } else if current && loading {
        "正在加载…"
    } else if needs_context {
        "导入模型上下文…"
    } else if problem {
        "处理问题"
    } else if current && loaded {
        "取消当前音轨歌声"
    } else if loaded {
        "用于当前音轨"
    } else if missing_dictionary {
        "设置词典目录"
    } else if current && matches!(voice, VoiceState::Failed(_)) {
        "重试加载"
    } else {
        "加载歌声"
    };
    let enabled = stable
        && entry.availability.is_loadable()
        && !(current && loading)
        && !app.editor.current_track().is_instrumental()
        && !app.synthesis.library.is_scanning()
        && app.synthesis.library.settings_error().is_none();
    let outline = loaded || problem || missing_dictionary;
    let button = Button::new(RichText::new(label).size(13.0).color(if !enabled {
        MUTED
    } else if outline {
        ACCENT
    } else {
        SURFACE
    }))
    .corner_radius(13)
    .fill(if !enabled || outline { CARD } else { ACCENT })
    .stroke(Stroke::new(0.8, if enabled { ACCENT } else { BORDER }));
    if ui
        .add_enabled(enabled, button.min_size(vec2(ui.available_width(), 36.0)))
        .clicked()
    {
        if needs_context {
            app.synthesis.open_context_import(key);
        } else if problem {
            state.expanded = Some(entry.database_id.clone());
        } else if current && loaded {
            app.unload_voice();
        } else if missing_dictionary {
            paths::choose_dictionaries(app);
        } else if current {
            app.reload_voice();
        } else {
            app.select_voice(catalog::selection(entry));
        }
    }
}

pub(super) fn menu(
    app: &mut App,
    ui: &mut Ui,
    entry: &VoiceCatalogEntry,
    state: &mut State,
    current: bool,
) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 13, BORDER);
    }
    for offset in [-5.0, 0.0, 5.0] {
        ui.painter()
            .circle_filled(rect.center() + vec2(0.0, offset), 1.5, MUTED);
    }
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "更多声库操作"));
    Popup::menu(&response).show(|ui| {
        if ui
            .button(if state.expanded.as_ref() == Some(&entry.database_id) {
                "收起详情"
            } else {
                "详情与诊断"
            })
            .clicked()
        {
            if state.expanded.as_ref() == Some(&entry.database_id) {
                state.expanded = None;
            } else {
                state.expanded = Some(entry.database_id.clone());
            }
            ui.close();
        }
        if !matches!(
            app.synthesis.voice_state(&entry.selection()),
            VoiceState::Unloaded
        ) {
            if ui
                .add_enabled(
                    !matches!(
                        app.synthesis.voice_state(&entry.selection()),
                        VoiceState::Loading
                    ),
                    Button::new("重新加载"),
                )
                .clicked()
            {
                app.synthesis.reload_voice(&entry.selection());
                ui.close();
            }
            if current && ui.button("取消当前音轨歌声").clicked() {
                app.unload_voice();
                ui.close();
            }
        }
    });
}
