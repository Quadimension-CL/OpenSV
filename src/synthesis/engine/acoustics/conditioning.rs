use super::PreparedRenderAcoustics;
use crate::synthesis::engine::PreparedModelFeatures;
use crate::synthesis::stages::{
    f0::expression::expression_controls,
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
            .zip(&self.prepared.phrase.retake_seeds)
            .map(|((note, frames), seeds)| AcousticNote {
                frames,
                timbre_seed: seeds.map(|seeds| seeds.timbre),
                kind: note.kind,
                factor: match &self.prepared.features {
                    // SV1's native frontend uses its own neutral factor of 1.
                    PreparedModelFeatures::Sv1Local => None,
                    PreparedModelFeatures::Sv2 { .. } => expression_controls(
                        note.kind,
                        note.performance.expression_x,
                        note.performance.expression_y,
                    )
                    .map(|controls| controls.position_b),
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
