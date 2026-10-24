use super::Editor;
use crate::{model::EntityId,synthesis::unison::UnisonSettings};
use serde_json::{Value,Map};

impl Editor {
    pub fn unison_settings(&self)->Result<(EntityId,UnisonSettings),String> {
        let track=self.current_track();
        if track.is_instrumental(){return Err("伴奏轨没有齐唱设置".into())}
        let reference=self.active_ref().unwrap_or(&track.main_ref);
        let settings=UnisonSettings::from_voice(reference.extra.get("voice")).map_err(|e|e.to_string())?;
        Ok((reference.id,settings))
    }

    pub fn set_unison(&mut self,id:EntityId,next:UnisonSettings)->Result<bool,String> {
        if self.is_editing(){return Err("请先完成当前编辑".into())}
        if !(1..=16).contains(&next.count)||!next.spacing.is_finite()||!(0.5..=2.0).contains(&next.spacing){return Err("齐唱参数超出范围".into())}
        let (reference_id,previous)=self.unison_settings()?;
        if id!=reference_id{return Err("选中的音符组已经改变".into())}
        if previous==next{return Ok(false)}
        if next.full_size!=previous.full_size||next.center!=previous.center {
            return Err("齐唱成员子集由工程设置管理".into())
        }
        let reference=self.active_ref().unwrap_or(&self.current_track().main_ref);
        let mut extra=reference.extra.clone();
        let voice=extra.entry("voice".into()).or_insert_with(||Value::Object(Map::new()))
            .as_object_mut().ok_or("voice 字段必须是对象")?;
        if next.count!=previous.count {
            if next.count==1 {voice.remove("choirNumStems");}
            else{voice.insert("choirNumStems".into(),Value::from(next.count));}
        }
        if next.part!=previous.part {voice.insert("choirPartName".into(),Value::from(next.part));}
        if next.seed!=previous.seed {voice.insert("choirSeatingSeed".into(),Value::from(next.seed));}
        if next.spacing!=previous.spacing {voice.insert("choirSeatingSeparation".into(),Value::from(next.spacing));}
        self.begin("调整齐唱");
        let track=self.project.tracks.iter_mut().find(|t|t.id==self.track).expect("selected track exists");
        std::iter::once(&mut track.main_ref).chain(&mut track.groups)
            .find(|reference|reference.id==id).expect("validated reference").extra=extra;
        self.finish();Ok(true)
    }
}
