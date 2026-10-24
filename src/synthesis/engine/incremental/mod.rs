pub(super) mod acoustics;
mod range;

pub use range::IncrementalEdit;

use super::{RenderResult, RenderStage, progressive};
use crate::synthesis::{
    plan::OwnerInput,
    render_cache::CompletedOwner,
    score::LyricFrontend,
    voice::{PreparedVoice, Result},
};

pub fn synthesize_owner(
    voice: &PreparedVoice,
    identity: Option<u64>,
    input: &OwnerInput,
    lexicon: &LyricFrontend,
    base: Option<&CompletedOwner>,
    mut checkpoint: impl FnMut(RenderStage) -> Result<()>,
) -> Result<RenderResult> {
    let mut prepared =
        progressive::prepare(voice, identity, input, lexicon, base, &mut checkpoint)?;
    let network = progressive::Network::prepare(voice)?;
    while let Some(mut cursor) = prepared.start(&network, voice, 0.0)? {
        while !cursor.is_finished() {
            prepared.advance(&mut cursor, 128, &mut checkpoint)?;
        }
    }
    prepared.finish()
}
