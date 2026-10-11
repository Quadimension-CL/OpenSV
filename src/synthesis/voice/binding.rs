use super::{VoiceAvailability, VoiceCatalog, VoiceCatalogEntry};
use crate::{model::singer_database, synthesis::settings::VoiceSelection};
use serde_json::Value;

impl VoiceCatalogEntry {
    pub fn selection(&self) -> VoiceSelection {
        VoiceSelection {
            database_id: self.database_id.clone(),
            revision: self.revision,
        }
    }
}

impl VoiceCatalog {
    pub fn entry(&self, selection: &VoiceSelection) -> Option<&VoiceCatalogEntry> {
        self.entries
            .iter()
            .find(|e| e.database_id == selection.database_id && e.revision == selection.revision)
    }

    pub fn resolve(
        &self,
        track: &Value,
        instance: &Value,
    ) -> Result<Option<VoiceSelection>, String> {
        let database = singer_database(track, instance)
            .as_object()
            .ok_or("歌声 database 必须是对象")?;
        let text = |key| -> Result<&str, String> {
            match database.get(key) {
                None => Ok(""),
                Some(Value::String(s)) => Ok(s),
                _ => Err(format!("歌声 database.{key} 必须是字符串")),
            }
        };
        let id = text("id")?;
        let name = text("name")?;
        if id.is_empty() && name.is_empty() {
            return Ok(None);
        }
        let version = text("version")?;
        let revision = if version.is_empty() || version == "-2" {
            None
        } else {
            Some(
                version
                    .parse::<u32>()
                    .map_err(|_| format!("歌声版本无效：{version}"))?,
            )
        };
        let matches: Vec<_> = self
            .entries
            .iter()
            .filter(|e| {
                (id.is_empty() || e.database_id == id)
                    && (name.is_empty() || e.display_name == name)
                    && revision.is_none_or(|r| e.revision == r)
            })
            .collect();
        if id.is_empty()
            && matches.iter().any(|e| {
                matches
                    .first()
                    .is_some_and(|first| first.database_id != e.database_id)
            })
        {
            return Err(format!("存在多个同名歌声 {name}，请为音轨重新选择歌手"));
        }
        let entry = matches
            .into_iter()
            .max_by_key(|e| (e.availability.is_loadable(), e.revision))
            .ok_or_else(|| {
                format!(
                    "找不到歌声 {}（版本 {}），请检查 databases 目录",
                    if name.is_empty() { id } else { name },
                    if version.is_empty() {
                        "默认"
                    } else {
                        version
                    }
                )
            })?;
        if let VoiceAvailability::Unsupported(reason) = &entry.availability {
            return Err(format!(
                "歌声 {}（版本 {}）不可用：{reason}",
                entry.display_name, entry.revision,
            ));
        }
        Ok(Some(entry.selection()))
    }
}
