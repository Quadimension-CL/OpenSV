use super::super::{canvas::note_rect, scene::SceneGroup};
use crate::{
    app::App,
    model::EntityId,
    synthesis::{plan::OwnerId, rendered::RenderedPhoneme},
    ui::{
        group_geometry::{clip_rect, visible_note_rect},
        theme::{MUTED, TEXT},
    },
};
use eframe::egui::*;
use std::collections::BTreeMap;

pub fn draw(app: &App, ui: &Ui, canvas: Rect, groups: &[SceneGroup]) {
    for scene in groups {
        let reference = &scene.reference;
        let Some(snapshot) = app.synthesis.phonemes(OwnerId(reference.id)) else {
            continue;
        };
        let Some(group) = app.editor.project.group(&reference.group_id) else {
            continue;
        };
        if !snapshot.input.matches_project(
            &app.editor.project,
            app.editor.current_track(),
            reference,
            group,
        ) {
            continue;
        }
        let mut phones: BTreeMap<EntityId, Vec<&RenderedPhoneme>> = BTreeMap::new();
        for phone in snapshot.data.phones.iter() {
            phones.entry(phone.source.note_id).or_default().push(phone);
        }
        let painter = ui.painter().with_clip_rect(
            clip_rect(reference, canvas, &app.view.piano).intersect(ui.clip_rect()),
        );
        let color = if app.editor.reference == Some(reference.id) {
            TEXT
        } else {
            MUTED
        };
        for note in &scene.notes {
            if visible_note_rect(
                note,
                reference,
                canvas,
                &app.view.piano,
                app.view.top_pitch,
                app.view.row_height,
            )
            .is_none()
            {
                continue;
            }
            let Some(phones) = phones.get_mut(&note.id) else {
                continue;
            };
            phones.sort_by_key(|phone| phone.source.phone_index);
            let label = phones
                .iter()
                .map(|phone| phone.token.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let rect = note_rect(
                note,
                reference.blick_offset,
                reference.pitch_offset,
                canvas,
                app,
            );
            let bounds = Rect::from_min_max(
                pos2(rect.left(), canvas.top()),
                pos2(rect.right(), rect.top()),
            );
            painter
                .with_clip_rect(bounds.intersect(painter.clip_rect()))
                .text(
                    pos2(rect.left() + 6.0, rect.top() - 3.0),
                    Align2::LEFT_BOTTOM,
                    label,
                    FontId::proportional(12.0),
                    color,
                );
        }
    }
}
