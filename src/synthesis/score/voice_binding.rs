use crate::synthesis::{
    plan::OwnerInput,
    voice::{Result, VoiceError},
};
use serde_json::Value;

/// Validate the selected catalog entry against the effective project selector.
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
    let database =
        crate::model::singer_database(&input.voice.track_default_database, &input.voice.database)
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
    if let Some(selected) = selector(database.get("version"), "version")?
        && selected != "-2"
        && selected != version
    {
        return Err(VoiceError::new(format!(
            "Selected voice version {selected:?} does not match loaded version {version:?}"
        )));
    }
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
