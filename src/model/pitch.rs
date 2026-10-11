//! Native SVP pitch controls in group-local blicks and absolute MIDI pitch.
//! Hand-drawn curve samples are relative to their anchor's time and pitch.

mod edit;
mod point;
mod serialization;

pub use edit::{erase_curves, replace_curve};
pub use point::PitchPoint;

use super::Extra;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum PitchControl {
    Point(PitchPoint),
    Curve(PitchCurve),
    /// Control types that the editor does not implement are preserved.
    Unsupported(Value),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PitchCurve {
    pub pos: i64,
    pub pitch: f64,
    pub id: String,
    #[serde(with = "super::automation::flat_points")]
    pub points: Vec<(i64, f64)>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl PitchCurve {
    pub fn valid(&self) -> bool {
        self.pitch.is_finite()
            && valid_id(&self.id)
            && self.points.len() >= 2
            && self.points.windows(2).all(|p| p[0].0 < p[1].0)
            && self.points.iter().all(|(time, pitch)| {
                self.pos.checked_add(*time).is_some()
                    && pitch.is_finite()
                    && (self.pitch + pitch).is_finite()
            })
    }

    pub fn range(&self) -> Option<(i64, i64)> {
        Some((
            self.pos.checked_add(self.points.first()?.0)?,
            self.pos.checked_add(self.points.last()?.0)?,
        ))
    }

    pub fn absolute_points(&self) -> impl Iterator<Item = (i64, f64)> + '_ {
        self.points
            .iter()
            .filter_map(|(time, pitch)| Some((self.pos.checked_add(*time)?, self.pitch + pitch)))
    }

    /// Samples a local absolute MIDI pitch only inside this curve's range.
    pub fn sample(&self, position: f64) -> Option<f64> {
        if !position.is_finite() {
            return None;
        }
        let time = position - self.pos as f64;
        let first = self.points.first()?;
        let last = self.points.last()?;
        if time < first.0 as f64 || time > last.0 as f64 {
            return None;
        }
        let index = self.points.partition_point(|p| (p.0 as f64) < time);
        if index == 0 {
            return Some(self.pitch + first.1);
        }
        let right = self.points.get(index).unwrap_or(last);
        let left = self.points[index - 1];
        let width = right.0 as f64 - left.0 as f64;
        if width <= 0.0 {
            return Some(self.pitch + right.1);
        }
        let fraction = ((time - left.0 as f64) / width).clamp(0.0, 1.0);
        Some(self.pitch + left.1 * (1.0 - fraction) + right.1 * fraction)
    }
}

fn fresh_id() -> String {
    (uuid::Uuid::new_v4().as_u128() as u64).to_string()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()) && id.parse::<u64>().is_ok()
}
