mod background;
mod background_notes;
mod canvas;
mod gestures;
mod keyboard;
mod lyrics;
mod pitch_anchor;
mod pitch_edit;
mod rendered;
mod retakes;
mod scale_guide;
mod scene;
mod toolbar;
use crate::model::*;
pub use canvas::show;
use eframe::egui::Pos2;
pub use keyboard::{KeyboardAudition, begin_keyboard_frame, cancel_audition, end_keyboard_frame};
pub use pitch_anchor::{
    AnchorSelection, anchor_delete, anchor_keyboard, anchor_select_all, end_anchor_frame,
};
pub use pitch_anchor::{
    anchor_clear, anchor_copy, anchor_cut, anchor_has_clipboard, anchor_has_selection,
    anchor_paste, anchor_shift_octave,
};
pub use pitch_edit::end_pitch_frame;
use std::collections::BTreeSet;
pub use toolbar::toolbar;
pub const TOOLBAR_HEIGHT: f32 = 32.0;
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    NoteSelect,
    NoteDraw,
    PitchAnchor,
    PitchDraw,
}
impl Tool {
    pub fn is_pitch(self) -> bool {
        matches!(self, Self::PitchAnchor | Self::PitchDraw)
    }
}
pub struct LyricEdit {
    pub id: EntityId,
    pub text: String,
    pub focus: bool,
}
#[derive(Clone, Copy)]
pub enum NoteDrag {
    Move,
    Left,
    Right,
}
pub enum Gesture {
    Anchor(Box<pitch_anchor::AnchorGesture>),
    Pitch(Box<pitch_edit::PitchGesture>),
    Notes {
        origin: Pos2,
        notes: Vec<Note>,
        kind: NoteDrag,
        anchor: i64,
    },
    Create {
        origin: Pos2,
        id: EntityId,
        onset: i64,
    },
    Select {
        origin: Pos2,
        initial: BTreeSet<EntityId>,
    },
    Parameter {
        lane: u64,
        time: i64,
    },
}
