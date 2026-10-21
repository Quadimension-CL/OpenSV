mod edit;
mod interpolation;
mod parameter;
mod sampling;
mod target;

pub use interpolation::Interpolation;
pub use parameter::Parameter;
pub use sampling::AutomationSampler;
pub use target::AutomationTarget;

use super::Extra;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Automation {
    pub mode: String,
    #[serde(with = "flat_points")]
    pub points: Vec<(i64, f64)>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Automation {
    fn default() -> Self {
        Self {
            mode: Interpolation::Cubic.key().into(),
            points: Vec::new(),
            extra: Extra::new(),
        }
    }
}

impl Automation {
    pub fn interpolation(&self) -> Result<Interpolation, String> {
        Interpolation::parse(&self.mode)
    }

    pub fn validate(&self) -> Result<(), String> {
        self.interpolation()?;
        validate_points(&self.points)
    }

    pub fn sampler(&self, default: f64) -> Result<AutomationSampler<'_>, String> {
        AutomationSampler::new(self, default)
    }

    pub fn sample(&self, position: i64, default: f64) -> Result<f64, String> {
        self.sampler(default)?.sample(position)
    }
}

fn validate_points(points: &[(i64, f64)]) -> Result<(), String> {
    if points.iter().any(|(_, value)| !(*value as f32).is_finite()) {
        return Err("参数控制点的值必须为有限数".into());
    }
    if points.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err("参数控制点的时间必须严格递增".into());
    }
    Ok(())
}

pub(super) mod flat_points {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
    use serde_json::Value;

    pub fn serialize<S: Serializer>(
        points: &[(i64, f64)],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let values: Vec<Value> = points
            .iter()
            .flat_map(|(time, value)| [Value::from(*time), Value::from(*value)])
            .collect();
        values.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<(i64, f64)>, D::Error> {
        let values = Vec::<Value>::deserialize(deserializer)?;
        if !values.len().is_multiple_of(2) {
            return Err(D::Error::custom("参数控制点必须为 time/value 对"));
        }
        values
            .as_chunks::<2>()
            .0
            .iter()
            .map(|point| {
                Ok((
                    point[0]
                        .as_i64()
                        .ok_or_else(|| D::Error::custom("控制点时间必须为 i64"))?,
                    point[1]
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| D::Error::custom("控制点值必须为有限数"))?,
                ))
            })
            .collect()
    }
}
