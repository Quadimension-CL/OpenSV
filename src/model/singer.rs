use serde_json::Value;
use std::borrow::Cow;

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

pub fn effective_singer_database<'a>(track:&'a Value,instance:&'a Value)->Cow<'a,Value> {
    let base=singer_database(track,instance);
    if std::ptr::eq(base,instance) { return Cow::Borrowed(base); }
    let Some(version)=instance.get("version") else{return Cow::Borrowed(base)};
    if version.as_str().is_some_and(|v|v.is_empty()||v=="-2") {return Cow::Borrowed(base)}
    let Some(mut selected)=base.as_object().cloned() else{return Cow::Borrowed(base)};
    selected.insert("version".into(),version.clone());
    Cow::Owned(Value::Object(selected))
}
