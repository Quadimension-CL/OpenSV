use super::{PreparedVoiceControls, control_buffer, validate_clock};
use crate::{
    model::{Parameter, VoiceParameter},
    synthesis::{
        stages::{timbre::records::FrameControl, vocoder::VoiceControl},
        voice::{Result, VoiceError, VoiceFamily},
    },
};

impl PreparedVoiceControls {
    pub fn sample_timbre_controls(
        &self,
        start_seconds: f64,
        frame_seconds: f64,
        frames: usize,
    ) -> Result<Option<Vec<FrameControl>>> {
        validate_clock(start_seconds, frame_seconds, frames)?;
        if self.scalar(VoiceParameter::ToneShift) == 0.0
            && !self.has_curve(Parameter::ToneShift)
            && !self.has_curve(Parameter::Mouth)
        {
            return Ok(None);
        }
        let cents =
            self.sample_parameter(Parameter::ToneShift, start_seconds, frame_seconds, frames)?;
        let mouth =
            self.sample_parameter(Parameter::Mouth, start_seconds, frame_seconds, frames)?;
        let mut values = control_buffer(frames)?;
        let mut active = false;
        for (cents, mouth) in cents.into_iter().zip(mouth) {
            let mouth = mouth as f32;
            let mouth = if (-0.01..0.01).contains(&mouth) {
                0.0
            } else {
                mouth
            };
            if self.family != VoiceFamily::Sv2 && mouth != 0.0 {
                return Err(VoiceError::new(
                    "一代及一代兼容歌声暂不支持口型参数曲线；请使用二代歌声或清除口型曲线。",
                ));
            }
            let pitch_ratio = ((cents / 1200.0).exp2() as f32)
                .clamp(f32::from_bits(0x3f35_0481), f32::from_bits(0x3fb5_0481));
            if !pitch_ratio.is_finite() || !mouth.is_finite() {
                return Err(VoiceError::new("音色参数曲线产生了无效数值。"));
            }
            active |= pitch_ratio != 1.0 || mouth != 0.0;
            values.push(FrameControl {
                pitch_ratio,
                scalars: [mouth, 0.0],
            });
        }
        Ok(active.then_some(values))
    }

    pub fn sample_vocoder_controls(
        &self,
        start_seconds: f64,
        frame_seconds: f64,
        frames: usize,
    ) -> Result<Option<Vec<VoiceControl>>> {
        validate_clock(start_seconds, frame_seconds, frames)?;
        if [
            VoiceParameter::Loudness,
            VoiceParameter::Tension,
            VoiceParameter::Breathiness,
            VoiceParameter::Gender,
        ]
        .into_iter()
        .all(|parameter| self.scalar(parameter) == 0.0)
            && [
                Parameter::Loudness,
                Parameter::Tension,
                Parameter::Breathiness,
                Parameter::Voicing,
                Parameter::Gender,
            ]
            .into_iter()
            .all(|parameter| !self.has_curve(parameter))
        {
            return Ok(None);
        }
        let loudness =
            self.sample_parameter(Parameter::Loudness, start_seconds, frame_seconds, frames)?;
        let tension =
            self.sample_parameter(Parameter::Tension, start_seconds, frame_seconds, frames)?;
        let breathiness =
            self.sample_parameter(Parameter::Breathiness, start_seconds, frame_seconds, frames)?;
        let voicing =
            self.sample_parameter(Parameter::Voicing, start_seconds, frame_seconds, frames)?;
        let gender =
            self.sample_parameter(Parameter::Gender, start_seconds, frame_seconds, frames)?;
        let mut values = control_buffer(frames)?;
        let mut active = false;
        for i in 0..frames {
            let loudness = loudness[i];
            let tension = tension[i];
            let breathiness = breathiness[i];
            let gender = gender[i];
            let value = VoiceControl {
                tension: -tension.clamp(-1.0, 1.0) as f32,
                primary_gain: (voicing[i].max(0.0)
                    * 10.0_f64.powf((loudness - 6.0 * tension - 5.5 * breathiness.max(0.0)) / 20.0))
                    as f32,
                secondary_gain: 10.0_f64.powf((loudness + 7.5 * breathiness) / 20.0) as f32,
                formant_ratio: (-0.5 * gender).exp() as f32,
            };
            if [
                value.tension,
                value.primary_gain,
                value.secondary_gain,
                value.formant_ratio,
            ]
            .iter()
            .any(|value| !value.is_finite())
            {
                return Err(VoiceError::new("歌声参数曲线产生了无效数值。"));
            }
            active |= value.tension != 0.0
                || value.primary_gain != 1.0
                || value.secondary_gain != 1.0
                || value.formant_ratio != 1.0;
            values.push(value);
        }
        Ok(active.then_some(values))
    }

    fn sample_parameter(
        &self,
        parameter: Parameter,
        start_seconds: f64,
        frame_seconds: f64,
        frames: usize,
    ) -> Result<Vec<f64>> {
        let baseline = match parameter {
            Parameter::Loudness => self.scalar(VoiceParameter::Loudness),
            Parameter::Tension => self.scalar(VoiceParameter::Tension),
            Parameter::Breathiness => self.scalar(VoiceParameter::Breathiness),
            Parameter::Gender => self.scalar(VoiceParameter::Gender),
            Parameter::ToneShift => self.scalar(VoiceParameter::ToneShift),
            _ => 0.0,
        };
        let sampled = self
            .automations
            .as_ref()
            .filter(|curves| curves.has_parameter(parameter))
            .map(|curves| curves.sample_parameter(parameter, start_seconds, frame_seconds, frames))
            .transpose()?;
        let mut values = control_buffer(frames)?;
        if let Some(sampled) = sampled {
            if sampled.len() != frames {
                return Err(VoiceError::new("参数曲线的帧数与音频时间线不一致。"));
            }
            values.extend(sampled.into_iter().map(f64::from));
        } else {
            let default = if parameter == Parameter::Voicing {
                1.0
            } else {
                0.0
            };
            values.resize(frames, baseline + default);
        }
        Ok(values)
    }

    fn has_curve(&self, parameter: Parameter) -> bool {
        self.automations
            .as_ref()
            .is_some_and(|curves| curves.has_parameter(parameter))
    }
}
