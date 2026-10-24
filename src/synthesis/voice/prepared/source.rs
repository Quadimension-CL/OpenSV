use std::{borrow::Cow, sync::Arc};

use crate::synthesis::voice::{Result, VoiceError, archive::Archive};

pub(super) enum ModelSource {
    Indexed(Arc<Archive>),
    Inline(Vec<Vec<u8>>),
}

impl ModelSource {
    pub(super) fn read(&self, index: usize) -> Result<Cow<'_, [u8]>> {
        match self {
            Self::Indexed(archive) => archive.read(index, None),
            Self::Inline(records) => records
                .get(index)
                .map(|bytes| Cow::Borrowed(bytes.as_slice()))
                .ok_or_else(|| VoiceError::new("Embedded model resource is absent")),
        }
    }
}
