use super::{Extra, Note};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteMode {
    #[default]
    Singing,
    Rap,
}

impl NoteMode {
    pub fn id(self) -> &'static str {
        match self {
            Self::Singing => "singing",
            Self::Rap => "rap",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "singing" => Ok(Self::Singing),
            "rap" => Ok(Self::Rap),
            _ => Err(format!("不支持的音符模式：{value}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct NotePerformance {
    pub mode: NoteMode,
    pub expression_x: f64,
    pub expression_y: f64,
    pub vibrato: f64,
    pub choir_onset: f32,
    pub choir_tail: f32,
    pub choir_pitch: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_tone: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_intonation: Option<f32>,
}

impl Default for NotePerformance {
    fn default() -> Self {
        Self {
            mode: NoteMode::Singing,
            expression_x: 0.0,
            expression_y: 0.0,
            vibrato: 1.0,
            choir_onset: 1.0,
            choir_tail: 1.0,
            choir_pitch: 1.0,
            r_tone: None,
            r_intonation: None,
        }
    }
}

impl NotePerformance {
    pub const EXPRESSION_MIN: f64 = -1.0;
    pub const EXPRESSION_MAX: f64 = 1.0;
    pub const VIBRATO_MIN: f64 = 0.0;
    pub const VIBRATO_MAX: f64 = 2.0;
    pub const RAP_TONE_MIN: f32 = -5.0;
    pub const RAP_TONE_MAX: f32 = 1.0;

    pub const RAP_INTONATION_MIN: f32 = -0.75;
    pub const RAP_INTONATION_MAX: f32 = 0.75;

    pub fn from_note(note: &Note) -> Result<Self, String> {
        Self::read_accent(&note.extra)?;
        Self::from_parts(&note.musical_type, &note.attributes)
    }

    pub fn read_accent(extra: &Extra) -> Result<Option<u8>, String> {
        match extra.get("accent") {
            None => Ok(None),
            Some(serde_json::Value::String(value)) if value.is_empty() => Ok(None),
            Some(serde_json::Value::String(value))
                if matches!(value.as_str(), "1" | "2" | "3" | "4" | "5") =>
            {
                Ok(Some(value.as_bytes()[0] - b'0'))
            }
            Some(_) => Err("说唱声调必须为空字符串或 1 到 5 的字符串".into()),
        }
    }

    pub fn from_parts(musical_type: &str, attributes: &Extra) -> Result<Self, String> {
        let value = Self {
            mode: NoteMode::parse(musical_type)?,
            expression_x: read_number(attributes, "expValueX", 0.0)?,
            expression_y: read_number(attributes, "expValueY", 0.0)?,
            vibrato: read_number(attributes, "dF0VbrMod", 1.0)?,
            choir_onset: read_number(attributes, "cTimeDispersion", 1.0)? as f32,
            choir_tail: read_number(attributes, "cPhraseTailDispersion", 1.0)? as f32,
            choir_pitch: read_number(attributes, "cPitchDispersion", 1.0)? as f32,
            r_tone: read_optional(attributes, "rTone")?,
            r_intonation: read_optional(attributes, "rIntonation")?,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !Self::valid_expression(self.expression_x, self.expression_y) {
            return Err("演绎坐标必须为 -1 到 1 之间的有限数值".into());
        }
        if !Self::valid_vibrato(self.vibrato) {
            return Err("颤音调制必须为 0 到 2 之间的有限数值".into());
        }
        if !Self::valid_rap_tone(self.r_tone) {
            return Err("说唱音调必须为 -5 到 1 之间的有限数值".into());
        }
        if !Self::valid_rap_intonation(self.r_intonation) {
            return Err("说唱语调必须为 -0.75 到 0.75 之间的有限数值".into());
        }
        if ![self.choir_onset, self.choir_tail, self.choir_pitch].into_iter().all(|v| (0.0..=2.0).contains(&v)) {
            return Err("合唱扩散必须为 0 到 2 之间的有限数值".into());
        }
        Ok(())
    }

    pub fn valid_expression(x: f64, y: f64) -> bool {
        [x, y]
            .into_iter()
            .all(|value| (Self::EXPRESSION_MIN..=Self::EXPRESSION_MAX).contains(&value))
    }

    pub fn valid_vibrato(value: f64) -> bool {
        (Self::VIBRATO_MIN..=Self::VIBRATO_MAX).contains(&value)
    }

    pub fn valid_rap_tone(value: Option<f32>) -> bool {
        value.is_none_or(|value| (Self::RAP_TONE_MIN..=Self::RAP_TONE_MAX).contains(&value))
    }

    pub fn valid_rap_intonation(value: Option<f32>) -> bool {
        value.is_none_or(|value| {
            (Self::RAP_INTONATION_MIN..=Self::RAP_INTONATION_MAX).contains(&value)
        })
    }

    pub fn write_expression(note: &mut Note, x: f64, y: f64) {
        write_number(&mut note.attributes, "expValueX", x, 0.0);
        write_number(&mut note.attributes, "expValueY", y, 0.0);
    }

    pub fn write_vibrato(note: &mut Note, value: f64) {
        write_number(&mut note.attributes, "dF0VbrMod", value, 1.0);
    }

    pub fn write_choir_spread(note: &mut Note, key: &str, value: f32) {
        debug_assert!(matches!(key, "cTimeDispersion" | "cPhraseTailDispersion" | "cPitchDispersion"));
        write_number(&mut note.attributes, key, f64::from(value), 1.0);
    }

    pub fn write_rap_tone(note: &mut Note, value: Option<f32>) {
        write_optional(&mut note.attributes, "rTone", value);
    }

    pub fn write_rap_intonation(note: &mut Note, value: Option<f32>) {
        write_optional(&mut note.attributes, "rIntonation", value);
    }
}

fn read_number(attributes: &Extra, key: &str, default: f64) -> Result<f64, String> {
    match attributes.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_f64()
            .map(|value| f64::from(value as f32))
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("音符属性 {key} 必须是有限数值")),
    }
}

fn read_optional(attributes: &Extra, key: &str) -> Result<Option<f32>, String> {
    attributes
        .contains_key(key)
        .then(|| read_number(attributes, key, 0.0).map(|value| value as f32))
        .transpose()
}

fn write_number(attributes: &mut Extra, key: &str, value: f64, default: f64) {
    let value = f64::from(value as f32);
    if value == default {
        attributes.remove(key);
    } else {
        attributes.insert(key.into(), value.into());
    }
}

fn write_optional(attributes: &mut Extra, key: &str, value: Option<f32>) {
    if let Some(value) = value {
        attributes.insert(key.into(), f64::from(value).into());
    } else {
        attributes.remove(key);
    }
}
