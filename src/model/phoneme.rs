use super::{EntityId, Extra};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Stable phoneme identity on the source note, before syllable redistribution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct SourcePhone {
    pub note_id: EntityId,
    pub phone_index: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhonemeParameter {
    LeftOffset,
    Position,
    Activity,
    Strength,
}

impl PhonemeParameter {
    pub const ALL: [Self; 4] = [
        Self::LeftOffset,
        Self::Position,
        Self::Activity,
        Self::Strength,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::LeftOffset => "leftOffset",
            Self::Position => "position",
            Self::Activity => "activity",
            Self::Strength => "strength",
        }
    }

    pub fn valid(self, value: f64) -> bool {
        value.is_finite()
            && match self {
                Self::Position | Self::Activity => (0.0..=1.0).contains(&value),
                Self::Strength => (-1.0..=1.0).contains(&value),
                Self::LeftOffset => true,
            }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PhonemeAttributes {
    /// Actual left boundary in seconds relative to the source note's onset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_offset: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strength: Option<f64>,
}

impl PhonemeAttributes {
    pub fn get(self, parameter: PhonemeParameter) -> Option<f64> {
        match parameter {
            PhonemeParameter::LeftOffset => self.left_offset,
            PhonemeParameter::Position => self.position,
            PhonemeParameter::Activity => self.activity,
            PhonemeParameter::Strength => self.strength,
        }
    }

    pub fn set(&mut self, parameter: PhonemeParameter, value: Option<f64>) {
        match parameter {
            PhonemeParameter::LeftOffset => self.left_offset = value,
            PhonemeParameter::Position => self.position = value,
            PhonemeParameter::Activity => self.activity = value,
            PhonemeParameter::Strength => self.strength = value,
        }
    }

    pub fn validate(self) -> Result<(), String> {
        for parameter in PhonemeParameter::ALL {
            if let Some(value) = self.get(parameter)
                && !parameter.valid(value)
            {
                return Err(format!("音素属性 {} 的数值无效", parameter.key()));
            }
        }
        Ok(())
    }

    pub fn read(attributes: &Extra) -> Result<Vec<Self>, String> {
        entries(attributes)?
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(index, entry)| Self::from_entry(entry, index))
            .collect()
    }

    pub fn get_at(attributes: &Extra, index: usize) -> Result<Self, String> {
        entries(attributes)?
            .and_then(|entries| entries.get(index))
            .map_or_else(
                || Ok(Self::default()),
                |entry| Self::from_entry(entry, index),
            )
    }

    /// Update known fields while retaining every unknown field and other entry.
    pub fn set_at(attributes: &mut Extra, index: usize, value: Self) -> Result<(), String> {
        value.validate()?;
        Self::get_at(attributes, index)?;
        if value == Self::default()
            && entries(attributes)?.is_none_or(|entries| index >= entries.len())
        {
            return Ok(());
        }
        let required = index.checked_add(1).ok_or("音素索引超出支持范围")?;
        let entries = attributes
            .entry("phonemes".into())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or("音符属性 phonemes 必须是数组")?;
        if entries.len() < required {
            entries
                .try_reserve(required - entries.len())
                .map_err(|_| "音素属性数量超出支持范围")?;
            entries.resize_with(required, || {
                Value::Object(Map::from_iter([("strength".into(), 0.0.into())]))
            });
        }
        let entry = entries[index]
            .as_object_mut()
            .ok_or_else(|| format!("音素属性第 {} 项必须是对象", index + 1))?;
        for parameter in PhonemeParameter::ALL {
            let number = value
                .get(parameter)
                .or_else(|| (parameter == PhonemeParameter::Strength).then_some(0.0));
            if let Some(number) = number {
                entry.insert(parameter.key().into(), number.into());
            } else {
                entry.remove(parameter.key());
            }
        }
        while entries.last().is_some_and(|entry| {
            entry.as_object().is_some_and(|entry| {
                entry.is_empty()
                    || (entry.len() == 1
                        && entry.get("strength").and_then(Value::as_f64) == Some(0.0))
            })
        }) {
            entries.pop();
        }
        if entries.is_empty() {
            attributes.remove("phonemes");
        }
        Ok(())
    }

    pub fn update_at(
        attributes: &mut Extra,
        index: usize,
        parameter: PhonemeParameter,
        value: Option<f64>,
    ) -> Result<(), String> {
        let mut current = Self::get_at(attributes, index)?;
        current.set(parameter, value);
        Self::set_at(attributes, index, current)
    }

    pub fn reset_at(
        attributes: &mut Extra,
        index: usize,
        parameter: Option<PhonemeParameter>,
    ) -> Result<(), String> {
        if let Some(parameter) = parameter {
            Self::update_at(attributes, index, parameter, None)
        } else {
            Self::set_at(attributes, index, Self::default())
        }
    }

    fn from_entry(entry: &Value, index: usize) -> Result<Self, String> {
        let object = entry
            .as_object()
            .ok_or_else(|| format!("音素属性第 {} 项必须是对象", index + 1))?;
        let mut attributes = Self::default();
        for parameter in PhonemeParameter::ALL {
            let number = object
                .get(parameter.key())
                .map(|value| {
                    value
                        .as_f64()
                        .ok_or_else(|| format!("音素属性 {} 必须是有限数值", parameter.key()))
                })
                .transpose()?;
            attributes.set(parameter, number);
        }
        attributes.validate()?;
        Ok(attributes)
    }
}

fn entries(attributes: &Extra) -> Result<Option<&[Value]>, String> {
    attributes
        .get("phonemes")
        .map(|value| {
            value
                .as_array()
                .map(Vec::as_slice)
                .ok_or_else(|| "音符属性 phonemes 必须是数组".into())
        })
        .transpose()
}
