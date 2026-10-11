use super::Editor;
use crate::model::Language;

impl Editor {
    pub fn set_track_voice(&mut self, singer: Option<(&str, &str, u32)>) -> Result<(), String> {
        if self.current_track().is_instrumental() {
            return Err("伴奏轨不能设置歌手".into());
        }
        if self
            .current_track()
            .references()
            .any(|r| !r.database.is_object())
        {
            return Err("音轨的歌声 database 字段必须是对象".into());
        }
        self.begin("切换音轨歌手");
        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|t| t.id == self.track)
            .expect("selected track exists");
        for reference in std::iter::once(&mut track.main_ref).chain(&mut track.groups) {
            let database = reference
                .database
                .as_object_mut()
                .expect("validated database");
            database.remove("id");
            let (name, version) = match singer {
                Some((id, name, version)) => {
                    database.insert("id".into(), id.into());
                    (name, version.to_string())
                }
                None => ("", "-2".to_owned()),
            };
            database.insert("name".into(), name.into());
            database.insert("version".into(), version.into());
            // These are bank defaults; explicit user language overrides stay intact.
            for key in ["language", "phoneset", "backendType"] {
                database.insert(key.into(), "".into());
            }
        }
        self.finish();
        Ok(())
    }

    /// With a group selected, edit only that instance. Otherwise edit the track
    /// defaults, which are inherited by subsequently created groups.
    pub fn set_voice_language(&mut self, language: Option<Language>) -> Result<(), String> {
        let target = self.active_ref().unwrap_or(&self.current_track().main_ref);
        let target_id = target.id;
        let database = target
            .database
            .as_object()
            .ok_or("歌声的 database 字段必须是对象")?;
        let value = language.map_or("", Language::id);
        let alphabet = language.map_or("", Language::phoneme_alphabet);
        if database
            .get("languageOverride")
            .and_then(serde_json::Value::as_str)
            == Some(value)
            && database
                .get("phonesetOverride")
                .and_then(serde_json::Value::as_str)
                == Some(alphabet)
        {
            return Ok(());
        }
        self.begin("切换歌声语言");
        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == self.track)
            .expect("selected track exists");
        let reference = std::iter::once(&mut track.main_ref)
            .chain(&mut track.groups)
            .find(|reference| reference.id == target_id)
            .expect("selected reference exists");
        let database = reference
            .database
            .as_object_mut()
            .expect("validated database");
        database.insert("languageOverride".into(), value.into());
        database.insert("phonesetOverride".into(), alphabet.into());
        self.finish();
        Ok(())
    }
}
