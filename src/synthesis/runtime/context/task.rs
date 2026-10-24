use super::{ContextEvent, ContextImportPhase, ContextImportSource};
use crate::synthesis::{
    runtime::{Runtime, events::WakeOnExit},
    voice::session_identity,
};
use std::{
    collections::BTreeSet,
    sync::mpsc::{self, TryRecvError},
    thread,
};
use zeroize::Zeroizing;

impl Runtime {
    pub fn import_context(&mut self) {
        let Some(dialog) = &mut self.context_import.dialog else {
            return;
        };
        if !dialog.can_import() {
            return;
        }
        let source = dialog.source;
        let session_key = if source == ContextImportSource::SessionFile && dialog.session_key_required {
            match session_identity::parse_session_key(&dialog.session_key) {
                Ok(key) => Some(Zeroizing::new(key)),
                Err(error) => { dialog.error = Some(error.to_string()); return; }
            }
        } else { None };
        use zeroize::Zeroize;
        dialog.session_key.zeroize();
        let session_path = dialog.session_path.clone();
        let path = dialog.file_path.clone();
        let identity = Zeroizing::new(dialog.manual_identity.clone());
        let process_id = dialog.selected_process;
        let voice_name = dialog.voice_name.clone();
        let model_version = dialog.model_version;
        dialog.error = None;
        dialog.phase = ContextImportPhase::Importing;
        self.context_import.feedback = None;
        self.start_context_task("model-context-import", move || {
            let result = match source {
                ContextImportSource::SessionFile => {
                    let path = session_path.expect("validated session path");
                    match session_key {
                        Some(key) => session_identity::import_session_context_with_key(&voice_name, model_version, &path, *key),
                        None => session_identity::import_session_context(&voice_name, model_version, &path),
                    }
                },
                ContextImportSource::File => session_identity::import_context_file(
                    &voice_name,
                    model_version,
                    &path.expect("validated context path"),
                ),
                ContextImportSource::Manual => session_identity::parse_identity(&identity)
                    .and_then(|identity| {
                        session_identity::save_manual_identity(&voice_name, model_version, identity)
                    }),
                ContextImportSource::Online => session_identity::import_online_context(
                    &voice_name,
                    model_version,
                    process_id.expect("validated context process"),
                ),
            };
            ContextEvent::Imported(result)
        });
    }

    pub(super) fn start_context_task(
        &mut self,
        name: &str,
        work: impl FnOnce() -> ContextEvent + Send + 'static,
    ) {
        self.context_import.task = None;
        let (tx, rx) = mpsc::sync_channel(1);
        let repaint = self.repaint.clone();
        match thread::Builder::new().name(name.into()).spawn(move || {
            let _wake = WakeOnExit(repaint);
            let _ = tx.send(work());
        }) {
            Ok(_) => self.context_import.task = Some(rx),
            Err(error) => self.context_import_failed(format!("无法启动模型上下文任务：{error}")),
        }
    }

    fn context_import_failed(&mut self, error: String) {
        self.context_import.task = None;
        if let Some(dialog) = &mut self.context_import.dialog {
            dialog.phase = ContextImportPhase::Ready;
            dialog.error = Some(error.clone());
            self.context_import.feedback = Some(Err(format!("{}：{error}", dialog.voice_name)));
        }
    }

    pub(in crate::synthesis::runtime) fn poll_context_import(&mut self) {
        let Some(task) = &self.context_import.task else {
            return;
        };
        let event = match task.try_recv() {
            Ok(event) => event,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.context_import_failed("模型上下文任务意外终止。".into());
                return;
            }
        };
        self.context_import.task = None;
        let Some(dialog) = &mut self.context_import.dialog else {
            return;
        };
        match event {
            ContextEvent::Sources(Ok(sources)) => {
                dialog.selected_process = if sources.len() == 1 {
                    Some(sources[0].process_id)
                } else {
                    None
                };
                dialog.sources = sources;
                dialog.phase = ContextImportPhase::Ready;
            }
            ContextEvent::Sources(Err(error)) => {
                self.context_import_failed(error);
            }
            ContextEvent::Imported(Err(error)) => {
                let retry = error.is_session_decryption_exhausted();
                self.context_import_failed(error.to_string());
                if let Some(dialog) = &mut self.context_import.dialog {
                    dialog.session_key_required = retry && dialog.source == ContextImportSource::SessionFile;
                }
            }
            ContextEvent::Imported(Ok(_)) => {
                let voice_name = dialog.voice_name.clone();
                let model_version = dialog.model_version;
                let selection = dialog.selection.clone();
                self.context_import.dialog = None;

                let mut reload: BTreeSet<_> = self
                    .voices
                    .keys()
                    .filter(|key| {
                        self.library.entry(key).is_some_and(|entry| {
                            entry.display_name == voice_name
                                && entry.model_version == Some(model_version)
                                && entry
                                    .family
                                    .is_some_and(|family| family.requires_identity())
                        })
                    })
                    .cloned()
                    .collect();
                reload.insert(selection);
                for selection in reload {
                    self.reload_voice(&selection);
                }
                self.aggregate_voices();
                self.context_import.feedback =
                    Some(Ok(format!("已导入 {voice_name} 的模型上下文。")));
            }
        }
    }
}
