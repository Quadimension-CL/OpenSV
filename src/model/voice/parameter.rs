#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum VoiceParameter {
    Loudness,
    Tension,
    Breathiness,
    Gender,
    ToneShift,
    PitchShift,
    KeyShift,
}

impl VoiceParameter {
    pub const ALL: [Self; 7] = [
        Self::Loudness,
        Self::Tension,
        Self::Breathiness,
        Self::Gender,
        Self::ToneShift,
        Self::PitchShift,
        Self::KeyShift,
    ];

    pub const BASIC: [Self; 5] = [
        Self::Loudness,
        Self::Tension,
        Self::Breathiness,
        Self::Gender,
        Self::ToneShift,
    ];

    pub const TUNING: [Self; 2] = [Self::PitchShift, Self::KeyShift];

    pub fn key(self) -> &'static str {
        match self {
            Self::Loudness => "paramLoudness",
            Self::Tension => "paramTension",
            Self::Breathiness => "paramBreathiness",
            Self::Gender => "paramGender",
            Self::ToneShift => "paramToneShift",
            Self::PitchShift => "transposeCents",
            Self::KeyShift => "transposeSemitones",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Loudness => "响度",
            Self::Tension => "张力",
            Self::Breathiness => "气声",
            Self::Gender => "性别",
            Self::ToneShift => "声区转换",
            Self::PitchShift => "音高转换",
            Self::KeyShift => "调性转换",
        }
    }

    pub fn range(self) -> (f64, f64) {
        match self {
            Self::Loudness | Self::KeyShift => (-12.0, 12.0),
            Self::ToneShift => (-400.0, 400.0),
            Self::PitchShift => (-50.0, 50.0),
            Self::Tension | Self::Breathiness | Self::Gender => (-1.0, 1.0),
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::Loudness => "dB",
            Self::ToneShift | Self::PitchShift => "音分",
            Self::KeyShift => "半音",
            Self::Tension | Self::Breathiness | Self::Gender => "",
        }
    }

    pub fn step(self) -> f64 {
        match self {
            Self::Loudness => 0.05,
            Self::ToneShift | Self::PitchShift | Self::KeyShift => 1.0,
            Self::Tension | Self::Breathiness | Self::Gender => 0.005,
        }
    }

    pub fn default_value(self) -> f64 {
        0.0
    }

    pub fn valid(self, value: f64) -> bool {
        let (min, max) = self.range();
        (min..=max).contains(&value)
    }
}
