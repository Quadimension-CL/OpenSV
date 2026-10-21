//! Owner parameter envelopes, prepared on the musical grid before sampling
//! the receiving model's frame clock.

mod prepare;
mod seconds_curve;

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    model::{Parameter, QUARTER, VocalModeComponent, VoiceSettings},
    synthesis::{
        plan::OwnerInput,
        voice::{Result, VoiceError},
    },
};

use prepare::{CurveGrid, prepare_curve, scalar_baseline};
use seconds_curve::SecondsCurve;

const ACOUSTIC_GRID: i64 = QUARTER / 80;
const PITCH_GRID: i64 = QUARTER / 64;

#[derive(Clone, Debug)]
pub struct PreparedAutomations {
    parameters: BTreeMap<String, SecondsCurve>,
    vocal_modes: BTreeMap<String, SecondsCurve>,
    pitch_vocal_modes: BTreeMap<String, SecondsCurve>,
}

impl PreparedAutomations {
    pub fn new(input: &OwnerInput) -> Result<Option<Self>> {
        if !input
            .automation
            .values()
            .chain(input.track_automation.values())
            .chain(input.vocal_modes.values())
            .chain(input.track_vocal_modes.values())
            .any(|curve| !curve.points.is_empty())
        {
            return Ok(None);
        }
        let mut begin = i64::MAX;
        let mut end = i64::MIN;
        for note in &input.notes {
            if super::options::muted(&note.attributes)? {
                continue;
            }
            let note_end = note
                .onset
                .0
                .checked_add(note.duration)
                .filter(|end| *end > note.onset.0)
                .ok_or_else(|| VoiceError::new("Invalid automation note range"))?;
            begin = begin.min(note.onset.0);
            end = end.max(note_end);
        }
        if begin >= end {
            return Ok(None);
        }
        let acoustic_begin = begin
            .checked_sub(QUARTER)
            .ok_or_else(|| VoiceError::new("Automation padding overflows"))?;
        let acoustic_end = end
            .checked_add(QUARTER)
            .ok_or_else(|| VoiceError::new("Automation padding overflows"))?;
        let settings =
            VoiceSettings::resolve(&input.voice.instance_options).map_err(VoiceError::new)?;
        let mut parameters = BTreeMap::new();
        let parameter_names: BTreeSet<_> = input
            .automation
            .keys()
            .chain(input.track_automation.keys())
            .collect();
        for name in parameter_names {
            let curve = input
                .automation
                .get(name)
                .filter(|curve| !curve.points.is_empty());
            let track_curve = input
                .track_automation
                .get(name)
                .filter(|curve| !curve.points.is_empty());
            if curve.is_none() && track_curve.is_none() {
                continue;
            }
            let parameter = Parameter::from_key(name)
                .ok_or_else(|| VoiceError::new(format!("Unknown automation {name}")))?;
            // The note modulation envelope combines vibrato on this same
            // musical grid before converting its values to seconds.
            if parameter == Parameter::Vibrato {
                continue;
            }
            parameters.insert(
                name.clone(),
                prepare_curve(
                    input,
                    curve,
                    track_curve,
                    CurveGrid {
                        default: parameter.default_value(),
                        baseline: scalar_baseline(&settings, parameter),
                        range: parameter.range(),
                        begin: acoustic_begin,
                        end: acoustic_end,
                        step: ACOUSTIC_GRID,
                        include_partial: true,
                        multiply: parameter == Parameter::Voicing,
                    },
                )?,
            );
        }
        let mut vocal_modes = BTreeMap::new();
        let mut pitch_vocal_modes = BTreeMap::new();
        let vocal_mode_names: BTreeSet<_> = input
            .vocal_modes
            .keys()
            .chain(input.track_vocal_modes.keys())
            .collect();
        for name in vocal_mode_names {
            let curve = input
                .vocal_modes
                .get(name)
                .filter(|curve| !curve.points.is_empty());
            let track_curve = input
                .track_vocal_modes
                .get(name)
                .filter(|curve| !curve.points.is_empty());
            if curve.is_some() || track_curve.is_some() {
                vocal_modes.insert(
                    name.clone(),
                    prepare_curve(
                        input,
                        curve,
                        track_curve,
                        CurveGrid {
                            default: 0.0,
                            baseline: 0.0,
                            range: (-150.0, 150.0),
                            begin: acoustic_begin,
                            end: acoustic_end,
                            step: ACOUSTIC_GRID,
                            include_partial: true,
                            multiply: false,
                        },
                    )?,
                );
            }
            // Pitch uses the local envelope. Track envelopes participate in
            // the separate acoustic pronunciation and timbre path.
            if let Some(curve) = curve {
                pitch_vocal_modes.insert(
                    name.clone(),
                    prepare_curve(
                        input,
                        Some(curve),
                        None,
                        CurveGrid {
                            default: 0.0,
                            baseline: 0.0,
                            range: (f64::NEG_INFINITY, f64::INFINITY),
                            begin,
                            end,
                            step: PITCH_GRID,
                            include_partial: false,
                            multiply: false,
                        },
                    )?,
                );
            }
        }
        if parameters.is_empty() && vocal_modes.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            parameters,
            vocal_modes,
            pitch_vocal_modes,
        }))
    }

    pub fn has_parameter(&self, parameter: Parameter) -> bool {
        self.parameters.contains_key(parameter.key())
    }

    pub fn has_vocal_mode(&self, name: &str) -> bool {
        self.vocal_modes
            .get(name)
            .is_some_and(SecondsCurve::has_nonzero_values)
    }

    pub fn vocal_mode_names(&self) -> impl Iterator<Item = &String> {
        self.vocal_modes
            .iter()
            .filter_map(|(name, curve)| curve.has_nonzero_values().then_some(name))
    }

    pub fn vocal_mode_names_for(
        &self,
        component: VocalModeComponent,
    ) -> impl Iterator<Item = &String> {
        let modes = if component == VocalModeComponent::Pitch {
            &self.pitch_vocal_modes
        } else {
            &self.vocal_modes
        };
        modes
            .iter()
            .filter_map(|(name, curve)| curve.has_nonzero_values().then_some(name))
    }

    pub fn has_vocal_mode_for(&self, name: &str, component: VocalModeComponent) -> bool {
        if component == VocalModeComponent::Pitch {
            self.pitch_vocal_modes
                .get(name)
                .is_some_and(SecondsCurve::has_nonzero_values)
        } else {
            self.has_vocal_mode(name)
        }
    }

    pub fn sample_parameter(
        &self,
        parameter: Parameter,
        start_seconds: f64,
        frame_seconds: f64,
        frames: usize,
    ) -> Result<Vec<f32>> {
        SecondsCurve::sample_optional(
            self.parameters.get(parameter.key()),
            parameter.default_value() as f32,
            start_seconds,
            frame_seconds,
            frames,
        )
    }

    pub fn sample_vocal_mode(
        &self,
        name: &str,
        start_seconds: f64,
        frame_seconds: f64,
        frames: usize,
    ) -> Result<Vec<f32>> {
        SecondsCurve::sample_optional(
            self.vocal_modes.get(name),
            0.0,
            start_seconds,
            frame_seconds,
            frames,
        )
    }

    pub fn sample_pitch_vocal_mode(
        &self,
        name: &str,
        start_seconds: f64,
        frame_seconds: f64,
        frames: usize,
    ) -> Result<Vec<f32>> {
        SecondsCurve::sample_optional(
            self.pitch_vocal_modes.get(name),
            0.0,
            start_seconds,
            frame_seconds,
            frames,
        )
    }

    pub fn memory_bytes(&self) -> usize {
        self.parameters
            .iter()
            .chain(&self.vocal_modes)
            .chain(&self.pitch_vocal_modes)
            .map(|(name, curve)| {
                name.capacity()
                    + curve.memory_bytes()
                    + std::mem::size_of::<(String, SecondsCurve)>()
                    + 3 * std::mem::size_of::<usize>()
            })
            .sum()
    }
}
