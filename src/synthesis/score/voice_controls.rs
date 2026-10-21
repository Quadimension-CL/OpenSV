//! Voice settings and parameter curves sampled on the receiving model's frame clock.

mod scalars;
mod styles;

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{
    model::{Parameter, VocalModeValues, VoiceParameter, VoiceSettings},
    synthesis::{
        plan::OwnerInput,
        voice::{Result, VoiceError, VoiceFamily},
    },
};

use super::PreparedAutomations;

#[derive(Clone, Debug)]
pub struct PreparedVoiceControls {
    scalars: [f64; 5],
    tuning_semitones: f64,
    styles: BTreeMap<String, VocalModeValues>,
    automations: Option<Arc<PreparedAutomations>>,
    family: VoiceFamily,
}

impl PreparedVoiceControls {
    pub fn new(
        input: &OwnerInput,
        family: VoiceFamily,
        automations: Option<Arc<PreparedAutomations>>,
    ) -> Result<Option<Self>> {
        let settings =
            VoiceSettings::resolve(&input.voice.instance_options).map_err(VoiceError::new)?;
        if family != VoiceFamily::Sv2
            && settings
                .vocal_modes
                .values()
                .any(|style| style.pitch != 0.0 || style.pronunciation != 0.0)
        {
            return Err(VoiceError::new(
                "一代及一代兼容歌声仅支持唱法的音色参数；音高与发音参数需使用二代歌声。",
            ));
        }
        let scalars = VoiceParameter::BASIC.map(|parameter| {
            settings
                .scalar(parameter)
                .unwrap_or_else(|| parameter.default_value())
        });
        let tuning_semitones = settings.tuning_semitones();
        let styles: BTreeMap<_, _> = settings
            .vocal_modes
            .into_iter()
            .filter(|(_, value)| *value != VocalModeValues::DEFAULT)
            .collect();
        let has_curves = automations.as_ref().is_some_and(|curves| {
            [
                Parameter::Loudness,
                Parameter::Tension,
                Parameter::Breathiness,
                Parameter::Voicing,
                Parameter::Gender,
                Parameter::ToneShift,
                Parameter::Mouth,
            ]
            .into_iter()
            .any(|parameter| curves.has_parameter(parameter))
                || curves.vocal_mode_names().next().is_some()
        });
        if scalars.iter().all(|value| *value == 0.0)
            && tuning_semitones == 0.0
            && styles.is_empty()
            && !has_curves
        {
            return Ok(None);
        }
        Ok(Some(Self {
            scalars,
            tuning_semitones,
            styles,
            automations,
            family,
        }))
    }

    pub fn tuning_semitones(&self) -> f64 {
        self.tuning_semitones
    }

    pub fn memory_bytes(&self) -> usize {
        self.styles
            .keys()
            .map(|name| {
                std::mem::size_of::<(String, VocalModeValues)>()
                    + name.capacity()
                    + 3 * std::mem::size_of::<usize>()
            })
            .sum()
    }

    fn scalar(&self, parameter: VoiceParameter) -> f64 {
        let index = VoiceParameter::BASIC
            .iter()
            .position(|key| *key == parameter)
            .unwrap();
        self.scalars[index]
    }
}

fn validate_clock(start: f64, period: f64, count: usize) -> Result<()> {
    if !start.is_finite()
        || !period.is_finite()
        || period <= 0.0
        || (count > 0 && !(start + (count - 1) as f64 * period).is_finite())
    {
        return Err(VoiceError::new("Invalid voice control frame clock"));
    }
    Ok(())
}

fn control_buffer<T>(count: usize) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| VoiceError::new("Voice control frame count is too large"))?;
    Ok(values)
}
