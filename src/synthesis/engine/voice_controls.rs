//! Owner-wide voice conditions shared by export and preview cursors.

mod phonemes;

use crate::{
    model::VocalModeComponent,
    synthesis::{
        inference::Sequence, score::PreparedPhrase, stages::timbre::records::FrameControl,
        voice::Result,
    },
};

use super::PreparedRenderAcoustics;

#[derive(Default)]
pub(super) struct AcousticControls {
    pub pronunciation: Option<Sequence>,
    pub timbre: Option<Sequence>,
    pub frames: Option<Vec<FrameControl>>,
}

impl AcousticControls {
    pub fn sample(
        acoustic: &PreparedRenderAcoustics,
        names: &[String],
        frame_seconds: f64,
        frames: usize,
    ) -> Result<Self> {
        let phrase = &acoustic.prepared.phrase;
        let mut frame_controls = phrase
            .voice_controls
            .as_ref()
            .map(|controls| {
                controls.sample_timbre_controls(phrase.start_seconds, frame_seconds, frames)
            })
            .transpose()?
            .flatten();
        phonemes::apply(acoustic, frame_seconds, frames, &mut frame_controls)?;
        Ok(Self {
            pronunciation: sample_styles(
                phrase,
                names,
                VocalModeComponent::Pronunciation,
                frame_seconds,
                frames,
            )?,
            timbre: sample_styles(
                phrase,
                names,
                VocalModeComponent::Timbre,
                frame_seconds,
                frames,
            )?,
            frames: frame_controls,
        })
    }
}

pub(super) fn sample_styles(
    phrase: &PreparedPhrase,
    names: &[String],
    component: VocalModeComponent,
    frame_seconds: f64,
    frames: usize,
) -> Result<Option<Sequence>> {
    phrase.voice_controls.as_ref().map_or(Ok(None), |controls| {
        controls.sample_styles(
            names,
            component,
            phrase.start_seconds,
            frame_seconds,
            frames,
        )
    })
}
