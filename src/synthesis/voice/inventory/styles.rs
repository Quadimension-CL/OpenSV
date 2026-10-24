use super::{LoadedModel, LoadedTree, VoiceInventory};
use crate::synthesis::voice::{
    Result, VoiceError, embedded,
    speakers::{LIST_TYPE, STYLE_TYPE, style_names},
};

impl VoiceInventory {
    pub(super) fn embedded_style_names(&self) -> Result<Vec<String>> {
        for (model, file) in self._models.iter().zip(&self.files) {
            let LoadedModel::Dnni { archive, trees } = model else {
                continue;
            };
            for (graph, tree) in file.graphs.iter().zip(trees) {
                if graph.role != "f0" {
                    continue;
                }
                return match tree {
                    LoadedTree::Indexed(tree) => {
                        let root = tree.root();
                        let node = root.children.get(1).ok_or_else(invalid)?.index();
                        names(
                            node,
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
                            |index| records[index].children.clone(),
                            |index| Ok(records[index].bytes.clone()),
                        )
                    }
                };
            }
        }
        Err(invalid())
    }
}

fn names(
    node: usize,
    children: impl Fn(usize) -> Vec<usize>,
    read: impl Fn(usize) -> Result<Vec<u8>>,
) -> Result<Vec<String>> {
    let bytes = read(node)?;
    if bytes.starts_with(&STYLE_TYPE) {
        return style_names(&bytes[16..]);
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
