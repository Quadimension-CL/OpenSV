//! Portable resource locations and the selected voice relative to its library.

use super::voice::VoiceCatalog;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceSelection {
    pub database_id: String,
    pub revision: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SynthesisSettings {
    pub databases: Option<PathBuf>,
    pub sv1_databases: Option<PathBuf>,
    pub dictionaries: Option<PathBuf>,
    pub selected_voice: Option<VoiceSelection>,
}

impl SynthesisSettings {
    pub fn database_roots(&self) -> impl Iterator<Item = (&'static str, &Path)> {
        [
            ("SV2", self.databases.as_deref()),
            ("SV1", self.sv1_databases.as_deref()),
        ]
        .into_iter()
        .filter_map(|(label, path)| path.map(|path| (label, path)))
    }

    pub fn has_databases(&self) -> bool {
        self.databases.is_some() || self.sv1_databases.is_some()
    }

    pub fn path() -> Result<PathBuf, String> {
        crate::platform::config_directory().map(|path| path.join("synthesis.json"))
    }

    pub fn load() -> Result<Self, String> {
        let path = Self::path()?;
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| format!("无法读取合成设置 {}：{error}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("无法读取合成设置 {}：{error}", path.display())),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path()?;
        // A malformed existing file needs explicit repair, not replacement by
        // defaults from a startup that could not deserialize it.
        Self::load()?;
        let parent = path.parent().expect("settings path has a parent");
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建合成设置目录 {}：{error}", parent.display()))?;
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("无法序列化合成设置：{error}"))?;
        let temporary = parent.join(format!("synthesis-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> std::io::Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|error| format!("无法保存合成设置 {}：{error}", path.display()))
    }

    pub fn selected_directory(&self) -> Result<PathBuf, String> {
        if !self.has_databases() {
            return Err("尚未设置声库目录。".to_owned());
        }
        let selected = self
            .selected_voice
            .as_ref()
            .ok_or_else(|| "尚未选择声库。".to_owned())?;
        let mut directories = Vec::new();
        let mut errors = Vec::new();
        for (label, root) in self.database_roots() {
            match VoiceCatalog::scan(root) {
                Ok(catalog) => {
                    directories.extend(catalog.entries.into_iter().filter_map(|entry| {
                        (entry.database_id == selected.database_id
                            && entry.revision == selected.revision)
                            .then_some(entry.directory)
                    }))
                }
                Err(error) => errors.push(format!("{label} {}：{error}", root.display())),
            }
        }
        directories.sort();
        directories.dedup();
        match directories.len() {
            1 => Ok(directories.pop().expect("one selected directory")),
            0 => {
                let mut message = format!(
                    "声库 {}/{} 不在已配置的声库目录中。",
                    selected.database_id, selected.revision
                );
                if !errors.is_empty() {
                    message.push('\n');
                    message.push_str(&errors.join("\n"));
                }
                Err(message)
            }
            _ => Err(format!(
                "声库 {}/{} 对应多个文件，请检查声库目录配置。",
                selected.database_id, selected.revision
            )),
        }
    }
}
