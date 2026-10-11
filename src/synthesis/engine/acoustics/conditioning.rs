use super::PreparedRenderAcoustics;
use crate::synthesis::engine::PreparedModelFeatures;
use crate::synthesis::stages::{
    f0::expression::neutral_singing_controls,
    timbre::records::{AcousticNote, PhoneImpulse},
};

impl PreparedRenderAcoustics {
    pub fn modulation(&self) -> Vec<AcousticNote> {
        let mut note_frames = vec![0_usize; self.prepared.phrase.notes.len()];
        for phone in &self.prepared.phones {
            // The preparation step already checked each note's frame sum.
            note_frames[phone.note_index] += phone.frames;
        }
        self.prepared
            .phrase
            .notes
            .iter()
            .zip(note_frames)
            .map(|(note, frames)| AcousticNote {
                frames,
                timbre_seed: None,
                kind: note.kind,
                factor: match &self.prepared.features {
                    // SV1's native frontend uses its own neutral factor of 1.
                    PreparedModelFeatures::Sv1Local => None,
                    PreparedModelFeatures::Sv2 { .. } => {
                        neutral_singing_controls(note.kind, note.flag)
                            .map(|controls| controls.position_b)
                    }
                },
            })
            .collect()
    }

    pub fn impulses(&self) -> Vec<Option<PhoneImpulse>> {
        self.duration
            .prediction
            .impulse_values
            .iter()
            .zip(&self.duration.prediction.impulse_positions)
            .map(|(&value, &position)| {
                (value > 0.0).then_some(PhoneImpulse {
                    value,
                    position: f64::from(position),
                })
            })
            .collect()
    }
}
