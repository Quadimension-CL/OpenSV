use super::{editable, interaction, widgets};
use crate::{app::App, editor::PerformanceParameter};
use eframe::egui::*;

pub(crate) fn available(app: &App) -> bool {
    app.voice_inventory().and_then(|v|v.configuration.as_ref()).is_some_and(|c|
        c.family==crate::synthesis::voice::VoiceFamily::Sv2 && !c.speakers.is_empty())
}
pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    let selection=app.editor.performance_selection();
    let selected=selection.as_ref().ok().and_then(Option::as_ref);
    let enabled=editable(app,ui)&&selected.is_some();
    for (index,label,parameter) in [
        (0,crate::i18n::t("音符开头"),PerformanceParameter::ChoirOnset),
        (1,crate::i18n::t("乐句结尾"),PerformanceParameter::ChoirTail),
        (2,crate::i18n::t("音高"),PerformanceParameter::ChoirPitch),
    ] {
        let current=selected.and_then(|s|s.choir_spread[index]);
        let mixed=selected.is_some()&&current.is_none();
        let mut value=current.unwrap_or(1.)/2.;
        let response=ui.add_enabled_ui(enabled,|ui|widgets::labeled_slider(ui,label,
            if mixed {crate::i18n::t("多个值").into()}else{format!("{:.0}%",current.unwrap_or(1.)*100.)},
            &mut value,mixed,0.5)).inner;
        interaction::apply(app,ui.ctx(),&response,parameter,|editor| {
            editor.update_choir_spread(parameter,(value*40.).round()/20.);
        });
        ui.add_space(8.);
    }
    if let Err(error)=selection {ui.weak(crate::i18n::message(&error));}
}
