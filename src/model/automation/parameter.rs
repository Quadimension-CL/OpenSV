use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Parameter {
    Pitch,
    Vibrato,
    Loudness,
    Tension,
    Breathiness,
    Voicing,
    Gender,
    ToneShift,
    Mouth,
}

impl Parameter {
    pub const ALL: [Self; 9] = [
        Self::Pitch,
        Self::Vibrato,
        Self::Loudness,
        Self::Tension,
        Self::Breathiness,
        Self::Voicing,
        Self::Gender,
        Self::ToneShift,
        Self::Mouth,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Pitch => "pitchDelta",
            Self::Vibrato => "vibratoEnv",
            Self::Loudness => "loudness",
            Self::Tension => "tension",
            Self::Breathiness => "breathiness",
            Self::Voicing => "voicing",
            Self::Gender => "gender",
            Self::ToneShift => "toneShift",
            Self::Mouth => "mouthOpening",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|parameter| parameter.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pitch => "音高偏差",
            Self::Vibrato => "颤音包络",
            Self::Loudness => "响度",
            Self::Tension => "张力",
            Self::Breathiness => "气声",
            Self::Voicing => "发声",
            Self::Gender => "性别",
            Self::ToneShift => "声区转换",
            Self::Mouth => "口型",
        }
    }

    pub fn range(self) -> (f64, f64) {
        match self {
            Self::Pitch => (-1200.0, 1200.0),
            Self::Loudness => (-48.0, 12.0),
            Self::ToneShift => (-400.0, 400.0),
            Self::Vibrato => (0.0, 2.0),
            Self::Voicing => (0.0, 1.0),
            Self::Tension | Self::Breathiness | Self::Gender | Self::Mouth => (-1.0, 1.0),
        }
    }

    pub fn default_value(self) -> f64 {
        match self {
            Self::Vibrato | Self::Voicing => 1.0,
            _ => 0.0,
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::Pitch | Self::ToneShift => "音分",
            Self::Loudness => "dB",
            _ => "",
        }
    }
}
