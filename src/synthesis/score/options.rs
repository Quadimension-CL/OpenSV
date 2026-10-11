//! Validate synthesis options against the selected voice family's support.
//! Unimplemented options are reported, never silently discarded.

use serde_json::Value;

use crate::{
    model::{Extra, Language, NoteMode, NotePerformance, database_language, pitch::PitchControl},
    synthesis::{
        plan::{NoteInput, OwnerInput},
        voice::{Result, VoiceError, VoiceFamily},
    },
};

pub(super) fn validate_owner(input: &OwnerInput, family: VoiceFamily) -> Result<()> {
    for (name, curve) in &input.automation {
        if !curve.points.is_empty() || !curve.extra.is_empty() {
            return unsupported(format!("automation {name}"));
        }
    }
    for control in &input.pitch_controls {
        if matches!(control, PitchControl::Point(_)) && family != VoiceFamily::Sv2 {
            return Err(VoiceError::new(
                "音高控制点仅支持二代歌声；当前歌声请改用手绘音高曲线。",
            ));
        }
        let extra = match control {
            PitchControl::Curve(curve) if curve.valid() => &curve.extra,
            PitchControl::Point(point) if point.valid() => &point.extra,
            PitchControl::Unsupported(_) => return unsupported("unknown pitch control"),
            _ => return Err(VoiceError::new("Invalid pitch control")),
        };
        if let Some(key) = extra.keys().find(|key| key.as_str() != "scriptData") {
            return unsupported(format!("pitch control option {key}"));
        }
    }
    for (key, value) in &input.group_options {
        if key != "vocalModes" || !empty_object(value) {
            return unsupported(format!("group option {key}"));
        }
    }
    for (label, values) in [
        ("instance", &input.voice.instance_options),
        ("track default", &input.voice.track_default_options),
    ] {
        for (key, value) in values {
            let supported = match key.as_str() {
                "isInstrumental" => value.as_bool() == Some(false),
                "dictionary" => value.is_string(),
                "voicePresetName" => value.as_str() == Some(""),
                "voice" => {
                    validate_voice(value, label)?;
                    true
                }
                "takes" => {
                    super::retakes::validate_instance(value)?;
                    true
                }
                _ => false,
            };
            if !supported {
                return unsupported(format!("{label} option {key}"));
            }
        }
    }
    let dictionary = input
        .voice
        .instance_options
        .get("dictionary")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            input
                .voice
                .track_default_options
                .get("dictionary")
                .and_then(Value::as_str)
        })
        .unwrap_or("");
    if !dictionary.is_empty() && input.voice.dictionary.is_none() {
        return Err(VoiceError::new(format!(
            "工程中找不到发音词典：{dictionary}"
        )));
    }
    if !input.voice.track_options.is_empty() || !input.project_options.is_empty() {
        return unsupported("additional track/project synthesis options");
    }
    if !input.tempo.extensions.is_empty() || !input.tempo.event_extensions.is_empty() {
        return unsupported("additional tempo mapping options");
    }
    Ok(())
}

pub(super) fn validate_note(note: &NoteInput, family: VoiceFamily) -> Result<()> {
    validate_performance(
        NotePerformance::from_parts(&note.musical_type, &note.attributes)
            .map_err(VoiceError::new)?,
        family,
    )?;
    for (key, value) in &note.attributes {
        let supported = match key.as_str() {
            "muted" => value.is_boolean(),
            "evenSyllableDuration" => value.is_boolean(),
            "languageOverride" => value
                .as_str()
                .is_some_and(|id| id.is_empty() || Language::parse(id).is_some()),
            // The alphabet is checked after resolving the note's language.
            "phonesetOverride" => value.is_string(),
            // The typed performance parser above validates these values.
            "expValueX" | "expValueY" | "dF0VbrMod" | "rTone" | "rIntonation" => true,
            _ => false,
        };
        if !supported {
            return unsupported(format!("note {} attribute {key}", note.id));
        }
    }
    for (key, value) in &note.extensions {
        let supported = match key.as_str() {
            "accent" => matches!(value.as_str(), Some("" | "1" | "2" | "3" | "4" | "5")),
            "detune" => value.as_f64() == Some(0.0),
            "takes" => {
                super::retakes::validate(value, family)?;
                true
            }
            _ => false,
        };
        if !supported {
            return unsupported(format!("note {} option {key}", note.id));
        }
    }
    Ok(())
}

