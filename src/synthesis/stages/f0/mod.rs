//! Mo Xu F0 program: note/phone encoding, expansion, two-pass prediction,
//! deterministic noise and pitch postprocessing.

pub mod condition;
mod execute;
pub mod expression;
pub mod features;
pub mod frontend;
mod input;
pub mod postprocess;
mod prepared;
pub mod sampler;
pub mod statistics;
pub mod style;
mod window;
pub use prepared::PreparedPitchFeatures;

use crate::synthesis::{
    inference::{Sequence, ops::range_normalizer::RangeNormalizer},
    voice::{PreparedTree, Result, VoiceError},
};

use condition::{NoteModulation, PitchSession};
use features::FeatureNetwork;
use frontend::{FrontendNote, NoteFrontend};
use sampler::F0Sampler;
use statistics::SamplerStatistics;
use style::StyleBasis;

pub const F0_ROOT_TYPE_ID: [u8; 16] = [
    0x99, 0xdf, 0xcd, 0x43, 0xff, 0x38, 0x9d, 0x0a, 0x13, 0xbc, 0xf4, 0xe0, 0x76, 0xbc, 0x4a, 0x3a,
];

/// One duration-stage phone with its corrected acoustic frame count.
#[derive(Clone, Debug)]
pub struct TimedPhone {
    pub note_index: usize,
    pub language: String,
    pub token: String,
    pub frames: usize,
}

pub struct PitchRequest<'a> {
    pub notes: &'a [FrontendNote],
    pub phones: &'a [TimedPhone],
    pub modulation: &'a [NoteModulation],
    /// Absolute pitch controls; NaN means unspecified.
    pub absolute_controls: &'a [f32],
    /// The pitch-expressiveness parameter sampled on the 5 ms frame grid.
    pub expressiveness: &'a [f32],
    /// Vocal-mode weights for every owner frame, in model style-name order.
    pub style_weights: Option<&'a Sequence>,
}

#[derive(Default)]
pub struct PitchInputControls<'a> {
    pub pitch_seeds: Option<&'a [Option<u32>]>,
    pub absolute_pitch: Option<&'a [f32]>,
    pub expressiveness: Option<&'a [f32]>,
    pub style_weights: Option<&'a Sequence>,
}

#[derive(Clone, Debug)]
pub struct PitchResult {
    /// Absolute model pitch after inverse normalization, pitch-range clamping,
    /// and user-control blending. This is not Hz.
    pub pitch: Vec<f32>,
    pub frame_seconds: f64,
    pub predicted_controls: SamplerStatistics,
}

pub struct F0Stage<'a> {
    tree: &'a PreparedTree,
    frontend: NoteFrontend,
    features: FeatureNetwork<'a>,
    style: StyleBasis,
    sampler: F0Sampler,
    note_normalizer: RangeNormalizer,
    pitch_normalizer: RangeNormalizer,
    fixed_style: Vec<f32>,
    pub session: PitchSession,
}

impl<'a> F0Stage<'a> {
    pub fn style_names(&self) -> &[String] {
        &self.style.names
    }

    pub fn prepare(tree: &'a PreparedTree) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("f0.prepare");
        let root = tree.node(tree.root_index())?;
        if root.type_id != F0_ROOT_TYPE_ID || !tree.payload(tree.root_index())?.is_empty() {
            return Err(VoiceError::new(
                "The supplied tree is not the researched F0 root",
            ));
        }
        let children: [usize; 5] = root
            .children
            .as_slice()
            .try_into()
            .map_err(|_| VoiceError::new("F0 root requires five children"))?;
        let frontend = NoteFrontend::prepare(tree, children[0])?;
        let note_normalizer = tree.range_normalizer(frontend.normalizer_node)?.clone();
        let pitch_normalizer = tree
            .range_normalizer(frontend.pitch_normalizer_node)?
            .clone();
        let fixed_style = tree
            .tensor(children[2])?
            .vector()
            .ok_or_else(|| VoiceError::new("F0 fixed style is not a float vector"))?
            .to_vec();
        let features = FeatureNetwork::prepare(tree, children[3])?;
        if fixed_style.len() != features.configuration().style_channels
            || pitch_normalizer.channels() < 2
        {
            return Err(VoiceError::new(
                "F0 fixed conditioning widths differ from its network",
            ));
        }
        Ok(Self {
            tree,
            frontend,
            features,
            style: StyleBasis::prepare(tree, children[1])?,
            sampler: F0Sampler::prepare(tree, children[4])?,
            note_normalizer,
            pitch_normalizer,
            fixed_style,
            session: PitchSession::default(),
        })
    }

