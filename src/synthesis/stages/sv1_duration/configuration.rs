use crate::synthesis::{
    inference::ModelRuntime,
    voice::{PreparedModule, PreparedTree, Result, VoiceError, embedded::types},
};

#[derive(Clone, Copy)]
pub(super) struct Configuration {
    pub language: usize,
    pub phone: usize,
    pub position: usize,
    pub duration: usize,
    pub pitch: usize,
    pub width: usize,
}

pub(super) fn validate(tree: &PreparedTree) -> Result<(Configuration, [usize; 6])> {
    let root = tree.node(tree.root_index())?;
    if root.type_id != types::DURATION_TYPE_ID {
        return Err(VoiceError::new("Not an original SV1 duration model"));
    }
    let children: [usize; 6] = root
        .children
        .as_slice()
        .try_into()
        .map_err(|_| VoiceError::new("SV1 duration requires six serialized children"))?;
    let payload = tree.payload(tree.root_index())?;
    let bytes: &[u8; 8] = payload
        .as_ref()
        .try_into()
        .map_err(|_| VoiceError::new("SV1 duration requires two u32 encoding widths"))?;
    let duration = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    let pitch = u32::from_le_bytes(bytes[4..].try_into().unwrap()) as usize;
    if [duration, pitch]
        .iter()
        .any(|width| *width < 2 || !width.is_multiple_of(2))
    {
        return Err(VoiceError::new("Invalid SV1 duration encoding widths"));
    }
    let embedding_width = |node| -> Result<usize> {
        let PreparedModule::Embedding(embedding) = &tree.node(node)?.module else {
            return Err(VoiceError::new("SV1 duration requires typed embeddings"));
        };
        if embedding.features() == 0 || embedding.entries() == 0 {
            return Err(VoiceError::new("SV1 duration embedding is empty"));
        }
        Ok(embedding.features())
    };
    let language = embedding_width(children[2])?;
    let phone = embedding_width(children[3])?;
    let position = embedding_width(children[4])?;
    let constant = tree
        .tensor(children[5])?
        .vector()
        .filter(|values| !values.is_empty())
        .ok_or_else(|| VoiceError::new("SV1 duration constant is not a float vector"))?;
    let width = [
        language,
        phone,
        position,
        position,
        duration,
        pitch,
        constant.len(),
    ]
    .into_iter()
    .try_fold(0_usize, |sum, width| sum.checked_add(width))
    .ok_or_else(|| VoiceError::new("SV1 duration feature width overflows"))?;
    if !matches!(tree.node(children[1])?.module, PreparedModule::Sequential)
        || ModelRuntime::new(tree).input_channels(children[1])? != Some(width)
    {
        return Err(VoiceError::new(
            "SV1 duration scalar network does not match its feature dimensions",
        ));
    }
    super::frontend::validate(tree, children[0])?;
    Ok((
        Configuration {
            language,
            phone,
            position,
            duration,
            pitch,
            width,
        },
        children,
    ))
}
