use crate::model::*;
use serde::Deserialize;

#[derive(Deserialize)]#[serde(rename_all="camelCase")]
struct Document {format_version:u32,project:Score}
#[derive(Deserialize)]#[serde(rename_all="camelCase")]
struct Score {name:String,tracks:Vec<Lane>,tempos:Vec<Speed>,time_signatures:Vec<Signature>,measure_prefix:i32}
#[derive(Deserialize)]struct Lane {name:String,notes:Vec<SourceNote>,pitch:Option<Pitch>}
#[derive(Deserialize)]#[serde(rename_all="camelCase")]
struct SourceNote {key:i32,tick_on:i64,tick_off:i64,lyric:String,phoneme:Option<String>}
#[derive(Deserialize)]#[serde(rename_all="camelCase")]
struct Speed {tick_position:i64,bpm:f64}
#[derive(Deserialize)]#[serde(rename_all="camelCase")]
struct Signature {measure_position:i32,numerator:u32,denominator:u32}
#[derive(Deserialize)]#[serde(rename_all="camelCase")]
struct Pitch {ticks:Vec<i64>,values:Vec<Option<f64>>,is_absolute:bool}
fn blick(tick:i64)->Result<i64,String>{tick.checked_mul(QUARTER/480).ok_or("导入工程时间超出范围".into())}

pub(super) fn project(value:serde_json::Value)->Result<Project,String> {
    let document:Document=serde_json::from_value(value).map_err(|e|format!("通用工程数据无效：{e}"))?;
    if document.format_version!=1{return Err("不支持的 UtaFormatix Data 版本".into());}
    let source=document.project;
    let mut p=Project::default();p.tracks.clear();
    p.render_config.insert("filename".into(),source.name.into());

    let _measure_prefix=source.measure_prefix;
    p.time.tempo=source.tempos.into_iter().map(|s|Ok(Tempo{position:blick(s.tick_position)?,bpm:s.bpm,extra:Default::default()})).collect::<Result<_,String>>()?;
    p.time.meter=source.time_signatures.into_iter().map(|s|Meter{index:s.measure_position,numerator:s.numerator,denominator:s.denominator,extra:Default::default()}).collect();
    if p.time.tempo.is_empty(){p.time.tempo=TimeAxis::default().tempo;}
    if p.time.meter.is_empty(){p.time.meter=TimeAxis::default().meter;}
    for lane in source.tracks {
        let mut track=Track::new(p.tracks.len());track.name=lane.name;
        for n in lane.notes {
            if !(0..=127).contains(&n.key)||n.tick_off<=n.tick_on{return Err("导入工程包含无效音符".into());}
            let start=blick(n.tick_on)?;let end=blick(n.tick_off)?;
            let mut note=Note::new(start,end.checked_sub(start).ok_or("导入音符时长超出范围")?,n.key);
            note.lyrics=n.lyric;note.phonemes=n.phoneme.unwrap_or_default();
            track.main_group.notes.push(note);
        }
        track.main_group.notes.sort_by_key(|n|n.onset);
        if let Some(pitch)=lane.pitch {apply_pitch(&mut track.main_group,pitch)?;}
        p.tracks.push(track);
    }
    if p.tracks.is_empty(){p.tracks.push(Track::new(0));}
    crate::storage::validate(&p)?;
    Ok(p)
}

fn apply_pitch(group:&mut Group,pitch:Pitch)->Result<(),String> {
    if pitch.ticks.len()!=pitch.values.len() || pitch.ticks.windows(2).any(|w|w[0]>w[1]) || pitch.values.iter().flatten().any(|v|!v.is_finite()) {
        return Err("导入工程的音高数据无效".into());
    }
    if !pitch.is_absolute && pitch.values.iter().any(Option::is_none){return Err("相对音高数据不能包含空值".into());}

    let mut samples:Vec<(i64,Option<f64>)>=Vec::new();
    for (&tick,&value) in pitch.ticks.iter().zip(&pitch.values) {
        if let Some(last)=samples.last_mut().filter(|p|p.0==tick){last.1=value;}
        else{samples.push((tick,value));}
    }
    let (ticks,values)=samples.into_iter().unzip();
    let pitch=Pitch{ticks,values,is_absolute:pitch.is_absolute};
    let end=group.notes.iter().map(|n|n.onset+n.duration).max().unwrap_or(0);
    let mut points=Vec::new();
    let flush=|points:&mut Vec<(i64,f64)>,group:&mut Group| {
        if points.len()>=2 {
            group.pitch_controls.push(PitchControl::Curve(PitchCurve{pos:0,pitch:0.0,
                id:(uuid::Uuid::new_v4().as_u128() as u64).to_string(),points:std::mem::take(points),extra:Default::default()}));
        }else{points.clear();}
    };
    for (i,(&tick,value)) in pitch.ticks.iter().zip(&pitch.values).enumerate() {
        let time=blick(tick)?;
        let next=pitch.ticks.get(i+1).map(|&t|blick(t)).transpose()?.unwrap_or(end.max(time+1));
        let Some(value)=value else{if pitch.is_absolute{flush(&mut points,group);}continue;};
        if pitch.is_absolute {
            points.push((time,*value));
            if next>time+1 {points.push((next-1,*value));}
        }else{
            let curve=group.automation_mut(&Parameter::Pitch);curve.mode="linear".into();
            curve.insert(time,*value*100.0);
            if next>time+1 {curve.insert(next-1,*value*100.0);}
        }
    }
    if pitch.is_absolute{flush(&mut points,group);}
    Ok(())
}
