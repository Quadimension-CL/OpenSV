//! Explicit model-context imports, shared by Windows and Linux CLI entry points.

use rynthv::synthesis::voice::{VoiceInventory, session_identity};
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

pub enum ContextImport {
    File(PathBuf),
    Identity(u64),
    Process(u32),
}

impl ContextImport {
    pub fn parse(values: &mut BTreeMap<String, OsString>) -> Result<Self, String> {
        let file = values.remove("--context-file");
        let identity = values.remove("--identity");
        let process = values.remove("--process-id");
        match (file, identity, process) {
            (Some(path), None, None) => {
                if path.is_empty() {
                    return Err("--context-file requires a nonempty path".into());
                }
                Ok(Self::File(PathBuf::from(path)))
            }
            (None, Some(value), None) => {
                let text = value.to_str().ok_or("--identity must be UTF-8")?;
                session_identity::parse_identity(text)
                    .map(Self::Identity)
                    .map_err(|error| error.to_string())
            }
            (None, None, Some(value)) => {
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
            _ => Err(
                "import-context requires exactly one of --context-file, --identity, or --process-id"
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
