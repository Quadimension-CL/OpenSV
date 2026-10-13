use super::{Editor, SelectionState};
use crate::model::{EntityId, GroupRef, VocalModeValues, VoiceParameter, VoiceSettings};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VoiceEditParameter {
    Scalar(VoiceParameter),
    VocalMode(String),
}

pub struct VoiceParameterState {
    pub values: VoiceSettings,
}

pub(super) struct VoiceEdit {
    parameter: VoiceEditParameter,
    selection: SelectionState,
    reference: EntityId,
}

impl Editor {
    pub fn voice_parameter_state(&self) -> Result<Option<VoiceParameterState>, String> {
        if self.current_track().is_instrumental() {
            return Ok(None);
        }
        Ok(Some(VoiceParameterState {
            values: VoiceSettings::resolve(&self.voice_target().extra)?,
        }))
    }

    pub fn begin_voice_edit(&mut self, parameter: VoiceEditParameter) -> bool {
        if self.is_editing() || !matches!(self.voice_parameter_state(), Ok(Some(_))) {
            return false;
        }
        let reference = self.voice_target().id;
        self.begin(match parameter {
            VoiceEditParameter::Scalar(VoiceParameter::PitchShift | VoiceParameter::KeyShift) => {
                "调整调音参数"
            }
            VoiceEditParameter::Scalar(_) => "调整歌声参数",
            VoiceEditParameter::VocalMode(_) => "调整唱法",
        });
        self.voice_edit = Some(VoiceEdit {
            parameter,
            selection: self.selection(),
            reference,
        });
        true
    }

    pub fn update_voice_parameter(&mut self, parameter: VoiceParameter, value: f64) -> bool {
        if !parameter.valid(value) || !self.check_voice_edit(&VoiceEditParameter::Scalar(parameter))
        {
            return false;
        }
        if self
            .voice_parameter_state()
            .ok()
            .flatten()
            .is_some_and(|state| state.values.scalar(parameter) == Some(value))
        {
            return true;
        }
        VoiceSettings::write_scalar(&mut self.voice_target_mut().extra, parameter, Some(value))
            .is_ok()
    }

    pub fn update_vocal_mode(&mut self, name: &str, values: VocalModeValues) -> bool {
        if !values.valid() || !self.check_voice_edit(&VoiceEditParameter::VocalMode(name.into())) {
            return false;
        }
        let Ok(Some(state)) = self.voice_parameter_state() else {
            return false;
        };
        if state.values.style(name) == values {
            return true;
        }
        let target = &mut self.voice_target_mut().extra;
        VoiceSettings::write_vocal_modes(target, &BTreeMap::from([(name.into(), values)])).is_ok()
    }

    pub fn reset_unsupported_vocal_modes(
        &mut self,
        supported_names: &[String],
        enabled_components: [bool; 3],
    ) -> Result<(), String> {
        if self.is_editing() {
            return Err("请先完成当前编辑".into());
        }
        let state = self.voice_parameter_state()?.ok_or("伴奏轨不能设置唱法")?;
        let changes = state
            .values
            .vocal_modes
            .iter()
            .filter_map(|(name, values)| {
                let known = supported_names.contains(name);
                let mut changed = values.as_array();
                for (value, enabled) in changed.iter_mut().zip(enabled_components) {
                    if !known || !enabled {
                        *value = 0.0;
                    }
                }
                let changed = VocalModeValues::from_array(changed);
                (changed != *values).then(|| (name.clone(), changed))
            })
            .collect::<BTreeMap<_, _>>();
        if changes.is_empty() {
            return Ok(());
        }
        self.begin("重置不支持的唱法参数");
        let result = VoiceSettings::write_vocal_modes(&mut self.voice_target_mut().extra, &changes);
        if result.is_ok() {
            self.finish();
        } else {
            self.cancel();
        }
        result
    }

    pub fn has_voice_edit(&self) -> bool {
        self.voice_edit.is_some()
    }

    pub fn voice_edit_matches_selection(&self) -> bool {
        self.voice_edit.as_ref().is_some_and(|edit| {
            self.is_editing()
                && self.track == edit.selection.track
                && self.reference == edit.selection.reference
                && self.selected == edit.selection.notes
                && !self.current_track().is_instrumental()
                && self.voice_target().id == edit.reference
        })
    }

    pub fn finish_voice_edit(&mut self) {
        if self.has_voice_edit() {
            self.finish();
        }
    }

    pub fn cancel_voice_edit(&mut self) {
        if self.has_voice_edit() {
            self.cancel();
        }
    }

    fn check_voice_edit(&mut self, parameter: &VoiceEditParameter) -> bool {
        if !self.voice_edit_matches_selection() {
            self.finish_voice_edit();
            return false;
        }
        self.voice_edit
            .as_ref()
            .is_some_and(|edit| &edit.parameter == parameter)
    }

    fn voice_target(&self) -> &GroupRef {
        self.active_ref().unwrap_or(&self.current_track().main_ref)
    }

    fn voice_target_mut(&mut self) -> &mut GroupRef {
        let target_id = self.voice_target().id;
        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == self.track)
            .expect("selected track exists");
        std::iter::once(&mut track.main_ref)
            .chain(&mut track.groups)
            .find(|reference| reference.id == target_id)
            .expect("selected reference exists")
    }
}
