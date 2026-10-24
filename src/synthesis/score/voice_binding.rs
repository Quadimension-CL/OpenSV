use crate::synthesis::{
    plan::OwnerInput,
    voice::{Result, VoiceError},
};
use serde_json::Value;

pub fn validate_voice(
    input: &OwnerInput,
    name: &str,
    version: &str,
    database_id: &str,
) -> Result<()> {
    if name.is_empty() || version.is_empty() {
        return Err(VoiceError::new(
            "Loaded voice configuration has no name or version",
        ));
    }
    let selected = crate::model::effective_singer_database(&input.voice.track_default_database, &input.voice.database);
    let database = selected
            .as_object()
            .ok_or_else(|| VoiceError::new("The selected voice database must be an object"))?;
    if let Some(selected) = selector(database.get("name"), "name")?
        && selected != name
    {
        return Err(VoiceError::new(format!(
            "Selected voice {selected:?} does not match loaded voice {name:?}"
        )));
    }
    if let Some(id) = selector(database.get("id"), "id")?
        && id != database_id
    {
        return Err(VoiceError::new(format!(
            "Selected voice id {id:?} does not match loaded catalog entry {database_id:?}"
        )));
    }

    selector(database.get("version"), "version")?;
    Ok(())
}
fn selector<'a>(value: Option<&'a Value>, field: &str) -> Result<Option<&'a str>> {
    match value {
        None => Ok(None),
        Some(Value::String(value)) if value.is_empty() => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(VoiceError::new(format!(
            "The selected database.{field} must be a string"
        ))),
    }
}
