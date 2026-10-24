use super::{VoiceAvailability, VoiceCatalog, VoiceCatalogEntry, VoiceRevision};
use crate::{model::effective_singer_database, synthesis::settings::VoiceSelection};
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
    pub fn is_bank_installed(&self,track:&Value,instance:&Value)->Option<bool> {
        let selected=effective_singer_database(track,instance);
        let id=selected.get("id").and_then(Value::as_str).unwrap_or("");
        let name=selected.get("name").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() && name.is_empty(){return None}
        Some(self.entries.iter().any(|e|(id.is_empty()||e.database_id==id)&&(name.is_empty()||e.display_name==name)))
    }
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
        let selected = effective_singer_database(track, instance);
        let database = selected
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
                    .parse::<VoiceRevision>()
                    .map_err(|_| format!("歌声版本无效：{version}"))?,
            )
        };
        let matches: Vec<_> = self
            .entries
            .iter()
            .filter(|e| {
                (id.is_empty() || e.database_id == id)
                    && (name.is_empty() || e.display_name == name)
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

        let entry = matches.iter().copied().filter(|e|Some(e.revision)==revision)
            .max_by_key(|e|e.availability.is_loadable())
            .or_else(||matches.into_iter().max_by_key(|e|(e.revision,e.availability.is_loadable())))
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
