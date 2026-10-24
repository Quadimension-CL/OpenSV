use std::collections::BTreeMap;
use crate::synthesis::{plan::{OwnerInput,GlobalBlick},stages::duration::DurationResult,voice::{Result,VoiceError}};
use super::{PreparedPhrase,PhoneSource,FRAME_SECONDS};
mod strength;
pub use strength::{StrengthEnvelope,activity};

#[derive(Clone,Copy,Debug,Default)]
pub struct PhoneControl {
    pub onset_seconds:Option<f64>,
    pub position:Option<f32>,
    pub activity:Option<f32>,
    pub strength:Option<f32>,
}
pub(super) fn from_owner(input:&OwnerInput)->Result<BTreeMap<PhoneSource,PhoneControl>> {
    let mut result=BTreeMap::new();
    for note in &input.notes {
        let Some(value)=note.attributes.get("phonemes") else{continue};
        let rows=value.as_array().ok_or_else(||VoiceError::new("音素属性必须为数组"))?;
        let origin=input.tempo.seconds_at(GlobalBlick(note.onset.0+input.content_offset.0));
        for (phone_index,row) in rows.iter().enumerate() {
            let object=row.as_object().ok_or_else(||VoiceError::new("音素属性必须为对象"))?;
            if let Some(key)=object.keys().find(|k|!matches!(k.as_str(),"leftOffset"|"position"|"activity"|"strength") && !(k.as_str()=="alt" && object[*k].as_i64()==Some(0))) {
                return Err(VoiceError::new(format!("Unknown phoneme attribute {key}")));
            }
            let read=|key:&str|->Result<Option<f32>> {
                match object.get(key).filter(|v|!v.is_null()) {
                    None=>Ok(None),Some(v)=>v.as_f64().map(|v|v as f32).filter(|v|v.is_finite())
                        .map(Some).ok_or_else(||VoiceError::new(format!("Invalid phoneme {key}"))),
                }
            };
            result.insert(PhoneSource{note_id:note.id,phone_index},PhoneControl {
                onset_seconds:read("leftOffset")?.map(|v|origin+f64::from(v)),
                position:read("position")?.map(|v|v.clamp(0.0,1.0)),
                activity:read("activity")?.map(|v|v.clamp(0.0,1.0)),

                strength:read("strength")?.or_else(||(!object.contains_key("strength")).then_some(1.0))
                    .map(|v|v.clamp(-1.0,1.0)),
            });
        }
    }
    Ok(result)
}

pub fn apply(phrase:&PreparedPhrase,duration:&mut DurationResult)->Result<()> {
    duration.natural_frames=duration.durations.iter().map(|phone|phone.frames).collect();
    if phrase.phoneme_controls.is_empty(){return Ok(());}
    let sources:Vec<_>=duration.durations.iter().map(|p|phrase.phone_sources[p.record_index][p.phone_index]).collect();
    let mut natural=phrase.start_seconds;let mut previous=0_usize;
    for index in 0..duration.durations.len() {
        natural+=duration.durations[index].duration_seconds;
        let specified=sources.get(index+1).copied().flatten().and_then(|s|phrase.phoneme_controls.get(&s)).and_then(|c|c.onset_seconds);
        let boundary=(specified.unwrap_or(natural)-phrase.start_seconds)/FRAME_SECONDS;
        if !boundary.is_finite() || boundary>f64::from(i32::MAX){return Err(VoiceError::new("Phoneme boundary exceeds model clock"));}
        let rounded=(boundary+f64::from_bits(0x3fdf_ffff_ffff_ffff).copysign(boundary)).trunc().max(0.0) as usize;
        let end=rounded.max(if index==0{0}else{previous+1});
        duration.durations[index].frames=end-previous;
        duration.durations[index].duration_seconds=(end-previous) as f64*FRAME_SECONDS;
        previous=end;
    }
    Ok(())
}
