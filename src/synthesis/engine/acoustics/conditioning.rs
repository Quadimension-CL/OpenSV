use super::PreparedRenderAcoustics;
use crate::synthesis::engine::PreparedModelFeatures;
use crate::synthesis::{
    stages::{
        f0::{expression::expression_controls, frontend::frame_count},
        timbre::records::{AcousticNote, PhoneImpulse},
    },
    voice::{Result, VoiceError},
};

impl PreparedRenderAcoustics {
    pub fn modulation(&self) -> Result<Vec<AcousticNote>> {
        let note_frames = match &self.prepared.features {
            PreparedModelFeatures::Sv1Local => {
                let mut frames = vec![0_usize; self.prepared.phrase.notes.len()];
                for phone in &self.prepared.phones {
                    let count = frames.get_mut(phone.note_index).ok_or_else(|| {
                        VoiceError::new("Acoustic phone is outside its note records")
                    })?;
                    *count = count
                        .checked_add(phone.frames)
                        .ok_or_else(|| VoiceError::new("Acoustic note frame count overflows"))?;
                }
                frames
            }
            PreparedModelFeatures::Sv2 { .. } => self
                .prepared
                .phrase
                .notes
                .iter()
                .map(frame_count)
                .collect::<Result<Vec<_>>>()?,
        };
        Ok(self
            .prepared
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
            .collect())
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
