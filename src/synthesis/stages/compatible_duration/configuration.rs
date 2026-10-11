use crate::synthesis::{
    stages::duration::frontend,
    voice::{PreparedModule, PreparedTree, Result, VoiceError},
};

use super::{FRONTEND_TYPE_ID, ROOT_TYPE_ID};

pub(super) fn validate(tree: &PreparedTree) -> Result<(Configuration, [usize; 9])> {
    let root = tree.node(tree.root_index())?;
    if root.type_id != ROOT_TYPE_ID {
        return Err(VoiceError::new(
            "Not a version-1 compatibility duration model",
        ));
    }
    let children: [usize; 9] =
        root.children.as_slice().try_into().map_err(|_| {
            VoiceError::new("Compatibility duration requires nine serialized children")
        })?;
    let configuration = Configuration::parse(&tree.payload(tree.root_index())?)?;
    let constant = tree
        .tensor(children[1])?
        .vector()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| VoiceError::new("Compatibility duration constant is not a vector"))?;
    let input = tree.affine(children[5])?;
    let identity_projection = tree.affine(children[6])?;
    if input.input_channels() != configuration.feature_width(constant.len())?
        || input.output_channels() != configuration.context
        || identity_projection.input_channels() != 64
        || identity_projection.output_channels() != configuration.context
    {
        return Err(VoiceError::new(
            "Compatibility duration projection dimensions do not match its configuration",
        ));
    }
    for (index, width) in [
        (2, configuration.language),
        (3, configuration.phone),
        (4, configuration.position),
    ] {
        let PreparedModule::Embedding(embedding) = &tree.node(children[index])?.module else {
            return Err(VoiceError::new(
                "Compatibility duration requires typed embeddings",
            ));
        };
        if embedding.features() != width || embedding.entries() == 0 {
            return Err(VoiceError::new(
                "Compatibility duration embedding dimensions do not match",
            ));
        }
    }
    if !matches!(tree.node(children[7])?.module, PreparedModule::ModuleList)
        || tree.node(children[7])?.children.is_empty()
        || !matches!(tree.node(children[8])?.module, PreparedModule::Sequential)
    {
        return Err(VoiceError::new(
            "Invalid compatibility duration context or scalar head",
        ));
    }
    frontend::prepare_explicit_typed(tree, &[], FRONTEND_TYPE_ID)?;
    let input_frontend = tree.node(children[0])?;
    for &node in &input_frontend.children[2..] {
        if tree.range_normalizer(node)?.channels() != 1 {
            return Err(VoiceError::new(
                "Compatibility duration normalizers must have one channel",
            ));
        }
    }
    Ok((configuration, children))
}

#[derive(Clone, Copy)]
pub(super) struct Configuration {
    pub language: usize,
    pub phone: usize,
    pub position: usize,
    pub duration: usize,
    pub pitch: usize,
    pub context: usize,
}

impl Configuration {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let bytes: &[u8; 12] = payload.try_into().map_err(|_| {
            VoiceError::new("Compatibility duration requires six u16 configuration fields")
        })?;
        let fields: Vec<_> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|word| usize::from(u16::from_le_bytes([word[0], word[1]])))
            .collect();
        if fields.contains(&0)
            || fields[3] < 2
            || fields[4] < 2
            || !fields[3].is_multiple_of(2)
            || !fields[4].is_multiple_of(2)
        {
            return Err(VoiceError::new("Invalid compatibility duration dimensions"));
        }
        Ok(Self {
            language: fields[0],
            phone: fields[1],
            position: fields[2],
            duration: fields[3],
            pitch: fields[4],
            context: fields[5],
        })
    }

    pub fn feature_width(self, constant: usize) -> Result<usize> {
        [
            self.language,
            self.phone,
            self.position,
            self.position,
            self.duration,
            self.pitch,
            constant,
        ]
        .into_iter()
        .try_fold(0_usize, |total, width| total.checked_add(width))
        .ok_or_else(|| VoiceError::new("Compatibility duration feature width overflows"))
    }
}
