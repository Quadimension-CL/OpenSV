mod task;

use super::Runtime;
use crate::synthesis::{
    settings::VoiceSelection,
    voice::session_identity::{self, OnlineContextSource},
};
use std::{path::PathBuf, sync::mpsc::Receiver};
use zeroize::Zeroize;

#[derive(Default)]
pub struct ContextImport {
    pub dialog: Option<ContextImportDialog>,
    pub feedback: Option<Result<String, String>>,
    task: Option<Receiver<ContextEvent>>,
}

pub struct ContextImportDialog {
    pub selection: VoiceSelection,
    pub voice_name: String,
    pub model_version: u8,
    pub source: ContextImportSource,
    pub session_path: Option<PathBuf>,
    pub file_path: Option<PathBuf>,
    pub manual_identity: String,
    pub sources: Vec<OnlineContextSource>,
    pub selected_process: Option<u32>,
    pub phase: ContextImportPhase,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ContextImportSource {
    SessionFile,
    File,
    Manual,
    Online,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ContextImportPhase {
    Discovering,
    Ready,
    Importing,
}

enum ContextEvent {
    Sources(Result<Vec<OnlineContextSource>, String>),
    Imported(Result<PathBuf, String>),
}

impl Drop for ContextImportDialog {
    fn drop(&mut self) {
        self.manual_identity.zeroize();
    }
}

impl ContextImportDialog {
    pub fn can_import(&self) -> bool {
        self.phase == ContextImportPhase::Ready
            && match self.source {
                ContextImportSource::SessionFile => {
                    session_identity::supports_session_import() && self.session_path.is_some()
                }
                ContextImportSource::File => self.file_path.is_some(),
                ContextImportSource::Manual => !self.manual_identity.trim().is_empty(),
                ContextImportSource::Online => self
                    .selected_process
                    .is_some_and(|id| self.sources.iter().any(|source| source.process_id == id)),
            }
    }
}

impl Runtime {
    pub fn can_import_context(&self, selection: &VoiceSelection) -> bool {
        !self.library.is_scanning()
            && self.library.settings_error().is_none()
            && !self
                .context_import
                .dialog
                .as_ref()
                .is_some_and(|dialog| dialog.phase == ContextImportPhase::Importing)
            && self.library.entry(selection).is_some_and(|entry| {
                entry.availability.is_loadable()
                    && !entry.display_name.trim().is_empty()
                    && entry.family.is_some_and(|family| {
                        family.requires_identity()
                            && entry.model_version == Some(family.model_version())
                    })
            })
    }

    pub fn needs_context_import(&self, selection: &VoiceSelection) -> bool {
        self.library
            .entry(selection)
            .and_then(|entry| entry.family)
            .is_some_and(|family| family.requires_identity())
            && self
                .voices
                .get(selection)
                .is_some_and(|voice| voice.context_required)
    }

    /// Keep the failing singer as the import target even if the track changes.
    pub fn context_import_targets(&self) -> Vec<(VoiceSelection, String)> {
        self.voices
            .iter()
            .filter_map(|(selection, voice)| {
                if !voice.context_required || !self.needs_context_import(selection) {
                    return None;
                }
                let name = self
                    .inventory(selection)?
                    .configuration
                    .as_ref()?
                    .name
                    .clone();
                Some((selection.clone(), name))
            })
            .collect()
    }

    /// Opening the dialog never reads another application's session or processes.
    pub fn open_context_import(&mut self, selection: VoiceSelection) {
        if !self.can_import_context(&selection) {
            return;
        }
        let Some(entry) = self.library.entry(&selection) else {
            return;
        };
        let Some(model_version) = entry.model_version else {
            return;
        };
        self.context_import = ContextImport {
            dialog: Some(ContextImportDialog {
                voice_name: entry.display_name.clone(),
                model_version,
                selection,
                source: if session_identity::supports_session_import() {
                    ContextImportSource::SessionFile
                } else {
                    ContextImportSource::File
                },
                session_path: None,
                file_path: None,
                manual_identity: String::new(),
                sources: Vec::new(),
                selected_process: None,
                phase: ContextImportPhase::Ready,
                error: None,
            }),
            ..ContextImport::default()
        };
    }

    pub fn set_context_import_source(&mut self, source: ContextImportSource) {
        let Some(dialog) = &mut self.context_import.dialog else {
            return;
        };
        if dialog.phase == ContextImportPhase::Importing
            || source == dialog.source
            || (source == ContextImportSource::SessionFile
                && !session_identity::supports_session_import())
            || (source == ContextImportSource::Online
                && !session_identity::supports_online_import())
        {
            return;
        }
        // Dropping the receiver detaches the previous discovery, so its result
        // can never overwrite the fields of a newly selected source.
        self.context_import.task = None;
        dialog.source = source;
        dialog.phase = ContextImportPhase::Ready;
        self.clear_context_import_error();
        if source == ContextImportSource::Online {
            self.refresh_context_sources();
        }
    }

    pub fn clear_context_import_error(&mut self) {
        if let Some(dialog) = &mut self.context_import.dialog {
            dialog.error = None;
        }
        self.context_import.feedback = None;
    }

    pub fn refresh_context_sources(&mut self) {
        let Some(dialog) = &mut self.context_import.dialog else {
            return;
        };
        if dialog.phase == ContextImportPhase::Importing
            || dialog.source != ContextImportSource::Online
            || !session_identity::supports_online_import()
        {
            return;
        }
        dialog.sources.clear();
        dialog.selected_process = None;
        dialog.error = None;
        dialog.phase = ContextImportPhase::Discovering;
        self.context_import.feedback = None;
        self.start_context_task("model-context-sources", || {
            ContextEvent::Sources(
                session_identity::online_context_sources().map_err(|error| error.to_string()),
            )
        });
    }

    pub fn cancel_context_import(&mut self) {
        if self
            .context_import
            .dialog
            .as_ref()
            .is_some_and(|dialog| dialog.phase != ContextImportPhase::Importing)
        {
            self.context_import.dialog = None;
            self.context_import.task = None;
        }
    }
}
