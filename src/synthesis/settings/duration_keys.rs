use super::{SynthesisSettings, VoiceSelection};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceDurationKeyOverride {
    pub voice: VoiceSelection,
    pub enabled: bool,
    pub value: u64,
}

impl SynthesisSettings {
    pub fn voice_duration_key_override(&self, voice: &VoiceSelection) -> Option<&VoiceDurationKeyOverride> {
        self.voice_duration_key_overrides.iter().find(|entry| &entry.voice == voice)
    }

    pub fn duration_key_override_for(&self, voice: &VoiceSelection) -> Option<u64> {
        self.voice_duration_key_override(voice)
            .filter(|entry| entry.enabled)
            .map(|entry| entry.value)
            .or_else(|| self.duration_key_override())
    }

    pub fn set_voice_duration_key_override(&mut self, voice: VoiceSelection, enabled: bool, value: u64) {
        if let Some(entry) = self.voice_duration_key_overrides.iter_mut().find(|entry| entry.voice == voice) {
            entry.enabled = enabled;
            entry.value = value;
        } else {
            self.voice_duration_key_overrides.push(VoiceDurationKeyOverride { voice, enabled, value });
        }
    }
}