pub(super) fn validate_performance(value: NotePerformance, family: VoiceFamily) -> Result<()> {
    value.validate().map_err(VoiceError::new)?;
    if family == VoiceFamily::Sv1 && (value.expression_x != 0.0 || value.expression_y != 0.0) {
        return Err(VoiceError::new("原生一代歌声暂不支持二维演绎参数。"));
    }
    if family == VoiceFamily::Sv1 && value.mode == NoteMode::Rap {
        return Err(VoiceError::new("原生一代歌声暂不支持说唱合成。"));
    }
    Ok(())
}

pub(super) fn language(input: &OwnerInput, default: Language) -> Result<Language> {
    let mut selected = default;
    for database in [&input.voice.track_default_database, &input.voice.database] {
        selected = database_language(database, selected).map_err(VoiceError::new)?;
        validate_phone_set(database.get("phonesetOverride"), selected)?;
    }
    Ok(selected)
}

pub(super) fn note_language(note: &NoteInput, inherited: Language) -> Result<Language> {
    let language = match note.attributes.get("languageOverride") {
        None => inherited,
        Some(Value::String(id)) if id.is_empty() => inherited,
        Some(Value::String(id)) => Language::parse(id)
            .ok_or_else(|| VoiceError::new(format!("音符 {} 的语言无效：{id}", note.id)))?,
        Some(_) => return unsupported(format!("note {} non-string language override", note.id)),
    };
    validate_phone_set(note.attributes.get("phonesetOverride"), language)?;
    Ok(language)
}

fn validate_phone_set(value: Option<&Value>, language: Language) -> Result<()> {
    match value {
        None => Ok(()),
        Some(Value::String(id)) if id.is_empty() || id == language.phoneme_alphabet() => Ok(()),
        Some(value) => Err(VoiceError::new(format!(
            "{}的音素集应为 {}，当前为 {value}",
            language.label(),
            language.phoneme_alphabet()
        ))),
    }
}

pub(super) fn muted(attributes: &Extra) -> Result<bool> {
    match attributes.get("muted") {
        None => Ok(false),
        Some(Value::Bool(muted)) => Ok(*muted),
        _ => unsupported("non-boolean note mute"),
    }
}

fn validate_voice(value: &Value, scope: &str) -> Result<()> {
    let Some(object) = value.as_object() else {
        return unsupported(format!("{scope} non-object voice"));
    };
    for (key, value) in object {
        let supported = match key.as_str() {
            // Explicit zero values are the neutral settings already consumed
            // by the pipeline; nonzero controls still require DSP support.
            "paramLoudness" | "paramTension" | "paramBreathiness" | "paramGender"
            | "paramToneShift" | "transposeCents" | "transposeSemitones" => {
                value.as_f64() == Some(0.0)
            }
            "vocalModeInherited" => value.is_boolean(),
            "vocalModePreset" => value.as_str() == Some(""),
            "vocalModeParams" => empty_object(value),
            _ => false,
        };
        if !supported {
            return unsupported(format!("{scope} voice.{key} = {value}"));
        }
    }
    Ok(())
}

fn empty_object(value: &Value) -> bool {
    value.as_object().is_some_and(|object| object.is_empty())
}

fn unsupported<T>(feature: impl std::fmt::Display) -> Result<T> {
    Err(VoiceError::new(format!(
        "Synthesis does not yet support {feature}"
    )))
}
