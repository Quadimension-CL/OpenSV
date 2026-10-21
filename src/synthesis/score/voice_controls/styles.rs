use super::{PreparedVoiceControls, control_buffer, validate_clock};
use crate::{
    model::VocalModeComponent,
    synthesis::{
        inference::Sequence,
        voice::{Result, VoiceError, VoiceFamily},
    },
};

impl PreparedVoiceControls {
    pub fn sample_styles(
        &self,
        names: &[String],
        component: VocalModeComponent,
        start_seconds: f64,
        frame_seconds: f64,
        frames: usize,
    ) -> Result<Option<Sequence>> {
        validate_clock(start_seconds, frame_seconds, frames)?;
        if self.family != VoiceFamily::Sv2 && component != VocalModeComponent::Timbre {
            return Ok(None);
        }
        let fixed = self
            .styles
            .iter()
            .filter_map(|(name, values)| (values.component(component) != 0.0).then_some(name));
        let curves = self
            .automations
            .iter()
            .flat_map(|curves| curves.vocal_mode_names_for(component));
        let mut configured = false;
        for name in fixed.chain(curves) {
            if !names.contains(name) {
                return Err(VoiceError::new(format!(
                    "当前声库不支持唱法 {name} 的{}参数。",
                    component.label()
                )));
            }
            configured = true;
        }
        if !configured {
            return Ok(None);
        }
        let count = frames
            .checked_mul(names.len())
            .ok_or_else(|| VoiceError::new("Voice style frame count overflows"))?;
        let mut values = control_buffer(count)?;
        values.resize(count, 0.0);
        let mut active = false;
        for (column, name) in names.iter().enumerate() {
            let baseline = self
                .styles
                .get(name)
                .map_or(0.0, |value| value.component(component));
            let sampled = self
                .automations
                .as_ref()
                .filter(|curves| curves.has_vocal_mode_for(name, component))
                .map(|curves| {
                    if component == VocalModeComponent::Pitch {
                        curves.sample_pitch_vocal_mode(name, start_seconds, frame_seconds, frames)
                    } else {
                        curves.sample_vocal_mode(name, start_seconds, frame_seconds, frames)
                    }
                })
                .transpose()?;
            if sampled
                .as_ref()
                .is_some_and(|sampled| sampled.len() != frames)
            {
                return Err(VoiceError::new("唱法曲线的帧数与音频时间线不一致。"));
            }
            for row in 0..frames {
                let offset = sampled
                    .as_ref()
                    .map_or(0.0, |values| f64::from(values[row]));
                let value = (baseline + offset) / 100.0;
                let value = if component == VocalModeComponent::Pitch {
                    value as f32
                } else {
                    value.clamp(0.0, 1.5) as f32
                };
                if !value.is_finite() {
                    return Err(VoiceError::new("唱法曲线产生了无效数值。"));
                }
                values[row * names.len() + column] = value;
                active |= value != 0.0;
            }
        }
        if !active {
            return Ok(None);
        }
        Ok(Some(Sequence::new(frames, names.len(), values)?))
    }
}
