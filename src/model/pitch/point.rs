use super::{Extra, fresh_id, valid_id};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PitchPoint {

    pub pos: i64,

    pub pitch: f64,
    pub id: String,
    #[serde(flatten)]
    pub extra: Extra,
}

impl PitchPoint {
    pub fn new(pos: i64, pitch: f64) -> Self {
        Self {
            pos,
            pitch,
            id: fresh_id(),
            extra: Extra::new(),
        }
    }

    pub fn valid(&self) -> bool {
        self.pitch.is_finite() && valid_id(&self.id)
    }
}
