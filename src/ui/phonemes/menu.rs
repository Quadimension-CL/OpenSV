use super::{Handle, geometry::Mapping, interaction};
use crate::{
    app::App,
    model::{EntityId, PhonemeAttributes, PhonemeParameter, SourcePhone},
    synthesis::rendered::RenderedPhoneme,
};
use eframe::egui::*;

#[derive(Clone, Copy)]
struct Target {
    track: EntityId,
    reference: Option<EntityId>,
    source: SourcePhone,
    timing_source: Option<SourcePhone>,
    supports_timing: bool,
    supports_strength: bool,
    supports_impulse: bool,
}

pub(super) fn show(
    app: &mut App,
    ui: &Ui,
    response: &Response,
    mapping: &Mapping,
    phones: &[RenderedPhoneme],
) {
    let id = response.id.with("phoneme-reset-target");
    if response.secondary_clicked() {
        let target = response
            .interact_pointer_pos()
            .and_then(|position| {
                interaction::hit(mapping, phones, position).or_else(|| {
                    phones.iter().enumerate().find_map(|(index, phone)| {
                        (mapping.clip().contains(position)
                            && mapping.phone_rect(phone).x_range().contains(position.x))
                        .then_some((index, Handle::Left))
                    })
                })
            })
            .map(|(index, handle)| {
                let phone = &phones[index];
                let timing_index = if handle == Handle::Right {
                    interaction::right_source(phones, index)
                } else {
                    Some(index)
                };
                Target {
                    track: app.editor.track,
                    reference: app.editor.reference,
                    source: phone.source,
                    timing_source: timing_index.map(|index| phones[index].source),
                    supports_timing: timing_index
                        .is_some_and(|index| phones[index].supports_timing),
                    supports_strength: phone.strength.is_some(),
                    supports_impulse: phone.supports_impulse,
                }
            });
        ui.ctx().data_mut(|data| data.insert_temp(id, target));
        if target.is_some() && editable(app, ui) {
            super::focus(app, ui.ctx());
        }
    }
    let target = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Option<Target>>(id))
        .flatten();
    response.context_menu(|ui| {
        let Some(target) = target.filter(|target| {
            target.track == app.editor.track && target.reference == app.editor.reference
        }) else {
            ui.close();
            return;
        };
        let attributes = app.editor.phoneme_attributes(target.source);
        let timing_attributes = target
            .timing_source
            .and_then(|source| app.editor.phoneme_attributes(source).ok());
        let enabled = editable(app, ui) && attributes.is_ok();
        let attributes = attributes.unwrap_or_default();
        let timing_enabled = enabled
            && timing_attributes.is_some_and(|attributes| {
                target.supports_timing || attributes.left_offset.is_some()
            });
        if ui
            .add_enabled(timing_enabled, Button::new("重置音素时值"))
            .clicked()
            && let Some(source) = target.timing_source
        {
            reset(app, source, Some(PhonemeParameter::LeftOffset));
            ui.close();
        }
        if ui
            .add_enabled(
                enabled && (target.supports_strength || attributes.strength.is_some()),
                Button::new("重置音素强度"),
            )
            .clicked()
        {
            reset(app, target.source, Some(PhonemeParameter::Strength));
            ui.close();
        }
        if ui
            .add_enabled(
                enabled
                    && (target.supports_impulse
                        || attributes.position.is_some()
                        || attributes.activity.is_some()),
                Button::new("重置辅音爆破"),
            )
            .clicked()
        {
            reset_impulse(app, target.source, attributes);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(
                enabled && attributes != PhonemeAttributes::default(),
                Button::new("重置此音素全部属性"),
            )
            .clicked()
        {
            reset(app, target.source, None);
            ui.close();
        }
    });
}

fn editable(app: &App, ui: &Ui) -> bool {
    !app.editor.is_editing()
        && app.gesture.is_none()
        && app.group_gesture.is_none()
        && !super::phoneme_dragging(app)
        && !app.view_pan.is_active()
        && app.audio_drag.is_none()
        && ui.is_enabled()
        && ui.input(|input| input.focused)
}

fn reset(app: &mut App, source: SourcePhone, parameter: Option<PhonemeParameter>) {
    if let Err(error) = app.editor.reset_phoneme_attributes(source, parameter) {
        app.error = Some(error);
    }
}

fn reset_impulse(app: &mut App, source: SourcePhone, mut attributes: PhonemeAttributes) {
    attributes.position = None;
    attributes.activity = None;
    if !app.editor.begin_phoneme_edit(source) {
        app.error = Some("无法编辑当前音素".into());
    } else if app.editor.update_phoneme_attributes(source, attributes) {
        app.editor.finish_phoneme_edit();
    } else {
        app.editor.cancel_phoneme_edit();
        app.error = Some("无法重置辅音爆破".into());
    }
}
