use rynthv::synthesis::voice::{VoiceInventory, session_identity};
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};
use zeroize::Zeroizing;

pub enum ContextImport {
    File(PathBuf),
    Identity(u64),
    Process(u32),
    SessionFile { path: PathBuf, key: Option<Zeroizing<u64>> },
}

impl ContextImport {
    pub fn parse(values: &mut BTreeMap<String, OsString>) -> Result<Self, String> {
        let file = values.remove("--context-file");
        let identity = values.remove("--identity");
        let process = values.remove("--process-id");
        let session_file = values.remove("--session-file");
        let key = values.remove("--session-key");
        if key.is_some() && session_file.is_none() {return Err("--session-key requires --session-file; it is not a duration override".into());}
        match (file, identity, process, session_file) {
            (Some(path), None, None, None) => {
                if path.is_empty() {
                    return Err("--context-file requires a nonempty path".into());
                }
                Ok(Self::File(PathBuf::from(path)))
            }
            (None, Some(value), None, None) => {
                let text = value.to_str().ok_or("--identity must be UTF-8")?;
                session_identity::parse_identity(text)
                    .map(Self::Identity)
                    .map_err(|error| error.to_string())
            }
            (None, None, Some(value), None) => {
                if !session_identity::supports_online_import() {
                    return Err(
                        "--process-id is available only on Windows x64; use --context-file or --identity"
                            .into(),
                    );
                }
                let process_id = value
                    .to_str()
                    .and_then(|text| text.parse::<u32>().ok())
                    .filter(|&pid| pid != 0)
                    .ok_or("--process-id requires a positive process ID")?;
                Ok(Self::Process(process_id))
            }
            (None, None, None, Some(path)) => {
                if path.is_empty() {
                    return Err("--session-file requires a nonempty path".into());
                }
                if !session_identity::supports_session_import() {
                    return Err(
                        "--session-file is available only on Windows x64; use --context-file or --identity"
                            .into(),
                    );
                }
                let key = key.map(|value| session_identity::parse_session_key(value.to_str().ok_or("--session-key must be UTF-8")?)
                    .map(Zeroizing::new).map_err(|e|e.to_string())).transpose()?;
                Ok(Self::SessionFile {path: PathBuf::from(path), key})
            }
            _ => Err(
                "import-context requires exactly one of --context-file, --identity, --process-id, or --session-file"
                    .into(),
            ),
        }
    }

    pub fn run(self, inventory: &VoiceInventory) -> Result<Value, String> {
        let configuration = inventory
            .configuration
            .as_ref()
            .ok_or("Voice configuration is missing")?;
        if !configuration.family.requires_identity() {
            return Err("原生 SV1 声库不需要导入模型上下文。".into());
        }
        let name = &configuration.name;
        let version = configuration.model_version;
        let path = match self {
            Self::File(path) => session_identity::import_context_file(name, version, &path),
            Self::Identity(identity) => {
                session_identity::save_manual_identity(name, version, identity)
            }
            Self::Process(process_id) => {
                session_identity::import_online_context(name, version, process_id)
            }
            Self::SessionFile { path, key } => {
                match key {
                    Some(key) => session_identity::import_session_context_with_key(name, version, &path, *key),
                    None => session_identity::import_session_context(name, version, &path),
                }
            }
        }
        .map_err(|error| error.to_string())?;
        Ok(json!({
            "voice_name": name,
            "model_version": version,
            "context_path": path,
            "imported": true,
        }))
    }
}
