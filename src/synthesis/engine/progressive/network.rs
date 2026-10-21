//! Voice-family dispatch for the shared demand-driven PCM pipeline.
use crate::synthesis::{
    engine::{PreparedRenderAcoustics, models, voice_controls::AcousticControls},
    stages::{
        sv1_timbre::{PreparedSv1TimbreFrames, Sv1TimbreNetwork, Sv1TimbreStream},
        timbre::{
            PreparedTimbreFrames, TimbreNetwork, TimbreOutput, TimbreRequest, TimbreStream,
            records::RecordsConfig,
        },
    },
    voice::{PreparedVoice, Result, VoiceError, VoiceFamily},
};
use std::sync::Arc;

pub enum Network<'tree> {
    Dds(Box<Sv1TimbreNetwork<'tree>>),
    Sv2(Box<TimbreNetwork<'tree>>),
}

pub(super) enum PreparedFrames {
    Dds(Arc<PreparedSv1TimbreFrames>),
    Sv2 {
        features: Arc<PreparedTimbreFrames>,
        controls: AcousticControls,
    },
}

pub(super) enum Stream<'network, 'tree> {
    Dds(Box<Sv1TimbreStream<'network, 'tree>>),
    Sv2(Box<TimbreStream<'network, 'tree>>),
}

impl<'tree> Network<'tree> {
    pub fn prepare(voice: &'tree PreparedVoice) -> Result<Self> {
        let tree = voice.model("timbre")?;
        match voice.family {
            VoiceFamily::Sv1 | VoiceFamily::Sv2Compatible => {
                Ok(Self::Dds(Box::new(models::dds_timbre(voice)?)))
            }
            VoiceFamily::Sv2 => Ok(Self::Sv2(Box::new(TimbreNetwork::prepare(tree)?))),
        }
    }

    pub(super) fn prepare_frames(
        &self,
        acoustic: &PreparedRenderAcoustics,
        voice: &PreparedVoice,
    ) -> Result<PreparedFrames> {
        let prepared = &acoustic.prepared;
        match self {
            Self::Dds(network) => {
                let modulation = acoustic.modulation()?;
                let controls = AcousticControls::sample(
                    acoustic,
                    &voice.style_names,
                    network.frame_seconds(),
                    prepared.phones.iter().map(|phone| phone.frames).sum(),
                )?;
                let frames = network.prepare_frame_features(
                    &TimbreRequest {
                        notes: &prepared.phrase.notes,
                        phones: &prepared.phones,
                        midi_pitch: &acoustic.pitch.pitch,
                        note_modulation: &modulation,
                        phone_impulses: None,
                        frame_controls: controls.frames.as_deref(),
                        primary_style_weights: controls.pronunciation.as_ref(),
                        secondary_style_weights: controls.timbre.as_ref(),
                        requested_frame: 0,
                    },
                    &acoustic.pitch,
                )?;
                Ok(PreparedFrames::Dds(Arc::new(frames)))
            }
            Self::Sv2(network) => {
                let controls = AcousticControls::sample(
                    acoustic,
                    &voice.style_names,
                    crate::synthesis::score::FRAME_SECONDS,
                    acoustic.pitch.pitch.len(),
                )?;
                let features = network.prepare_frame_features(
                    &prepared.phrase.notes,
                    &prepared.phones,
                    prepared.sv2_features()?.1,
                )?;
                Ok(PreparedFrames::Sv2 {
                    features: Arc::new(features),
                    controls,
                })
            }
        }
    }

    pub(super) fn start<'network>(
        &'network self,
        acoustic: &PreparedRenderAcoustics,
        frames: &PreparedFrames,
        requested_frame: usize,
    ) -> Result<Stream<'network, 'tree>> {
        let prepared = &acoustic.prepared;
        let modulation = acoustic.modulation()?;
        let impulses = acoustic.impulses();
        let request = TimbreRequest {
            notes: &prepared.phrase.notes,
            phones: &prepared.phones,
            midi_pitch: &acoustic.pitch.pitch,
            note_modulation: &modulation,
            phone_impulses: None,
            frame_controls: None,
            primary_style_weights: None,
            secondary_style_weights: None,
            requested_frame,
        };
        match (self, frames) {
            (Self::Dds(network), PreparedFrames::Dds(frames)) => network
                .start_prepared(request, Arc::clone(frames))
                .map(|stream| Stream::Dds(Box::new(stream))),
            (Self::Sv2(network), PreparedFrames::Sv2 { features, controls }) => network
                .start_prepared(
                    &TimbreRequest {
                        phone_impulses: Some(&impulses),
                        frame_controls: controls.frames.as_deref(),
                        primary_style_weights: controls.pronunciation.as_ref(),
                        secondary_style_weights: controls.timbre.as_ref(),
                        ..request
                    },
                    &RecordsConfig::default(),
                    Arc::clone(features),
                )
                .map(|stream| Stream::Sv2(Box::new(stream))),
            _ => Err(VoiceError::new(
                "Progressive frame conditions belong to another model family",
            )),
        }
    }
}

impl Stream<'_, '_> {
    pub fn first_frame(&self) -> usize {
        match self {
            Self::Dds(stream) => stream.first_frame(),
            Self::Sv2(stream) => stream.first_frame(),
        }
    }

    pub fn is_finished(&self) -> bool {
        match self {
            Self::Dds(stream) => stream.is_finished(),
            Self::Sv2(stream) => stream.is_finished(),
        }
    }

    pub fn next(&mut self, max_frames: usize) -> Result<TimbreOutput> {
        match self {
            Self::Dds(stream) => stream.next(max_frames),
            Self::Sv2(stream) => stream.next(max_frames),
        }
    }
}
