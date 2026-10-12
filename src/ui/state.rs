use crate::model::{Parameter, new_id};
use serde::{Deserialize, Serialize};

use super::scale::ScaleDisplay;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Column {
    Editor,
    Voice,
    Note,
    Dictionary,
    Render,
    Script,
}
impl Column {
    pub const PANELS: [Self; 5] = [
        Self::Voice,
        Self::Note,
        Self::Dictionary,
        Self::Render,
        Self::Script,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Editor => "编辑器",
            Self::Voice => "歌声",
            Self::Note => "音符",
            Self::Dictionary => "词典",
            Self::Render => "渲染",
            Self::Script => "脚本",
        }
    }
    pub fn icon(self) -> super::icons::Icon {
        use super::icons::Icon;
        match self {
            Self::Editor => Icon::NoteSelect,
            Self::Voice => Icon::Microphone,
            Self::Note => Icon::Notes,
            Self::Dictionary => Icon::Dictionary,
            Self::Render => Icon::Render,
            Self::Script => Icon::Script,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Lane {
    pub id: u64,
    pub foreground: Parameter,
    pub background: Parameter,
    pub visible: bool,
    pub height: f32,
}
impl Lane {
    pub fn new() -> Self {
        Self {
            id: new_id(),
            foreground: Parameter::Pitch,
            background: Parameter::Loudness,
            visible: true,
            height: 148.0,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Viewport {
    pub start: f64,
    pub beat_width: f32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ViewState {
    pub columns: Vec<Column>,
    pub panel_width: f32,
    pub arrangement_first: bool,
    pub arrangement_height: f32,
    pub arrangement_collapsed: bool,
    pub piano: Viewport,
    pub arrangement: Viewport,
    pub top_pitch: f32,
    pub row_height: f32,
    pub grid: i32,
    pub triplet: bool,
    pub snap: bool,
    pub scale: ScaleDisplay,
    pub lanes: Vec<Lane>,
    pub phonemes: bool,
}
impl Default for ViewState {
    fn default() -> Self {
        Self {
            columns: vec![Column::Editor],
            panel_width: 234.0,
            arrangement_first: true,
            arrangement_height: 264.0,
            arrangement_collapsed: false,
            piano: Viewport {
                start: 0.0,
                beat_width: 160.0,
            },
            arrangement: Viewport {
                start: 0.0,
                beat_width: 20.0,
            },
            top_pitch: 74.0,
            row_height: 24.0,
            grid: 16,
            triplet: false,
            snap: true,
            scale: ScaleDisplay::default(),
            lanes: vec![Lane {
                visible: false,
                ..Lane::new()
            }],
            phonemes: false,
        }
    }
}
impl ViewState {
    pub fn toggle(&mut self, column: Column) {
        if self.columns.contains(&column) {
            self.columns.retain(|c| *c != column);
        } else {
            let editor = self
                .columns
                .iter()
                .position(|c| *c == Column::Editor)
                .unwrap();
            self.columns.insert(editor + 1, column);
        }
    }
    pub fn step(&self) -> f64 {
        4.0 / self.grid as f64 * if self.triplet { 2.0 / 3.0 } else { 1.0 }
    }
    pub fn snap_beat(&self, beat: f64) -> f64 {
        if self.snap {
            (beat / self.step()).round() * self.step()
        } else {
            beat
        }
    }
    pub fn valid(&self) -> bool {
        self.columns
            .iter()
            .filter(|c| **c == Column::Editor)
            .count()
            == 1
            && self
                .columns
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                == self.columns.len()
            && self.panel_width.is_finite()
            && (120.0..=600.0).contains(&self.panel_width)
            && self.arrangement_height.is_finite()
            && (28.0..=1000.0).contains(&self.arrangement_height)
            && [&self.piano, &self.arrangement].iter().all(|v| {
                v.start.is_finite()
                    && v.start >= 0.0
                    && v.beat_width.is_finite()
                    && (4.0..=640.0).contains(&v.beat_width)
            })
            && self.top_pitch.is_finite()
            && (12.0..=127.0).contains(&self.top_pitch)
            && self.row_height.is_finite()
            && (8.0..=48.0).contains(&self.row_height)
            && [4, 8, 16, 32, 64, 128].contains(&self.grid)
            && self.lanes.len() <= 16
            && self
                .lanes
                .iter()
                .all(|l| l.height.is_finite() && (64.0..=600.0).contains(&l.height))
    }
}
#[derive(Clone, Copy)]
pub enum DragPayload {
    Column(Column),
    Arrangement,
    Piano,
    Lane(u64),
}
