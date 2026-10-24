use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum VoiceFamily {
    Sv1,
    Sv2Compatible,
    Sv2,
}

impl VoiceFamily {
    pub fn model_version(self) -> u8 {
        match self {
            Self::Sv1 | Self::Sv2Compatible => 1,
            Self::Sv2 => 2,
        }
    }

    pub fn requires_identity(self) -> bool {
        self != Self::Sv1
    }
}
