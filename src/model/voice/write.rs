use super::{Extra, VocalModeComponent, VocalModeValues, VoiceParameter, VoiceSettings};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

impl VoiceSettings {
    pub fn write_scalar(
        extra: &mut Extra,
        parameter: VoiceParameter,
        value: Option<f64>,
    ) -> Result<(), String> {
        if value.is_some_and(|value| !parameter.valid(value)) {
            return Err(format!("歌声参数 {} 超出有效范围", parameter.label()));
        }
        let voice = voice_mut(extra)?;
        if let Some(value) = value {
            voice.insert(parameter.key().into(), value.into());
        } else {
            voice.remove(parameter.key());
        }
        Ok(())
    }

    /// Changes only the supplied styles and preserves other fields in each object.
    pub fn write_vocal_modes(
        extra: &mut Extra,
        values: &BTreeMap<String, VocalModeValues>,
    ) -> Result<(), String> {
        if values.values().any(|value| !value.valid()) {
            return Err("唱法参数必须为 0 到 150 之间的有限数值".into());
        }
        Self::from_extra(extra)?;
        let voice = voice_mut(extra)?;
        let modes = voice
            .entry("vocalModeParams")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("validated mode map");
        for (name, value) in values {
            let fields = modes
                .entry(name)
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .expect("validated mode object");
            for component in VocalModeComponent::ALL {
                fields.insert(component.key().into(), value.component(component).into());
            }
        }
        voice.insert("vocalModePreset".into(), "".into());
        Ok(())
    }
}

fn voice_mut(extra: &mut Extra) -> Result<&mut Map<String, Value>, String> {
    extra
        .entry("voice".into())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "歌声参数 voice 必须是对象".into())
}
