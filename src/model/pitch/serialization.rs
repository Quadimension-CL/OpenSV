use super::PitchControl;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use serde_json::Value;

impl Serialize for PitchControl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Tagged<'a, T: Serialize> {
            #[serde(rename = "type")]
            kind: &'static str,
            #[serde(flatten)]
            value: &'a T,
        }
        match self {
            Self::Point(value) => Tagged {
                kind: "point",
                value,
            }
            .serialize(serializer),
            Self::Curve(value) => Tagged {
                kind: "curve",
                value,
            }
            .serialize(serializer),
            Self::Unsupported(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for PitchControl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        match value.get("type").and_then(Value::as_str) {
            Some("point") => {
                value.as_object_mut().unwrap().remove("type");
                serde_json::from_value(value)
                    .map(Self::Point)
                    .map_err(D::Error::custom)
            }
            Some("curve") => {
                value.as_object_mut().unwrap().remove("type");
                serde_json::from_value(value)
                    .map(Self::Curve)
                    .map_err(D::Error::custom)
            }
            _ => Ok(Self::Unsupported(value)),
        }
    }
}