    /// Evaluate the complete default-vocal-mode path, without a retake or drawn
    /// pitch curve. This still runs the actual model's two conditional passes.
    pub fn predict_default(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
    ) -> Result<PitchResult> {
        let prepared = self.prepare_features(notes, phones)?;
        self.predict_default_prepared(notes, phones, &prepared)
    }

    pub fn predict_default_prepared(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
        prepared: &PreparedPitchFeatures,
    ) -> Result<PitchResult> {
        self.predict_singing_prepared(notes, phones, prepared, PitchInputControls::default(), None)
    }

    pub fn predict_default_prepared_frames(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
        prepared: &PreparedPitchFeatures,
        frames: std::ops::Range<usize>,
    ) -> Result<PitchResult> {
        self.predict_singing_prepared(
            notes,
            phones,
            prepared,
            PitchInputControls::default(),
            Some(frames),
        )
    }

    /// Performance-conditioned singing with optional absolute pitch controls. Controls
    /// cover the complete owner even when only a work interval is evaluated.
    pub fn predict_singing_prepared(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
        prepared: &PreparedPitchFeatures,
        controls: PitchInputControls<'_>,
        frame_range: Option<std::ops::Range<usize>>,
    ) -> Result<PitchResult> {
        let PitchInputControls {
            pitch_seeds,
            absolute_pitch: absolute_controls,
            expressiveness,
            style_weights,
        } = controls;
        if pitch_seeds.is_some_and(|seeds| seeds.len() != notes.len()) {
            return Err(VoiceError::new(
                "F0 retake seeds differ from the note timeline",
            ));
        }
        let modulation: Vec<_> = notes
            .iter()
            .enumerate()
            .map(|(index, note)| {
                let mut modulation = NoteModulation {
                    frames: frontend::frame_count(note)?,
                    kind: note.kind,
                    flag: note.flag,
                    pitch_seed: pitch_seeds.and_then(|seeds| seeds[index]),
                    pitch_center: note.performance.r_intonation,
                    pitch_motion: note.performance.r_tone,
                    ..NoteModulation::default()
                };
                if let Some(controls) = expression::expression_controls(
                    note.kind,
                    note.performance.expression_x,
                    note.performance.expression_y,
                ) {
                    modulation.expression = Some(controls.expression);
                    modulation.position_c = Some(controls.position_c);
                    modulation.position_b = Some(controls.position_b);
                    modulation.accent = Some(controls.accent);
                }
                Ok(modulation)
            })
            .collect::<Result<_>>()?;
        let frames = modulation
            .iter()
            .try_fold(0_usize, |sum, note| sum.checked_add(note.frames))
            .ok_or_else(|| VoiceError::new("F0 request frame count overflow"))?;
        let absent_controls;
        let absolute_controls = match absolute_controls {
            Some(controls) => controls,
            None => {
                absent_controls = vec![f32::NAN; frames];
                &absent_controls
            }
        };
        let default_expressiveness;
        let expressiveness = match expressiveness {
            Some(values) => values,
            None => {
                default_expressiveness = notes
                    .iter()
                    .zip(&modulation)
                    .flat_map(|(note, modulation)| {
                        std::iter::repeat_n(note.performance.vibrato as f32, modulation.frames)
                    })
                    .collect::<Vec<_>>();
                &default_expressiveness
            }
        };
        let request = PitchRequest {
            notes,
            phones,
            modulation: &modulation,
            absolute_controls,
            expressiveness,
            style_weights,
        };
        match frame_range {
            Some(frames) => self.predict_prepared_frames(&request, prepared, frames),
            None => self.predict_prepared(&request, prepared),
        }
    }
}
