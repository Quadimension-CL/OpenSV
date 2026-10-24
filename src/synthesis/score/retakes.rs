use crate::{
    model::{RetakeSeeds, TakeSet},
    synthesis::{
        plan::NoteInput,
        voice::{Result, VoiceError, VoiceFamily},
    },
};
use serde_json::Value;

pub(super) fn neutral_legacy(value:&Value)->bool {
    let Some(object)=value.as_object() else{return false};
    object.keys().all(|key|matches!(key.as_str(),"activeTakeId"|"takes"))
        && object.get("activeTakeId").and_then(Value::as_u64)==Some(0)
        && object.get("takes").and_then(Value::as_array).is_some_and(|takes|takes.len()==1
            && takes[0].as_object().is_some_and(|take|take.keys().all(|key|matches!(key.as_str(),"id"|"expr"|"liked"))
                && take.get("id").and_then(Value::as_u64)==Some(0)
                && take.get("expr").and_then(Value::as_f64)==Some(0.0)
                && take.get("liked").and_then(Value::as_bool)==Some(false)))
}

pub(super) fn validate(value: &Value, family: VoiceFamily) -> Result<()> {
    let takes = TakeSet::from_value(value).map_err(VoiceError::new)?;
    validate_seeds(takes.active().seeds(), family)
}

pub(super) fn validate_instance(value: &Value) -> Result<()> {
    let takes = TakeSet::from_value(value).map_err(VoiceError::new)?;
    if takes.active().seeds() != RetakeSeeds::default() {
        return Err(VoiceError::new(
            "暂不支持音符组实例的重录采样，请使用音符重录。",
        ));
    }
    Ok(())
}

pub(super) fn validate_seeds(seeds: RetakeSeeds, family: VoiceFamily) -> Result<()> {
    if family != VoiceFamily::Sv2 && seeds.duration != 0 {
        return Err(VoiceError::new(
            "时值重录仅支持二代歌声；当前歌声可使用音高和音色重录。",
        ));
    }
    Ok(())
}

pub(super) fn note_seeds(note: &NoteInput, family: VoiceFamily) -> Result<RetakeSeeds> {
    let seeds = TakeSet::from_extra(&note.extensions)
        .map_err(VoiceError::new)?
        .active()
        .seeds();
    validate_seeds(seeds, family)?;
    Ok(seeds)
}
