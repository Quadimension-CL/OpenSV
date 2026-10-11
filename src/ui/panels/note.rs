mod performance;
mod retake_buttons;
mod retakes;
use super::controls::*;
use crate::{
    app::App,
    model::{Language, Note, database_language},
};
use eframe::egui::*;
pub(crate) use performance::end_frame as end_performance_frame;

#[derive(Clone, Default)]
struct PhonemeDraft {
    group: String,
    selection: Vec<u64>,
    reference: Option<u64>,
    source: Vec<String>,
    text: String,
    dirty: bool,
}

pub(super) fn show(app: &mut App, ui: &mut Ui) {
    section(ui, "模式", |ui| performance::mode(app, ui));
    section(ui, "演绎", |ui| performance::expression(app, ui));
    section(ui, "AI 重录", |ui| retakes::show(app, ui));
    let notes: Vec<_> = app
        .editor
        .notes()
        .iter()
        .filter(|n| app.editor.selected.contains(&n.id))
        .cloned()
        .collect();
    section(ui, "语言", |ui| {
        ui.add_enabled_ui(!notes.is_empty(), |ui| language(app, ui, &notes));
    });
    section(ui, "音素", |ui| {
        ui.add_enabled_ui(!notes.is_empty(), |ui| phonemes(app, ui));
        if notes.is_empty() {
            ui.weak("在钢琴卷帘中选择音符");
        }
    });
}

fn language(app: &mut App, ui: &mut Ui, notes: &[Note]) {
    let config = app
        .voice_inventory()
        .and_then(|voice| voice.configuration.as_ref());
    let languages: Vec<_> = config
        .map(|config| {
            config
                .supported_languages
                .iter()
                .filter_map(|s| Language::parse(s))
                .collect()
        })
        .unwrap_or_default();
    let native = config.and_then(|c| Language::parse(&c.default_language));
    let inherited = native.and_then(|native| {
        let track = &app.editor.current_track().main_ref;
        let language = database_language(&track.database, native).ok()?;
        database_language(&app.editor.active_ref().unwrap_or(track).database, language).ok()
    });
    let override_value = |note: &Note| {
        note.attributes
            .get("languageOverride")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let original = notes.first().map(override_value).unwrap_or_default();
    let mixed = notes.iter().any(|n| override_value(n) != original);
    let mut selected = original.clone();
    let default_label = inherited.map_or_else(
        || "继承歌声语言".into(),
        |lang| format!("{}（继承歌声）", lang.label()),
    );
    let label = if mixed {
        "多个值".into()
    } else if selected.is_empty() {
        default_label.clone()
    } else {
        Language::parse(&selected).map_or_else(|| selected.clone(), |lang| lang.label().into())
    };
    let mut clicked = false;
    ComboBox::from_id_salt("note-language")
        .width(ui.available_width())
        .selected_text(label)
        .show_ui(ui, |ui| {
            clicked |= ui
                .selectable_value(&mut selected, String::new(), default_label)
                .clicked();
            for language in languages {
                clicked |= ui
                    .selectable_value(&mut selected, language.id().to_owned(), language.label())
                    .clicked();
            }
        });
    if clicked && (mixed || selected != original) {
        app.commit_lyric();
        app.editor.set_note_language(Language::parse(&selected));
    }
}

fn phonemes(app: &mut App, ui: &mut Ui) {
    let id = ui.make_persistent_id("phoneme-draft");
    let mut draft = ui
        .ctx()
        .data_mut(|d| d.get_temp::<PhonemeDraft>(id).unwrap_or_default());
    let selection_changed = draft.reference != app.editor.reference
        || draft
            .selection
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            != app.editor.selected;
    if selection_changed && draft.dirty {
        // A canvas click can change selection before this panel is drawn. The
        // draft belongs to the original notes, never the newly selected ones.
        let still_current = app.editor.project.group(&draft.group).is_some_and(|group| {
            draft
                .selection
                .iter()
                .zip(&draft.source)
                .all(|(id, source)| {
                    group
                        .notes
                        .iter()
                        .any(|n| n.id == *id && n.phonemes == *source)
                })
        });
        if still_current && !app.editor.is_editing() {
            app.commit_lyric();
            app.editor
                .set_note_phonemes(&draft.group, &draft.selection, &draft.text);
        }
    }
    let notes: Vec<_> = app
        .editor
        .notes()
        .iter()
        .filter(|n| app.editor.selected.contains(&n.id))
        .cloned()
        .collect();
    let selection: Vec<_> = notes.iter().map(|n| n.id).collect();
    let source: Vec<_> = notes.iter().map(|n| n.phonemes.clone()).collect();
    if draft.selection != selection
        || draft.reference != app.editor.reference
        || draft.source != source
    {
        draft = PhonemeDraft {
            group: app
                .editor
                .group()
                .map(|g| g.uuid.clone())
                .unwrap_or_default(),
            selection,
            reference: app.editor.reference,
            text: if source.windows(2).any(|pair| pair[0] != pair[1]) {
                String::new()
            } else {
                source.first().cloned().unwrap_or_default()
            },
            source,
            dirty: false,
        };
    }
    let mixed = draft.source.windows(2).any(|pair| pair[0] != pair[1]);
    ui.weak("音素");
    let response = ui.add_sized(
        vec2(ui.available_width(), 28.0),
        TextEdit::singleline(&mut draft.text)
            .id(id.with("input"))
            .hint_text(if mixed { "多个值" } else { "自动" })
            .vertical_align(Align::Center)
            .margin(vec2(7.0, 5.0))
            .background_color(crate::ui::theme::BORDER),
    );
    draft.dirty |= response.changed();
    if (response.has_focus() || response.lost_focus()) && ui.input(|i| i.key_pressed(Key::Escape)) {
        draft.text = if mixed {
            String::new()
        } else {
            draft.source.first().cloned().unwrap_or_default()
        };
        draft.dirty = false;
        response.surrender_focus();
    } else if draft.dirty
        && (response.lost_focus()
            || response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
    {
        app.commit_lyric();
        app.editor
            .set_note_phonemes(&draft.group, &draft.selection, &draft.text);
        draft.dirty = false;
        response.surrender_focus();
    }
    response.on_hover_text(
        "音素以空格分隔；回车或移开焦点应用到所选音符，清空恢复自动发音。Esc 取消输入。",
    );
    let even = |note: &Note| {
        note.attributes
            .get("evenSyllableDuration")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
    };
    let mut value = notes.first().is_none_or(even);
    let mixed = notes.iter().any(|note| even(note) != value);
    if ui
        .add(Checkbox::new(&mut value, "按音符时值均匀分配音节").indeterminate(mixed))
        .changed()
    {
        app.commit_lyric();
        app.editor.set_even_syllables(value);
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, draft));
}
