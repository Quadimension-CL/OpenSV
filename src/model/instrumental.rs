use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// External audio is placed in seconds; tempo edits never time-stretch it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentalTrack {
    pub clip: Option<AudioClip>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioClip {
    pub path: PathBuf,
    pub onset_seconds: f64,
    pub duration_seconds: f64,
}

impl AudioClip {
    pub fn end_seconds(&self) -> f64 {
        self.onset_seconds + self.duration_seconds
    }

    pub fn name(&self) -> &str {
        self.path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("伴奏")
    }

    pub fn valid(&self) -> bool {
        !self.path.as_os_str().is_empty()
            && self.onset_seconds.is_finite()
            && self.onset_seconds >= 0.0
            && self.duration_seconds.is_finite()
            && self.duration_seconds > 0.0
            && self.end_seconds() < i64::MAX as f64 / 48_000.0
    }
}
