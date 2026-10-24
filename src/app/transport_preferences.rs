use super::App;
use crate::editor::LoopRegion;
use crate::model::Project;

impl App {
    pub(super) fn restore_transport_choices(&mut self) {
        let extra=&self.editor.project.extra;
        self.editor.transport.return_on_pause=self.preferences.return_on_pause;
        self.editor.transport.looping=extra.get("loopEnabled").and_then(serde_json::Value::as_bool)
            .unwrap_or(self.preferences.looping);
        if let Some(region)=extra.get("loopBegin").and_then(serde_json::Value::as_i64)
            .zip(extra.get("loopEnd").and_then(serde_json::Value::as_i64))
            .and_then(|(begin,end)|LoopRegion::from_saved(begin,end)) {
            self.editor.transport.loop_region=region;
        }
    }
    pub(super) fn project_for_save(&self)->Project {
        let mut project=self.editor.project.clone();
        project.extra.insert("loopEnabled".into(),self.editor.transport.looping.into());
        project.extra.insert("loopBegin".into(),self.editor.transport.loop_region.start().into());
        project.extra.insert("loopEnd".into(),self.editor.transport.loop_region.end().into());
        project
    }
}
