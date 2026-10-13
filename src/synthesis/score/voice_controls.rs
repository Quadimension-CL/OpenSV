//! Fixed voice settings expanded on the receiving model's frame clock.

use std::collections::BTreeMap;

use crate::{
    model::{VocalModeComponent, VocalModeValues, VoiceParameter, VoiceSettings},
    synthesis::{
        inference::Sequence,
        plan::OwnerInput,
        stages::{timbre::records::FrameControl, vocoder::VoiceControl},
        voice::{Result, VoiceError, VoiceFamily},
    },
};

#[derive(Clone, Debug)]
pub struct PreparedVoiceControls {
    scalars: [f64; 5],
    tuning_semitones: f64,
    styles: BTreeMap<String, VocalModeValues>,
}

impl PreparedVoiceControls {
    pub fn new(input: &OwnerInput, family: VoiceFamily) -> Result<Option<Self>> {
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
        if scalars.iter().all(|value| *value == 0.0) && tuning_semitones == 0.0 && styles.is_empty()
        {
            return Ok(None);
        }
        Ok(Some(Self {
            scalars,
            tuning_semitones,
            styles,
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

    pub fn sample_styles(
        &self,
        names: &[String],
        component: VocalModeComponent,
        start_seconds: f64,
        frame_seconds: f64,
        frames: usize,
    ) -> Result<Option<Sequence>> {
        validate_clock(start_seconds, frame_seconds, frames)?;
        let mut active = false;
        for (name, values) in &self.styles {
            if values.component(component) == 0.0 {
                continue;
            }
            if !names.contains(name) {
                return Err(VoiceError::new(format!(
                    "当前声库不支持唱法 {name} 的{}参数。",
                    component.label()
                )));
            }
            active = true;
        }
        if !active {
            return Ok(None);
        }
        let count = frames
            .checked_mul(names.len())
            .ok_or_else(|| VoiceError::new("Voice style frame count overflows"))?;
        let row: Vec<_> = names
            .iter()
            .map(|name| {
                self.styles
                    .get(name)
                    .map_or(0.0, |value| (value.component(component) / 100.0) as f32)
            })
            .collect();
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| VoiceError::new("Voice style frame count is too large"))?;
        for _ in 0..frames {
            values.extend_from_slice(&row);
        }
        Ok(Some(Sequence::new(frames, names.len(), values)?))
    }

    pub fn sample_timbre_controls(
        &self,
        start_seconds: f64,
        frame_seconds: f64,
        frames: usize,
    ) -> Result<Option<Vec<FrameControl>>> {
        validate_clock(start_seconds, frame_seconds, frames)?;
        let cents = self.scalar(VoiceParameter::ToneShift);
        if cents == 0.0 {
            return Ok(None);
        }
        let value = FrameControl {
            pitch_ratio: ((cents / 1200.0).exp2() as f32)
                .clamp(f32::from_bits(0x3f35_0481), f32::from_bits(0x3fb5_0481)),
            scalars: [0.0; 2],
        };
        Ok(Some(repeated(value, frames)?))
    }

    pub fn sample_vocoder_controls(
        &self,
        start_seconds: f64,
        frame_seconds: f64,
        frames: usize,
    ) -> Result<Option<Vec<VoiceControl>>> {
        validate_clock(start_seconds, frame_seconds, frames)?;
        let loudness = self.scalar(VoiceParameter::Loudness);
        let tension = self.scalar(VoiceParameter::Tension);
        let breathiness = self.scalar(VoiceParameter::Breathiness);
        let gender = self.scalar(VoiceParameter::Gender);
        if [loudness, tension, breathiness, gender]
            .into_iter()
            .all(|value| value == 0.0)
        {
            return Ok(None);
        }
        let value = VoiceControl {
            tension: -tension.clamp(-1.0, 1.0) as f32,
            primary_gain: 10.0_f64
                .powf((loudness - 6.0 * tension - 5.5 * breathiness.max(0.0)) / 20.0)
                as f32,
            secondary_gain: 10.0_f64.powf((loudness + 7.5 * breathiness) / 20.0) as f32,
            formant_ratio: (-0.5 * gender).exp() as f32,
        };
        Ok(Some(repeated(value, frames)?))
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

fn repeated<T: Clone>(value: T, count: usize) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| VoiceError::new("Voice control frame count is too large"))?;
    values.resize(count, value);
    Ok(values)
}
