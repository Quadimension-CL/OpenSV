use crate::app::App;
use eframe::egui::*;

pub(super) fn paint(app:&App,ui:&mut Ui,canvas:Rect) {
    if !app.preferences.display_background_image || app.preferences.background_transparency>=1.0 {
        return;
    }
    let Some(selection)=app.voice_selection() else {return};
    let Some(artwork)=app.synthesis.library.entry(&selection).and_then(|entry|entry.portrait.as_ref()) else {return};
    let image=crate::ui::voice_library::artwork::image(artwork);
    let Ok(texture)=image.load_for_size(ui.ctx(),canvas.size()) else {return};
    let Some(size)=texture.size() else {return};
    let height=canvas.height().min(size.y);
    let scale=(height/size.y).min(canvas.width()/size.x);
    let size=size*scale;
    let rect=Rect::from_min_size(canvas.right_bottom()-size,size);
    let mut clipped=ui.new_child(UiBuilder::new().max_rect(canvas));
    clipped.set_clip_rect(canvas.intersect(ui.clip_rect()));
    image.tint(Color32::from_white_alpha(((1.0-app.preferences.background_transparency)*255.0).round() as u8))
        .paint_at(&clipped,rect);
}
