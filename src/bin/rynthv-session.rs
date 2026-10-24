use rynthv::synthesis::voice::session_identity;
use rynthv::synthesis::settings::{SynthesisSettings, VoiceSelection};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, process::ExitCode};
use zeroize::Zeroizing;

fn main() -> ExitCode {
    match run() {
        Ok(value) => {
            println!("{value}");
            if value["errors"].as_u64().unwrap_or(0) == 0 {ExitCode::SUCCESS} else {ExitCode::FAILURE}
        }
        Err(error) => {eprintln!("{error}"); ExitCode::FAILURE}
    }
}

fn run() -> Result<Value, Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut options = BTreeMap::new();
    let mut saved = false;
    while let Some(key) = args.next() {
        if key == "--saved" { saved = true; continue; }
        let key = key.to_str().ok_or("Invalid option")?.to_owned();
        if !matches!(key.as_str(), "--session" | "--session-key" | "--voice" | "--model-version" | "--context-list" | "--settings" | "--database-id" | "--revision") {
            return Err("Usage: rynthv-session (--session PATH | --saved) (--voice NAME --model-version 1|2 | --context-list CONTEXT.json) [--settings PATH --database-id UUID --revision VERSION]".into());
        }
        let value = args.next().ok_or("Option requires a value")?;
        if options.insert(key, value).is_some() {return Err("Duplicate option".into());}
    }
    let source = options.remove("--session").map(PathBuf::from);
    let manual_key = options.remove("--session-key").map(|value|
        session_identity::parse_session_key(value.to_str().ok_or("Session key must be UTF-8")?).map(Zeroizing::new)
            .map_err(|e|e.to_string())).transpose()?;
    if manual_key.is_some() && source.is_none() { return Err("--session-key requires --session; it is not a DUR override".into()); }
    if saved == source.is_some() {return Err("Select exactly one of --session and --saved".into());}
    let policy = if let Some(path) = options.remove("--settings") {
        if options.contains_key("--context-list") { return Err("Settings inspection requires one exact voice revision".into()); }
        let path = PathBuf::from(path);
        if std::fs::metadata(&path)?.len() > 1024 * 1024 {return Err("Settings exceed 1 MiB".into());}
        let settings: SynthesisSettings = serde_json::from_slice(&std::fs::read(path)?)?;
        let database_id = options.remove("--database-id").ok_or("Missing --database-id")?.into_string().map_err(|_|"Invalid database ID")?;
        let database_id = uuid::Uuid::parse_str(&database_id)?.to_string();
        let revision = options.remove("--revision").ok_or("Missing --revision")?.into_string().map_err(|_|"Invalid revision")?.parse()?;
        Some((settings, VoiceSelection { database_id, revision }))
    } else { None };
    let mut voices = Vec::new();
    if let Some(path) = options.remove("--context-list") {
        let path = PathBuf::from(path);
        if std::fs::metadata(&path)?.len() > 1024 * 1024 {return Err("Context list exceeds 1 MiB".into());}
        let context: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        for entry in context["voices"].as_array().ok_or("Missing voices list")? {
            let name = entry["voiceName"].as_str().ok_or("Missing voice name")?.to_owned();
            let version = entry["modelVersion"].as_u64().and_then(|v|u8::try_from(v).ok()).ok_or("Invalid model generation")?;
            voices.push((name, version));
        }
    } else {
        let name = options.remove("--voice").ok_or("Missing --voice")?.into_string().map_err(|_|"Invalid voice name")?;
        let version = options.remove("--model-version").ok_or("Missing --model-version")?
            .to_str().ok_or("Invalid model generation")?.parse::<u8>()?;
        voices.push((name, version));
    }
    if !options.is_empty() {return Err("Incompatible options".into());}
    let mut rows = Vec::new();
    for (name, version) in voices {
        if !matches!(version, 1 | 2) { return Err("Model generation must be 1 or 2".into()); }
        let overridden = policy.as_ref().and_then(|(settings, selection)| {
            settings.duration_key_override_for(selection).map(|identity| {
                let provenance = if settings.voice_duration_key_override(selection).is_some_and(|entry|entry.enabled) {
                    "voice_revision_override"
                } else { "global_override" };
                json!({"identity":identity,"protection":provenance,"database_id":selection.database_id,"revision":selection.revision})
            })
        });
        let result = if let Some(condition) = overridden { Ok(condition) } else { match &source {
            Some(path) => match &manual_key {
                Some(key) => session_identity::resolve_session_condition_with_key(&name, version, path, **key),
                None => session_identity::resolve_session_condition(&name, version, path),
            }
                .map(|condition| json!({"identity":condition.identity,"protection":condition.protection})),
            None => session_identity::saved_identity(&name, version)
                .map(|identity| json!({"identity":identity,"protection":"saved_context"})),
        }};
        rows.push(match result {
            Ok(condition) => json!({"voiceName":name,"modelVersion":version,"condition":condition}),
            Err(error) => json!({"voiceName":name,"modelVersion":version,"error":error.to_string(),
                "session_decryption_exhausted":error.is_session_decryption_exhausted()}),
        });
    }
    let errors = rows.iter().filter(|row| row.get("error").is_some()).count();
    Ok(json!({"voices":rows,"errors":errors,"read_only":true}))
}
