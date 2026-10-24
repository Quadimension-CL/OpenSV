use serde::Serialize;
use super::{PreparedVoice, PreparedTree, Result, VoiceError};

#[derive(Clone, Debug, Serialize)]
pub struct MemberVocalModes {
    pub speaker_id: usize,
    pub public_names: Vec<String>,
    pub pitch_names: Vec<String>,

    pub acoustic_names: Vec<String>,
}

pub(super) fn prepare(voice:&PreparedVoice) -> Result<Vec<MemberVocalModes>> {
    let public=voice.vocal_modes.as_deref().unwrap_or_default();
    let mut catalogues=Vec::with_capacity(voice.speakers.len());
    for speaker in &voice.speakers {
        let pitch_names=names(voice.model("f0")?,speaker.id)?;
        let acoustic_names=names(voice.model("timbre")?,speaker.id)?;
        let public_names=public.iter().filter(|name| pitch_names.contains(name)||acoustic_names.contains(name)).cloned().collect();
        catalogues.push(MemberVocalModes {speaker_id:speaker.id,public_names,pitch_names,acoustic_names});
    }
    Ok(catalogues)
}

fn names(tree:&PreparedTree,speaker:usize) -> Result<Vec<String>> {
    let root=tree.node(tree.root_index())?;
    let book=*root.children.get(1).ok_or_else(||VoiceError::new("Member has no style ownership slot"))?;
    let selected=super::speakers::style(tree,book,Some(speaker))?;
    let names=super::speakers::style_names(&tree.payload(selected)?)?;

    let basis=crate::synthesis::stages::f0::style::StyleBasis::prepare(tree,selected)?;
    if basis.names!=names {return Err(VoiceError::new("Member style metadata differs from its basis"));}
    Ok(names)
}
