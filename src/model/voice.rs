mod parameter;
mod write;

use super::Extra;
use std::collections::BTreeMap;

pub use parameter::VoiceParameter;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VocalModeComponent {
    Pitch,
    Timbre,
    Pronunciation,
}

impl VocalModeComponent {
    pub const ALL: [Self; 3] = [Self::Pitch, Self::Timbre, Self::Pronunciation];

    pub fn key(self) -> &'static str {
        match self {
            Self::Pitch => "pitch",
            Self::Timbre => "timbre",
            Self::Pronunciation => "pronunciation",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pitch => "音高",
            Self::Timbre => "音色",
            Self::Pronunciation => "发音",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VocalModeValues {
    pub pitch: f64,
    pub timbre: f64,
    pub pronunciation: f64,
}

impl VocalModeValues {
    pub const DEFAULT: Self = Self::uniform(0.0);
    pub const MIN: f64 = 0.0;
    pub const MAX: f64 = 150.0;

    pub const fn uniform(value: f64) -> Self {
        Self::from_array([value; 3])
    }

    pub const fn from_array(values: [f64; 3]) -> Self {
        Self {
            pitch: values[0],
            timbre: values[1],
            pronunciation: values[2],
        }
    }

    pub const fn as_array(self) -> [f64; 3] {
        [self.pitch, self.timbre, self.pronunciation]
    }

    pub fn component(self, component: VocalModeComponent) -> f64 {
        match component {
            VocalModeComponent::Pitch => self.pitch,
            VocalModeComponent::Timbre => self.timbre,
            VocalModeComponent::Pronunciation => self.pronunciation,
        }
    }

    pub fn set_component(&mut self, component: VocalModeComponent, value: f64) {
        match component {
            VocalModeComponent::Pitch => self.pitch = value,
            VocalModeComponent::Timbre => self.timbre = value,
            VocalModeComponent::Pronunciation => self.pronunciation = value,
        }
    }

    pub fn valid(self) -> bool {
        self.as_array()
            .into_iter()
            .all(|value| (Self::MIN..=Self::MAX).contains(&value))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VoiceSettings {
    scalars: BTreeMap<VoiceParameter, f64>,
    pub vocal_mode_inherited: bool,
    pub vocal_mode_preset: String,
    pub vocal_modes: BTreeMap<String, VocalModeValues>,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            scalars: BTreeMap::new(),
            vocal_mode_inherited: true,
            vocal_mode_preset: String::new(),
            vocal_modes: BTreeMap::new(),
        }
    }
}

impl VoiceSettings {
    /// Reads one reference and keeps absent scalar fields distinguishable from explicit values.
    pub fn from_extra(extra: &Extra) -> Result<Self, String> {
        let Some(value) = extra.get("voice") else {
            return Ok(Self::default());
        };
        let voice = value.as_object().ok_or("歌声参数 voice 必须是对象")?;
        let mut settings = Self::default();
        for parameter in VoiceParameter::ALL {
            if let Some(value) = voice.get(parameter.key()) {
                let value = value
                    .as_f64()
                    .filter(|value| parameter.valid(*value))
                    .ok_or_else(|| format!("歌声参数 {} 超出有效范围", parameter.label()))?;
                settings.scalars.insert(parameter, value);
            }
        }
        if let Some(value) = voice.get("vocalModeInherited") {
            settings.vocal_mode_inherited = value.as_bool().ok_or("唱法继承标记必须是布尔值")?;
        }
        if let Some(value) = voice.get("vocalModePreset") {
            settings.vocal_mode_preset = value.as_str().ok_or("唱法预设必须是字符串")?.into();
        }
        if let Some(value) = voice.get("vocalModeParams") {
            let modes = value.as_object().ok_or("唱法参数必须是对象")?;
            for (name, value) in modes {
                let fields = value
                    .as_object()
                    .ok_or_else(|| format!("唱法 {name} 的参数必须是对象"))?;
                let mut values = VocalModeValues::DEFAULT;
                for component in VocalModeComponent::ALL {
                    if let Some(value) = fields.get(component.key()) {
                        let value = value
                            .as_f64()
                            .filter(|value| {
                                (VocalModeValues::MIN..=VocalModeValues::MAX).contains(value)
                            })
                            .ok_or_else(|| {
                                format!("唱法 {name} 的{}必须为 0 到 150", component.label())
                            })?;
                        values.set_component(component, value);
                    }
                }
                settings.vocal_modes.insert(name.clone(), values);
            }
        }
        Ok(settings)
    }

    pub fn scalar(&self, parameter: VoiceParameter) -> Option<f64> {
        self.scalars.get(&parameter).copied()
    }

    pub fn tuning_semitones(&self) -> f64 {
        self.scalar(VoiceParameter::PitchShift).unwrap_or_default() / 100.0
            + self.scalar(VoiceParameter::KeyShift).unwrap_or_default()
    }

    pub fn style(&self, name: &str) -> VocalModeValues {
        self.vocal_modes.get(name).copied().unwrap_or_default()
    }

    /// Missing values use the neutral setting for this reference.
    pub fn resolve(extra: &Extra) -> Result<Self, String> {
        let mut result = Self::from_extra(extra)?;
        for parameter in VoiceParameter::ALL {
            result
                .scalars
                .entry(parameter)
                .or_insert_with(|| parameter.default_value());
        }
        Ok(result)
    }
}
