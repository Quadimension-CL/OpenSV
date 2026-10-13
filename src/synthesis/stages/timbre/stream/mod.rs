//! Incremental execution of the acoustic model with persistent temporal state.
mod conditions;
mod features;
mod input;
mod program;
mod queue;

use std::sync::Arc;

use super::{
    PreparedTimbreFrames, TimbreConditions, TimbreNetwork, TimbreOutput, records::AcousticRecords,
};
use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};
use features::Features;
use input::Input;
use program::Program;

pub struct TimbreStream<'network, 'tree> {
    program: Program<'network, 'tree>,
}

impl<'network, 'tree> TimbreStream<'network, 'tree> {
    pub(super) fn from_records(
        network: &'network TimbreNetwork<'tree>,
        features: Sequence,
        records: &AcousticRecords,
    ) -> Result<Self> {
        Ok(Self {
            program: Program::new(
                network,
                Input::from_records(network, Features::Dense(features), records, None, None)?,
            ),
        })
    }

    pub(super) fn from_prepared(
        network: &'network TimbreNetwork<'tree>,
        source: Arc<PreparedTimbreFrames>,
        records: &AcousticRecords,
        primary_style_weights: Option<&Sequence>,
        secondary_style_weights: Option<&Sequence>,
    ) -> Result<Self> {
        if records.first_frame > source.len() || records.total_frames != source.len() {
            return Err(VoiceError::new(
                "Prepared timbre frames differ from their records",
            ));
        }
        let features = Features::Prepared {
            source,
            first_frame: records.first_frame,
        };
        Ok(Self {
            program: Program::new(
                network,
                Input::from_records(
                    network,
                    features,
                    records,
                    primary_style_weights,
                    secondary_style_weights,
                )?,
            ),
        })
    }

    pub(super) fn from_conditions(
        network: &'network TimbreNetwork<'tree>,
        input: TimbreConditions<'_>,
    ) -> Result<Self> {
        Ok(Self {
            program: Program::new(network, Input::from_conditions(network, input)?),
        })
    }

    /// Feed at most this many full-rate source rows. The output contains only
    /// newly stable acoustic/control rows; it can be empty while kernels wait
    /// for their right context. Final flush occurs only at the true source end.
    pub fn next(&mut self, max_input_frames: usize) -> Result<TimbreOutput> {
        self.program.advance(max_input_frames)
    }

    pub fn is_finished(&self) -> bool {
        self.program.consumed == self.program.input.features.frames()
    }

    pub fn first_frame(&self) -> usize {
        self.program.input.first_frame
    }

    pub fn total_frames(&self) -> usize {
        self.program.input.first_frame + self.program.input.features.frames()
    }

    /// Materialize a requested prefix through the same streaming numerical
    /// program. Stopping early drops state; it never fabricates a model ending.
    pub fn collect_until(mut self, end_frame: usize) -> Result<TimbreOutput> {
        let first_frame = self.first_frame();
        if end_frame <= first_frame || end_frame > self.total_frames() {
            return Err(VoiceError::new(
                "Timbre output end is outside the source timeline",
            ));
        }
        let count = end_frame - first_frame;
        let mut acoustics = Vec::with_capacity(count * 72);
        let mut controls = Vec::with_capacity(count * 2);
        while acoustics.len() / 72 < count {
            let output = self.next(128)?;
            let keep = output.acoustics.frames.min(count - acoustics.len() / 72);
            acoustics.extend_from_slice(&output.acoustics.values[..keep * 72]);
            let metadata = output.controls.as_ref().ok_or_else(|| {
                VoiceError::new("SV2 acoustic stream omitted its learned metadata")
            })?;
            controls.extend_from_slice(&metadata.values[..keep * 2]);
        }
        Ok(TimbreOutput {
            first_frame,
            acoustics: Sequence::from_values(72, acoustics)?,
            controls: Some(Sequence::from_values(2, controls)?),
        })
    }
}
