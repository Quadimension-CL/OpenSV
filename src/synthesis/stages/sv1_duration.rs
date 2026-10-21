//! Six-child duration program for SV1 NOFS models.
//!
//! Its conditioning is a stored model vector. It has neither a session identity
//! projection nor the impulse heads of the newer duration programs.

mod configuration;
mod encoding;
mod frontend;
mod postprocess;

use crate::synthesis::voice::{PreparedTree, Result, VoiceError};

use super::duration::{DurationPrediction, DurationRecord, DurationResult};
use configuration::Configuration;

pub struct Sv1DurationStage<'a> {
    tree: &'a PreparedTree,
    children: [usize; 6],
    configuration: Configuration,
}

pub struct Sv1DurationResult {
    pub result: DurationResult,
    /// Derived record targets after stop/semivowel time borrowing. The F0
    /// note clock consumes these values rather than the original score times.
    pub record_seconds: Vec<f64>,
}

impl<'a> Sv1DurationStage<'a> {
    pub fn prepare(tree: &'a PreparedTree) -> Result<Self> {
        let (configuration, children) = configuration::validate(tree)?;
        Ok(Self {
            tree,
            children,
            configuration,
        })
    }

    pub fn predict_explicit(&self, records: &[DurationRecord]) -> Result<Sv1DurationResult> {
        let phones = frontend::prepare(self.tree, self.children[0], records)?;
        let scalars = if phones.is_empty() {
            Vec::new()
        } else {
            let features =
                encoding::encode(self.tree, &self.children, self.configuration, &phones)?;
            let scalars = self.tree.execute_node(self.children[1], &features)?;
            if scalars.len() != phones.len() || scalars.iter().any(|value| !value.is_finite()) {
                return Err(VoiceError::new(
                    "SV1 duration must produce one finite scalar per phone",
                ));
            }
            scalars
        };
        let (durations, record_seconds) =
            postprocess::execute(self.tree, self.children[0], records, &scalars)?;
        let mut result = DurationResult::new(
            records,
            DurationPrediction {
                phones,
                scalars,
                impulse_values: Vec::new(),
                impulse_positions: Vec::new(),
                default_impulse_values: Vec::new(),
                default_impulse_positions: Vec::new(),
            },
            durations.clone(),
            durations,
        )?;
        result.supports_timing = false;
        result.supports_strength = false;
        Ok(Sv1DurationResult {
            result,
            record_seconds,
        })
    }
}
