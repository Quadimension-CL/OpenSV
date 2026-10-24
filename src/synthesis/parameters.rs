use crate::model::Extra;
use super::{plan::VoiceInput, voice::{Result, VoiceError}};

#[derive(Clone,Copy)]
pub struct ParameterSpec {
    pub key:&'static str,
    pub label:&'static str,
    pub limit:f32,
    pub step:f32,
    pub unit:&'static str,
}

pub const VOICE_CONTROLS:[ParameterSpec;5]=[
    ParameterSpec{key:"paramLoudness",label:"响度",limit:12.0,step:0.1,unit:"dB"},
    ParameterSpec{key:"paramTension",label:"张力",limit:1.0,step:0.005,unit:""},
    ParameterSpec{key:"paramBreathiness",label:"气声",limit:1.0,step:0.005,unit:""},
    ParameterSpec{key:"paramGender",label:"性别",limit:1.0,step:0.005,unit:""},
    ParameterSpec{key:"paramToneShift",label:"声区转换",limit:400.0,step:1.0,unit:"cents"},
];
pub const TUNING_CONTROLS:[ParameterSpec;2]=[
    ParameterSpec{key:"transposeCents",label:"音高转换",limit:50.0,step:0.1,unit:"cents"},
    ParameterSpec{key:"transposeSemitones",label:"调性转换",limit:12.0,step:1.0,unit:"半音"},
];
pub const CONSONANT_CONTROLS:[ParameterSpec;2]=[
    ParameterSpec{key:"consonantStrength",label:"力度",limit:0.5,step:0.0,unit:""},
    ParameterSpec{key:"consonantDuration",label:"时长",limit:0.9,step:0.0,unit:""},
];

#[derive(Clone,Copy,Debug,Default,PartialEq)]
pub struct VoiceParameters {
    pub loudness:f32,
    pub tension:f32,
    pub breathiness:f32,
    pub gender:f32,
    pub tone_shift:f32,
    pub consonant_strength:f32,
    pub consonant_duration:f32,
}
impl VoiceParameters {
    pub fn from_input(input:&VoiceInput)->Result<Self> {
        let read=|key:&str|->Result<f32>{
            input.instance_options.get("voice").and_then(|v|v.get(key))
                .map_or(Ok(0.0),|v|v.as_f64().filter(|v|v.is_finite()&&v.abs()<=1_000_000.0)
                    .map(|v|v as f32).ok_or_else(||VoiceError::new(format!("Invalid voice.{key}"))))
        };
        Ok(Self {loudness:read("paramLoudness")?,tension:read("paramTension")?,
            breathiness:read("paramBreathiness")?,gender:read("paramGender")?,tone_shift:read("paramToneShift")?,
            consonant_strength:read("consonantStrength")?,consonant_duration:read("consonantDuration")?})
    }
}

#[derive(Clone,Copy,Debug)]
pub struct FrameParameters {
    pub shape_control:f32,
    pub primary_gain:f32,
    pub secondary_gain:f32,
    pub gender_ratio:f32,
    pub tone_ratio:f32,
}
pub fn frame(phrase:&crate::synthesis::score::PreparedPhrase,seconds:f64)->FrameParameters {
    sample(phrase.voice_parameters,phrase.automation.as_ref(),seconds)
}
pub fn sample(p:VoiceParameters,curves:Option<&crate::synthesis::score::PreparedAutomation>,seconds:f64)->FrameParameters {
    let curve=|key:&str,default|curves.map_or(default,|c|c.value(key,seconds,default));
    let loudness=p.loudness+curve("loudness",0.0);
    let tension=p.tension+curve("tension",0.0);
    let breath=p.breathiness+curve("breathiness",0.0);
    let gender=p.gender+curve("gender",0.0);
    let tone=p.tone_shift+curve("toneShift",0.0);
    let primary_db=(loudness-6.0*tension)-5.5*breath.max(0.0);
    let secondary_db=loudness+7.5*breath;
    FrameParameters {
        shape_control:-tension.clamp(-1.0,1.0),
        primary_gain:curve("voicing",1.0).max(0.0)*10.0_f32.powf((f64::from(primary_db)/20.0) as f32),
        secondary_gain:10.0_f32.powf((f64::from(secondary_db)/20.0) as f32),
        gender_ratio:(-gender*0.5).exp(),
        tone_ratio:2.0_f32.powf(tone/1200.0).clamp(0.707_1,1.414_2),
    }
}

pub fn local_loudness(options: &Extra) -> Result<f32> {
    let Some(value) = options.get("voice").and_then(|voice| voice.get("paramLoudness")) else { return Ok(0.0); };
    let value = value.as_f64().filter(|v| v.is_finite() && (-12.0..=12.0).contains(v))
        .ok_or_else(|| VoiceError::new("响度必须在 -12..12 dB 范围内"))?;
    Ok(value as f32)
}

pub fn transpose(input:&VoiceInput)->Result<f32> {
    let value=|key:&str|->Result<f32>{
        let local=input.instance_options.get("voice").and_then(|v|v.get(key));
        match local {
            None=>Ok(0.0),
            Some(value)=>value.as_f64().filter(|v|v.is_finite() && v.abs()<=1_000_000.0)
                .map(|v|v as f32).ok_or_else(||VoiceError::new(format!("Invalid voice.{key}"))),
        }
    };
    Ok(value("transposeCents")?/100.0+value("transposeSemitones")?)
}
