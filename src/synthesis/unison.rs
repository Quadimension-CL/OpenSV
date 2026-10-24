mod seating;
mod augmentation;
pub(crate) use augmentation::prepare_members;

#[cfg(feature = "acoustic-diagnostic")]
pub fn diagnostic_members(voice: &super::voice::PreparedVoice, phrase: &super::score::PreparedPhrase, seating: &SeatingPlan) -> super::voice::Result<Vec<super::score::PreparedPhrase>> {
    prepare_members(voice, phrase, seating)
}

use crate::model::RetakeSeeds;
use super::{plan::VoiceInput, score::PreparedPhrase, voice::{Result, Speaker, VoiceError}};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct UnisonSettings {
    pub count: usize,
    pub part: String,
    pub seed: u32,
    pub full_size: usize,
    pub center: i32,
    pub spacing: f32,
}

impl Default for UnisonSettings {
    fn default() -> Self {
        Self {count:1,part:String::new(),seed:0,full_size:0,center:0,spacing:0.7}
    }
}

#[derive(Clone, Debug)]
pub struct SeatingPlan {
    pub part: String,

    pub rows: Vec<Vec<usize>>,
    pub speakers: Vec<Option<usize>>,
    pub first: usize,
    pub count: usize,
}

impl UnisonSettings {
    pub fn from_input(input: &VoiceInput) -> Result<Self> {

        Self::from_voice(input.instance_options.get("voice"))
    }

    pub fn from_voice(value: Option<&Value>) -> Result<Self> {
        let mut settings=Self::default();
        let Some(value)=value else{return Ok(settings)};
        let object=value.as_object().ok_or_else(||VoiceError::new("voice 必须为对象"))?;
        for (key,value) in object {
            match key.as_str() {

                "choirNumStems" => settings.count=integer(value,key)?.clamp(1,16) as usize,
                "choirPartName" => settings.part=value.as_str().ok_or_else(||invalid(key))?.into(),
                "choirSeatingSeed" => settings.seed=integer(value,key)? as u32,
                "choirSeatingFullSize" => settings.full_size=usize::try_from(integer(value,key)?).map_err(|_|invalid(key))?,
                "choirSeatingCenterIndex" => settings.center=i32::try_from(integer(value,key)?).map_err(|_|invalid(key))?,
                "choirSeatingSeparation" => settings.spacing=value.as_f64().filter(|n|n.is_finite())
                    .map(|n|n as f32).filter(|n|n.is_finite()).ok_or_else(||invalid(key))?,
                _=>(),
            }
        }
        if settings.full_size>4096 {return Err(VoiceError::new("齐唱座位数量超出处理范围"))}
        Ok(settings)
    }

    pub fn seating(&self, speakers: &[Speaker]) -> Result<SeatingPlan> {
        seating::prepare(self,speakers)
    }

    pub fn member_phrase(&self, phrase: &PreparedPhrase, index: usize) -> Result<PreparedPhrase> {
        if index>=self.count {return Err(VoiceError::new("齐唱成员编号超出范围"))}
        let mut member=phrase.clone();
        let offset=(index as u32).wrapping_mul(7919);
        if offset!=0 {
            for (source,seeds) in member.source_note_ids.iter().zip(&mut member.retake_seeds) {
                if source.is_some() {
                    let old=seeds.unwrap_or_default();
                    *seeds=Some(RetakeSeeds {duration:old.duration.wrapping_add(offset),
                        pitch:old.pitch.wrapping_add(offset),timbre:old.timbre.wrapping_add(offset)});
                }
            }
        }
        Ok(member)
    }
}

fn integer(value: &Value,key: &str)->Result<i64> {value.as_i64().ok_or_else(||invalid(key))}
fn invalid(key:&str)->VoiceError {VoiceError::new(format!("齐唱参数 {key} 无效"))}
