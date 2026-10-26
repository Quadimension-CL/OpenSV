use super::{LoadedModel, LoadedTree, VoiceInventory};
use crate::synthesis::voice::{
    Result, VoiceError, embedded,
    speakers::{LIST_TYPE, STYLE_TYPE, style_names},
};

impl VoiceInventory {
    pub(super) fn embedded_style_names(&self) -> Result<Vec<String>> {
        let mut discovered = Vec::new();
        let acoustic_only = self.configuration.as_ref().is_some_and(|config| config.model_version == 1);
        for (model, file) in self._models.iter().zip(&self.files) {
            let (archive, trees) = match model {
                LoadedModel::Dnni { archive, trees } => (archive, trees),
                LoadedModel::Nofs { trees } => {
                    for (graph, bytes) in file.graphs.iter().zip(trees) {
                        if graph.role != "timbre" { continue; }
                        let records = embedded::parse(bytes)?;
                        let node = *records.last().ok_or_else(invalid)?.children.get(1).ok_or_else(invalid)?;
                        append(&mut discovered, names(node, true,
                            |index| records[index].children.clone(),
                            |index| Ok(records[index].bytes.clone()))?);
                    }
                    continue;
                }
            };
            for (graph, tree) in file.graphs.iter().zip(trees) {
                if graph.role != "timbre" && (acoustic_only || graph.role != "f0") {
                    continue;
                }
                let current = match tree {
                    LoadedTree::Indexed(tree) => {
                        let root = tree.root();
                        let node = root.children.get(1).ok_or_else(invalid)?.index();
                        names(
                            node,
                            acoustic_only,
                            |index| {
                                tree.nodes[index]
                                    .children
                                    .iter()
                                    .map(|child| child.index())
                                    .collect()
                            },
                            |index| {
                                archive
                                    .read(
                                        archive
                                            .index(tree.nodes[index].resource)
                                            .ok_or_else(invalid)?,
                                        None,
                                    )
                                    .map(|bytes| bytes.into_owned())
                            },
                        )
                    }
                    LoadedTree::Embedded { resource_index } => {
                        let records = embedded::parse(&archive.read(*resource_index, None)?)?;
                        let node = *records
                            .last()
                            .ok_or_else(invalid)?
                            .children
                            .get(1)
                            .ok_or_else(invalid)?;
                        names(
                            node,
                            acoustic_only,
                            |index| records[index].children.clone(),
                            |index| Ok(records[index].bytes.clone()),
                        )
                    }
                }?;
                append(&mut discovered, current);
            }
        }
        Ok(discovered)
    }
}

fn append(names: &mut Vec<String>, current: Vec<String>) {
    for name in current {
        if !names.contains(&name) { names.push(name); }
    }
}

fn names(
    node: usize,
    acoustic_only: bool,
    children: impl Fn(usize) -> Vec<usize>,
    read: impl Fn(usize) -> Result<Vec<u8>>,
) -> Result<Vec<String>> {
    let bytes = read(node)?;
    if bytes.starts_with(&STYLE_TYPE) || acoustic_only {
        let owned = children(node);
        if bytes.len() < 16 || owned.len() != 2 { return Err(invalid()); }
        let names = style_names(&bytes[16..])?;
        if children(owned[0]).len() != names.len() + 1 { return Err(invalid()); }
        return Ok(names);
    }
    if !bytes.starts_with(&LIST_TYPE) {
        return Err(invalid());
    }
    let mut names = Vec::new();
    for speaker in children(node) {
        let bytes = read(speaker)?;
        if !bytes.starts_with(&STYLE_TYPE) {
            return Err(invalid());
        }
        let current = style_names(&bytes[16..])?;
        for name in current {if !names.contains(&name) {names.push(name);}}
    }
    if children(node).is_empty() {return Err(invalid());}
    Ok(names)
}

fn invalid() -> VoiceError {
    VoiceError::new("Voice has no consistent embedded vocal-mode names")
}
