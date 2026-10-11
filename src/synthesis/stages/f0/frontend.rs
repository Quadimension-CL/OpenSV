//! Shared note conditioning for the F0 and timbre models.
//!
//! This module builds the note matrix and frame expansion indexes. The matrix
//! must pass through `normalizer_node` before entering the feature network.

mod config;
mod timing;

pub use timing::frame_count;

use crate::synthesis::voice::{PreparedTree, Result, VoiceError};

use config::Configuration;
pub(crate) use timing::duration_seconds;

pub const NOTE_FRONTEND_TYPE_ID: [u8; 16] = [
    0xc8, 0x66, 0x33, 0x90, 0x32, 0x8f, 0xfd, 0xad, 0x9d, 0x6a, 0x2c, 0xd8, 0x16, 0x42, 0x63, 0x22,
];

/// Note data and partitioning used by the conditioning frontend.
#[derive(Clone, Debug)]
pub struct FrontendNote {
    /// Phone strings before language/unified-table mapping.
    pub phones: Vec<String>,
    pub pitch: f32,
    pub pitch_offset: f32,
    pub duration_seconds: f64,
    /// Frame hop in seconds; normally 240 / 48_000.
    pub frame_seconds: f64,
    /// Numeric record kind used directly by the conditioning model.
    pub kind: u32,
    /// Per-note conditioning flag.
    pub flag: bool,
    /// Optional tag, compared literally to the stored tags.
    pub tag: Option<String>,
    pub parts: u32,
    pub part_index: u32,
    /// Durations of the associated linked notes.
    pub appended_durations: Vec<f64>,
    /// Per-note performance controls retain the source note's full settings
    /// after syllable splitting and boundary redistribution.
    pub performance: crate::model::NotePerformance,
}

pub struct NoteFrontend {
    configuration: Configuration,
    pub normalizer_node: usize,
    pub pitch_normalizer_node: usize,
}

pub struct NoteFeatures {
    /// Unnormalized, note-major rows of `width` floats.
    pub values: Vec<f32>,
    pub width: usize,
    pub frame_counts: Vec<usize>,
    /// Each note contributes `0..frame_counts[note]` to this flat array.
    pub frame_positions: Vec<usize>,
}

impl NoteFrontend {
    pub fn prepare(tree: &PreparedTree, index: usize) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("note_frontend.prepare");
        let node = tree.node(index)?;
        if node.type_id != NOTE_FRONTEND_TYPE_ID || node.children.len() != 4 {
            return Err(VoiceError::new(
                "Expected the shared note frontend with four children",
            ));
        }
        let configuration = Configuration::parse(&tree.payload(index)?)?;
        let normalizer_node = node.children[2];
        Ok(Self {
            configuration,
            normalizer_node,
            pitch_normalizer_node: node.children[3],
        })
    }

    pub fn frame_seconds(&self) -> f64 {
        self.configuration.frame_seconds
    }

    pub fn encode(&self, notes: &[FrontendNote]) -> Result<NoteFeatures> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("note_frontend.encode");
        let width = self.configuration.width();
        let size = notes
            .len()
            .checked_mul(width)
            .ok_or_else(|| VoiceError::new("Note feature matrix dimensions overflow"))?;
        let mut output = NoteFeatures {
            values: vec![0.0; size],
            width,
            frame_counts: Vec::with_capacity(notes.len()),
            frame_positions: Vec::new(),
        };
        for (note, row) in notes.iter().zip(output.values.chunks_exact_mut(width)) {
            let duration = duration_seconds(note)?;
            let count = frame_count(note)?;
            let pitch = note.pitch + note.pitch_offset;
            if !pitch.is_finite() {
                return Err(VoiceError::new("Note conditioning pitch is not finite"));
            }
            row[0] = if note.phones.len() == 1
                && (note.kind == 1 || matches!(note.phones[0].as_str(), "br" | "sil" | "pau"))
            {
                1.0
            } else {
                0.0
            };
            row[1] = if note.kind == 2 { 1.0 } else { 0.0 };
            row[2] = pitch;
            row[3] = (duration + 0.001).min(5.0).ln() as f32;
            row[4] = if note.flag { 1.0 } else { 0.0 };
            if let Some(tag) = note.tag.as_ref()
                && let Some(index) = self
                    .configuration
                    .tags
                    .iter()
                    .position(|value| value == tag)
            {
                row[5 + index] = 1.0;
            }
            output
                .frame_positions
                .try_reserve(count)
                .map_err(|_| VoiceError::new("Note frame expansion is too large"))?;
            output.frame_positions.extend(0..count);
            output.frame_counts.push(count);
        }
        Ok(output)
    }
}
