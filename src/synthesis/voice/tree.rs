use super::{
    Result, VoiceError,
    archive::{Archive, ResourceId},
    array, u32_at,
};

#[derive(Clone, Copy)]
pub(super) struct NodeId(usize);

impl NodeId {
    pub(super) fn index(self) -> usize {
        self.0
    }
}

pub(super) struct ModuleNode {
    pub resource: ResourceId,
    pub children: Vec<NodeId>,
}

/// Flat postorder containment arena; not a tensor dataflow execution graph.
pub(super) struct ModuleTree {
    pub nodes: Vec<ModuleNode>,
    pub root: NodeId,
    pub maximum_depth: usize,
}

struct Frame {
    remaining: usize,
    children: Vec<NodeId>,
}

impl ModuleTree {
    pub fn parse(payload: &[u8], archive: &Archive) -> Result<Self> {
        if payload.is_empty() || !payload.len().is_multiple_of(20) {
            return Err(VoiceError::new(
                "Module tree length must be a positive multiple of 20",
            ));
        }
        let mut offset = 0;
        let mut opened = 0;
        let mut frames = Vec::new();
        let mut nodes = Vec::new();
        let mut maximum_depth = 0;
        open(payload, &mut offset, &mut opened, &mut frames)?;
        while let Some(frame) = frames.last_mut() {
            if frame.remaining != 0 {
                frame.remaining -= 1;
                open(payload, &mut offset, &mut opened, &mut frames)?;
                maximum_depth = maximum_depth.max(frames.len() - 1);
                continue;
            }
            let resource = ResourceId(array(payload, offset)?);
            offset += 16;
            if archive.index(resource).is_none() {
                return Err(VoiceError::new("Module tree refers to an absent resource"));
            }
            let index = NodeId(nodes.len());
            let frame = frames.pop().expect("current frame exists");
            nodes.push(ModuleNode {
                resource,
                children: frame.children,
            });
            if let Some(parent) = frames.last_mut() {
                parent.children.push(index);
            }
        }
        if offset != payload.len() || nodes.len() != payload.len() / 20 {
            return Err(VoiceError::new(
                "Module tree does not exactly consume its payload",
            ));
        }
        Ok(Self {
            root: NodeId(nodes.len() - 1),
            nodes,
            maximum_depth,
        })
    }

    pub fn root(&self) -> &ModuleNode {
        &self.nodes[self.root.0]
    }
}

fn open(
    payload: &[u8],
    offset: &mut usize,
    opened: &mut usize,
    frames: &mut Vec<Frame>,
) -> Result<()> {
    let children = u32_at(payload, *offset)? as usize;
    *offset += 4;
    *opened += 1;
    if *opened > payload.len() / 20 || children > (payload.len() - *offset) / 20 {
        return Err(VoiceError::new(
            "Module child count exceeds remaining tree bytes",
        ));
    }
    // Do not allocate by the untrusted child count before parsing its records.
    frames.push(Frame {
        remaining: children,
        children: Vec::new(),
    });
    Ok(())
}
