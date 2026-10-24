pub mod controls;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tonic {
    #[default]
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

impl Tonic {
    pub const ALL: [Self; 12] = [
        Self::C,
        Self::CSharp,
        Self::D,
        Self::DSharp,
        Self::E,
        Self::F,
        Self::FSharp,
        Self::G,
        Self::GSharp,
        Self::A,
        Self::ASharp,
        Self::B,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CSharp => "C# / Db",
            Self::DSharp => "D# / Eb",
            Self::FSharp => "F# / Gb",
            Self::GSharp => "G# / Ab",
            Self::ASharp => "A# / Bb",
            _ => self.short_label(),
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::C => "C",
            Self::CSharp => "C#",
            Self::D => "D",
            Self::DSharp => "D#",
            Self::E => "E",
            Self::F => "F",
            Self::FSharp => "F#",
            Self::G => "G",
            Self::GSharp => "G#",
            Self::A => "A",
            Self::ASharp => "A#",
            Self::B => "B",
        }
    }

    pub fn native_name(self) -> &'static str {
        self.short_label()
    }

    pub fn from_native(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|tonic| tonic.native_name() == value)
    }

    pub fn semitone(self) -> i32 {
        match self {
            Self::C => 0,
            Self::CSharp => 1,
            Self::D => 2,
            Self::DSharp => 3,
            Self::E => 4,
            Self::F => 5,
            Self::FSharp => 6,
            Self::G => 7,
            Self::GSharp => 8,
            Self::A => 9,
            Self::ASharp => 10,
            Self::B => 11,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scale {
    #[default]
    Major,
    NaturalMinor,
    HarmonicMinor,
    MelodicMinor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
}

impl Scale {
    pub const ALL: [Self; 12] = [
        Self::Major,
        Self::NaturalMinor,
        Self::HarmonicMinor,
        Self::MelodicMinor,
        Self::Dorian,
        Self::Phrygian,
        Self::Lydian,
        Self::Mixolydian,
        Self::Locrian,
        Self::MajorPentatonic,
        Self::MinorPentatonic,
        Self::Blues,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Major => crate::i18n::t("大调音阶"),
            Self::NaturalMinor => crate::i18n::t("自然小调"),
            Self::HarmonicMinor => crate::i18n::t("和声小调"),
            Self::MelodicMinor => crate::i18n::t("旋律小调（上行）"),
            Self::Dorian => crate::i18n::t("多利亚调式"),
            Self::Phrygian => crate::i18n::t("弗里几亚调式"),
            Self::Lydian => crate::i18n::t("利底亚调式"),
            Self::Mixolydian => crate::i18n::t("混合利底亚调式"),
            Self::Locrian => crate::i18n::t("洛克里亚调式"),
            Self::MajorPentatonic => crate::i18n::t("大调五声音阶"),
            Self::MinorPentatonic => crate::i18n::t("小调五声音阶"),
            Self::Blues => crate::i18n::t("布鲁斯音阶"),
        }
    }

    pub fn native_name(self) -> &'static str {
        match self {
            Self::Major => "Major",
            Self::NaturalMinor => "NaturalMinor",
            Self::HarmonicMinor => "HarmonicMinor",
            Self::MelodicMinor => "MelodicMinor",
            Self::Dorian => "Dorian",
            Self::Phrygian => "Phrygian",
            Self::Lydian => "Lydian",
            Self::Mixolydian => "Mixolydian",
            Self::Locrian => "Locrian",
            Self::MajorPentatonic => "MajorPentatonic",
            Self::MinorPentatonic => "MinorPentatonic",
            Self::Blues => "MinorBlues",
        }
    }

    pub fn from_native(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|scale| scale.native_name() == value)
    }

    fn intervals(self) -> &'static [i32] {
        match self {
            Self::Major => &[0, 2, 4, 5, 7, 9, 11],
            Self::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
            Self::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Self::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Self::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Self::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Self::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Self::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Self::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Self::MajorPentatonic => &[0, 2, 4, 7, 9],
            Self::MinorPentatonic => &[0, 3, 5, 7, 10],
            Self::Blues => &[0, 3, 5, 6, 7, 10],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleDisplay {
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScaleGuide {
    pub enabled: bool,
    pub tonic: Tonic,
    pub scale: Scale,
}

impl ScaleGuide {
    pub fn contains(self, pitch: i32) -> bool {
        self.scale
            .intervals()
            .contains(&self.relative_semitone(pitch))
    }

    pub fn is_tonic(self, pitch: i32) -> bool {
        self.relative_semitone(pitch) == 0
    }

    fn relative_semitone(self, pitch: i32) -> i32 {
        (pitch.rem_euclid(12) - self.tonic.semitone()).rem_euclid(12)
    }
}
