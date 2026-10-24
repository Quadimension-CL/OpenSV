use super::*;
pub(crate) fn show(app:&mut App,ctx:&Context) {
    let mut state=std::mem::take(&mut app.preset_library);
    let Some(mut naming)=state.naming.take() else {app.preset_library=state;return;};
    let valid=app.voice_selection().is_some_and(|s|s.database_id.to_string()==naming.bank&&s.revision.to_string()==naming.revision)
        && app.editor.voice_preset(&naming.revision).is_ok_and(|s|s.0==naming.reference);
    if !valid {app.preset_library=state;return;}
    let mut close=false;let mut submit=false;
    let response=Modal::new(Id::new("save-voice-preset"))
        .frame(Frame::new().fill(Color32::from_rgb(61,59,64)).corner_radius(4).inner_margin(24))
        .show(ctx,|ui| {
            ui.set_width(340.0);ui.spacing_mut().item_spacing.y=12.0;
            ui.label(RichText::new(crate::i18n::t("保存为新的预设")).font(section_title_font()));
            ui.label(crate::i18n::t("预设名称"));
            let input=ui.add_sized(vec2(ui.available_width(),28.0),TextEdit::singleline(&mut naming.name));
            if naming.focus {
                input.request_focus();
                if let Some(mut state)=TextEdit::load_state(ctx,input.id) {
                    state.cursor.set_char_range(Some(text::CCursorRange::two(text::CCursor::new(0),text::CCursor::new(naming.name.chars().count()))));
                    state.store(ctx,input.id);
                }
                naming.focus=false;
            }
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center),|ui| {
                submit=ui.add_enabled(!naming.name.is_empty(),Button::new(crate::i18n::t("确定")).fill(Color32::from_rgb(116,204,20)).min_size(vec2(80.0,28.0))).clicked();
                close=ui.add_sized(vec2(80.0,28.0),Button::new(crate::i18n::t("取消"))).clicked();
            });
            submit|=!naming.name.is_empty()&&ui.input(|i|i.key_pressed(Key::Enter));
        });
    close|=response.should_close();
    if submit {
        let catalogue=app.voice_inventory().and_then(|v|v.configuration.as_ref()).map(|c|c.vocal_mode_names().to_vec()).unwrap_or_default();
        match save(app,&mut state,&naming.bank,&naming.revision,naming.reference,&naming.name,&catalogue) {
            Ok(())=>close=true,Err(error)=>{app.error=Some(error);close=true;},
        }
    }
    if !close {state.naming=Some(naming);}
    app.preset_library=state;
}
