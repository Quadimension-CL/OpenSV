use serde_json::Value;

/// A group with its own singer overrides the track; an unbound group inherits.
pub fn singer_database<'a>(track: &'a Value, instance: &'a Value) -> &'a Value {
    if ["id", "name"].iter().any(|key| {
        instance
            .get(key)
            .is_some_and(|value| value.as_str().is_none_or(|text| !text.is_empty()))
    }) {
        instance
    } else {
        track
    }
}
