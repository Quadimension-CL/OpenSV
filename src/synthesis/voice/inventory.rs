mod inspect;
mod nofs;
mod styles;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{
    Result, VoiceError,
    archive::Archive,
    config::{CONFIGURATION_ID, VoiceConfiguration},
    format::FormatContext,
    preparation::{PreparedFile, PreparedModel, PreparedVoice},
    prepared::PreparedTree,
    tree::ModuleTree,
};

#[derive(Serialize)]
pub struct VoiceInventory {
    pub database_id: String,
    pub directory: PathBuf,
    pub display_name: String,
    pub content_hash: String,
    pub resource_count: usize,
    pub graph_count: usize,
    pub tensor_count: usize,
    pub files: Vec<VoiceFile>,
    pub configuration: Option<VoiceConfiguration>,
    #[serde(skip)]
    _models: Vec<LoadedModel>,
    #[serde(skip)]
    preparation: OnceLock<Result<Arc<PreparedVoice>>>,
}

enum LoadedModel {
    Dnni {
        archive: Arc<Archive>,
        trees: Vec<LoadedTree>,
    },
    Nofs {
        trees: Vec<Vec<u8>>,
    },
}

enum LoadedTree {
    Indexed(ModuleTree),
    Embedded { resource_index: usize },
}

#[derive(Serialize)]
pub struct VoiceFile {
    pub name: String,
    pub bytes: usize,
    pub sha256: String,
    pub sha256_scope: &'static str,
    pub resources: usize,
    pub borrowed_resources: usize,
    pub encoded_resources: usize,
    pub header_signature_equal_branch_verified: bool,
    pub index_digest_verified: bool,
    pub all_resource_digests_verified: bool,
    pub graphs: Vec<GraphSummary>,
    pub referenced_modules: usize,
    pub unreferenced_resources: usize,
    pub tensors: BTreeMap<&'static str, TensorSummary>,
    pub configuration_parsed: bool,
}

#[derive(Serialize)]
pub struct GraphSummary {
    pub resource_index: usize,
    pub nodes: usize,
    pub root_children: usize,
    pub maximum_depth: usize,
    pub role: &'static str,
}

#[derive(Default, Serialize)]
pub struct TensorSummary {
    pub resources: usize,
    pub borrowed_resources: usize,
    pub decoded_resources: usize,
    pub elements: usize,
    pub scales: usize,
}

impl VoiceInventory {
    pub fn prepare(&self) -> Result<Arc<PreparedVoice>> {
        self.preparation
            .get_or_init(|| {
                let files = self
                    ._models
                    .iter()
                    .zip(&self.files)
                    .map(|(model, metadata)| {
                        let trees = metadata
                            .graphs
                            .iter()
                            .enumerate()
                            .map(|(index, graph)| {
                                let legacy_integer=self.configuration.as_ref().is_some_and(|c|
                                    c.family==super::VoiceFamily::Sv2Compatible && matches!(graph.role,"f0"|"timbre"));
                                let tree = match model {
                                    LoadedModel::Dnni { archive, trees } => match &trees[index] {
                                        LoadedTree::Indexed(tree) => {
                                            PreparedTree::prepare_execution(Arc::clone(archive), tree,legacy_integer)?
                                        }
                                        LoadedTree::Embedded { resource_index } => {
                                            PreparedTree::prepare_embedded_execution(
                                                &archive.read(*resource_index, None)?,
                                                legacy_integer,
                                            )?
                                        }
                                    },
                                    LoadedModel::Nofs { trees } => {
                                        PreparedTree::prepare_embedded(&trees[index])?
                                    }
                                };
                                Ok(PreparedModel {
                                    role: graph.role,
                                    tree: Arc::new(tree),
                                })
                            })
                            .collect::<Result<Vec<_>>>()?;
                        Ok(PreparedFile {
                            name: metadata.name.clone(),
                            trees,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let configuration = self
                    .configuration
                    .as_ref()
                    .ok_or_else(|| VoiceError::new("Voice configuration is missing"))?;
                Ok(Arc::new(PreparedVoice::new(
                    files,
                    configuration.family,
                    configuration.f0_enhancement,
                    configuration.speakers.clone(),
                ).with_acoustic_version(&configuration.version)
                .with_vocal_modes(Some(configuration.vocal_mode_names().to_vec()))?))
            })
            .clone()
    }

    pub fn preparation(&self) -> Option<&Arc<PreparedVoice>> {
        self.preparation.get()?.as_ref().ok()
    }

    pub fn identity(&self) -> &str {
        &self.content_hash
    }

    pub fn load(directory: &Path) -> Result<Self> {
        if super::nofs::is_file(directory) {
            return nofs::load(directory);
        }
        let nofs_files = super::nofs::paths(directory)?;
        if !nofs_files.is_empty() {
            if nofs_files.len() != 1 {
                return Err(VoiceError::new(
                    "SV1 目录包含多个声库文件，请从声库列表选择明确版本",
                ));
            }
            let has_dnni = fs::read_dir(directory)?.try_fold(false, |found, entry| {
                let path = entry?.path();
                Ok::<_, std::io::Error>(
                    found
                        || (path.is_file()
                            && path
                                .extension()
                                .is_some_and(|extension| extension.eq_ignore_ascii_case("dnni"))),
                )
            })?;
            if has_dnni {
                return Err(VoiceError::new(
                    "目录同时包含 SV1 与 SV2 模型，请选择明确的声库文件或版本目录",
                ));
            }
            return nofs::load(&nofs_files[0]);
        }
        let context = FormatContext::dnni_v3();
        let mut paths = fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.retain(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dnni"))
        });
        paths.sort();
        if paths.is_empty() {
            return Err(VoiceError::new(format!(
                "No DNNI model files in {}",
                directory.display()
            )));
        }
        let mut inventory = Self {
            database_id: directory
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned(),
            directory: directory.to_owned(),
            files: Vec::new(),
            configuration: None,
            display_name: String::new(),
            content_hash: String::new(),
            resource_count: 0,
            graph_count: 0,
            tensor_count: 0,
            _models: Vec::new(),
            preparation: OnceLock::new(),
        };
        for path in paths {
            let archive = Arc::new(Archive::load(&path, &context)?);
            archive.verify_all()?;
            let (file, trees) = inspect::file(&path, &archive)?;
            if let Some(index) = archive.index(CONFIGURATION_ID) {
                if inventory.configuration.is_some() {
                    return Err(VoiceError::new(
                        "Multiple voice configuration resources need explicit model routing",
                    ));
                }
                inventory.configuration =
                    Some(VoiceConfiguration::parse(&archive.read(index, None)?)?);
            }
            inventory.files.push(file);
            inventory._models.push(LoadedModel::Dnni { archive, trees });
        }
        if inventory.configuration.is_some() {
            let names = inventory.embedded_style_names()?;
            inventory
                .configuration
                .as_mut()
                .expect("checked configuration")
                .resolve_embedded_styles(&names)?;
        }
        let mut identity = Sha256::new();
        for file in &inventory.files {
            identity.update((file.name.len() as u64).to_le_bytes());
            identity.update(file.name.as_bytes());
            identity.update(file.sha256.as_bytes());
            inventory.resource_count += file.resources;
            inventory.graph_count += file.graphs.len();
            inventory.tensor_count += file
                .tensors
                .values()
                .map(|tensor| tensor.resources)
                .sum::<usize>();
        }
        inventory.content_hash = super::hex(&identity.finalize());
        inventory.display_name = inventory.configuration.as_ref().map_or_else(
            || "未识别声库".to_owned(),
            |configuration| configuration.name.clone(),
        );
        Ok(inventory)
    }
}
