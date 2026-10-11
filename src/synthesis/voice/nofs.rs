//! Original SV1 AI database resources. Only public metadata and model payloads
//! are accessed; unrelated registration records have no synthesis role.

mod container;

pub(super) use container::NofsArchive;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use super::{
    Result, VoiceError,
    config::VoiceConfiguration,
    embedded::{self, types},
};

const MAX_METADATA_BYTES: usize = 64 * 1024;
pub(super) const MAX_MODEL_BYTES: usize = 512 * 1024 * 1024;

pub(super) struct Description {
    pub configuration: VoiceConfiguration,
    pub models: Vec<(usize, &'static str)>,
}

pub(super) fn paths(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|path| is_file(path));
    paths.sort();
    Ok(paths)
}

pub(super) fn is_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("nofs"))
}

pub(super) fn database_id(path: &Path) -> Result<String> {
    let resolved = fs::canonicalize(path)?;
    let directory = resolved
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| VoiceError::new("SV1 voice path has no valid database directory"))?;
    Ok(format!("sv1:{directory}"))
}

pub(super) fn describe(archive: &mut NofsArchive) -> Result<Description> {
    let mut metadata = BTreeMap::new();
    let mut metadata_bytes = 0usize;
    let mut models = BTreeMap::new();
    for index in 0..archive.records.len() {
        let key = archive.records[index].key.clone();
        if key.starts_with(b".") || key == b"f0model-configs" {
            let name = std::str::from_utf8(&key)
                .map_err(|_| VoiceError::new("SV1 public metadata key is not UTF-8"))?;
            let value = archive.read(index, MAX_METADATA_BYTES - metadata_bytes)?;
            metadata_bytes += value.len();
            metadata.insert(
                name.to_owned(),
                String::from_utf8(value)
                    .map_err(|_| VoiceError::new("SV1 public metadata value is not UTF-8"))?,
            );
        } else if key == b"f0model-dds" || model_key(&key) {
            let header = archive.prefix(index, 28)?;
            let role = match embedded::root_type(&header)? {
                types::DURATION_TYPE_ID => "duration",
                types::F0_WRAPPER_TYPE_ID => "f0",
                types::TIMBRE_TYPE_ID => "timbre",
                types::VOCODER_TYPE_ID => "vocoder",
                _ => return Err(VoiceError::new("不支持的 SV1 模型类型")),
            };
            if (key == b"f0model-dds") != (role == "f0") || models.insert(role, index).is_some() {
                return Err(VoiceError::new(
                    "SV1 model resources have ambiguous stage roles",
                ));
            }
        }
    }
    let missing = ["duration", "f0", "timbre", "vocoder"]
        .into_iter()
        .filter(|role| !models.contains_key(role))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(VoiceError::new(format!(
            "SV1 声库缺少受支持的本地模型：{}",
            missing.join("、")
        )));
    }
    Ok(Description {
        configuration: VoiceConfiguration::from_nofs(metadata)?,
        models: models
            .into_iter()
            .map(|(role, index)| (index, role))
            .collect(),
    })
}

fn model_key(key: &[u8]) -> bool {
    key.len() == 29
        && key.starts_with(&[0xb3, 0xd8, 0x12, 0x6b, 0x21, 0x18, 0x3f, 0x76])
        && key.ends_with(&[0x06, 0x25, 0x88, 0x49, 0xbd])
}
