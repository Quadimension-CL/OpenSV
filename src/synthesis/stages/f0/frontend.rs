mod config;
mod timing;

pub use timing::frame_count;

use crate::synthesis::voice::{PreparedTree, Result, VoiceError};

use config::Configuration;
pub(crate) use timing::duration_seconds;

pub const NOTE_FRONTEND_TYPE_ID: [u8; 16] = [
    0xc8, 0x66, 0x33, 0x90, 0x32, 0x8f, 0xfd, 0xad, 0x9d, 0x6a, 0x2c, 0xd8, 0x16, 0x42, 0x63, 0x22,
];
pub const PHONETIC_NOTE_FRONTEND_TYPE_ID:[u8;16]=[
    0xae,0xc1,0xef,0x93,0xaa,0xe8,0x3a,0xb6,
    0x8f,0x0b,0xd7,0x2e,0xbe,0x6d,0x43,0xb1,
];

#[derive(Clone, Debug)]
pub struct FrontendNote {

    pub phones: Vec<String>,
    pub pitch: f32,
    pub pitch_offset: f32,
    pub duration_seconds: f64,

    pub frame_seconds: f64,

    pub kind: u32,

    pub flag: bool,

    pub tag: Option<String>,
    pub parts: u32,
    pub part_index: u32,

    pub appended_durations: Vec<f64>,

    pub performance: crate::model::NotePerformance,
}

pub struct NoteFrontend {
    configuration: Configuration,
    pub normalizer_node: usize,
    pub pitch_normalizer_node: usize,
}

pub struct NoteFeatures {

    pub values: Vec<f32>,
    pub width: usize,
    pub frame_counts: Vec<usize>,

    pub frame_positions: Vec<usize>,
}

impl NoteFrontend {
    pub fn prepare(tree: &PreparedTree, index: usize) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("note_frontend.prepare");
        let node = tree.node(index)?;
        let (normalizer,pitch_normalizer)=match (node.type_id,node.children.as_slice()) {
            (NOTE_FRONTEND_TYPE_ID,[_,_,normalizer,pitch_normalizer])=>(*normalizer,*pitch_normalizer),
            (PHONETIC_NOTE_FRONTEND_TYPE_ID,[map,normalizer,pitch_normalizer])=>{
                if tree.node(*map)?.type_id!=crate::synthesis::voice::PHONETIC_MAP_TYPE_ID {
                    return Err(VoiceError::new("Phonetic frontend lacks its named descriptor map"));
                }
                (*normalizer,*pitch_normalizer)
            }
            _=>return Err(VoiceError::new("Unsupported shared note frontend schema")),
        };
        let configuration = Configuration::parse(&tree.payload(index)?)?;
        let normalizer_node = normalizer;
        Ok(Self {
            configuration,
            normalizer_node,
            pitch_normalizer_node: pitch_normalizer,
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
