use super::{error_label, weak, mode_controls};
use crate::{app::App, editor::VocalModeEditCapability, model::PublicModeControl};
use eframe::egui::Ui;

fn voice_key(app: &App) -> Option<String> {
    let selected = app.voice_selection()?;
    let voice = app.voice_inventory()?;

    Some(format!("{}:{}:{}",selected.database_id,selected.revision,voice.identity()))
}

pub(super) fn show(app: &mut App, ui: &mut Ui) {
    let Some(inventory) = app.voice_inventory() else {
        weak(ui,crate::i18n::t("加载歌声后显示支持的唱法")); return;
    };
    let Some(configuration) = inventory.configuration.as_ref() else {
        weak(ui,crate::i18n::t("加载歌声后显示支持的唱法")); return;
    };
    let capability = VocalModeEditCapability::from_inventory(inventory);
    let independent = capability.as_ref().map_or(
        configuration.family == crate::synthesis::voice::VoiceFamily::Sv2,
        VocalModeEditCapability::independent_lanes,
    );
    let mut names = configuration.vocal_mode_names().to_vec();
    let hidden = configuration.hidden_styles.clone();
    let selected_voice = voice_key(app);
    let selection = match app.editor.vocal_mode_selection() {
        Ok(selection) => selection,
        Err(error) => { error_label(ui,error); return; }
    };
    let mut reason = capability.as_ref().err().cloned();
    if selected_voice.is_none() { reason = Some(crate::i18n::t("等待声库与当前选择匹配。").into()); }
    if app.editor.is_editing() { reason = Some(crate::i18n::t("请先完成当前编辑，再调整唱法。").into()); }

    if names.is_empty() {
        names.extend(selection.controls.keys().cloned());
        if names.is_empty() { weak(ui,crate::i18n::t("此歌声没有可用的唱法")); }
    }
    let mut actions = Vec::new();
    ui.spacing_mut().item_spacing.y = 2.0;
    for (slot,name) in names.into_iter().enumerate() {
        let control = selection.controls.get(&name).cloned().unwrap_or_else(PublicModeControl::default);
        let changes = mode_controls::show(ui,
            selected_voice.as_deref().unwrap_or("unavailable"),selection.reference_id,slot,
            &name,&control,reason.as_deref(),independent,hidden.contains(&name));
        if !changes.is_empty() {actions.push((name,changes));}
    }
    for (name,changes) in actions {
        app.commit_lyric();

        let current = app.voice_inventory().map(VocalModeEditCapability::from_inventory);
        if voice_key(app) != selected_voice || current.as_ref() != Some(&capability) {
            app.error = Some(crate::i18n::t("歌声或唱法目录已变化；已取消旧的拖动。").into());
            continue;
        }
        if let Ok(capability) = &capability {
            let result = if let [(component,value)] = changes.as_slice() {
                app.editor.set_vocal_mode_component(selection.reference_id,capability,&name,*component,*value)
            } else {
                app.editor.set_vocal_mode_components(selection.reference_id,capability,&name,&changes)
            };
            if let Err(error) = result
            { app.error = Some(error); }
        }
    }
    let unsupported=eframe::egui::Id::new("unsupported-style-lanes");
    if ui.ctx().data(|data|data.get_temp::<bool>(unsupported)).unwrap_or(false)
        && crate::ui::prompt::notice(ui.ctx(),crate::i18n::t("功能不支持"),
            "请将旧版兼容歌声升级版本 2 以单独控制音高、音色和发音。")
    {ui.ctx().data_mut(|data|data.remove::<bool>(unsupported));}
}
