//! DDS pitch program with SV1 NOFS and SV2 compatibility input contracts.

mod controls;
mod features;
mod frontend;
mod sampler;
mod statistics;
mod timing;

use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    stages::f0::{PitchResult, TimedPhone, frontend::FrontendNote},
    voice::{
        PreparedTree, Result, VoiceError,
        embedded::types::{F0_ROOT_TYPE_ID, F0_WRAPPER_TYPE_ID},
    },
};

use frontend::Frontend;

#[derive(Clone, Copy)]
enum Contract {
    Compatible,
    Native { enhancement: f32 },
}

pub struct Sv1F0Stage<'a> {
    tree: &'a PreparedTree,
    frontend: Frontend<'a>,
    features: usize,
    sampler: usize,
    style: Vec<f32>,
    contract: Contract,
    defaults: controls::Defaults,
}

impl<'a> Sv1F0Stage<'a> {
    pub fn prepare(tree: &'a PreparedTree) -> Result<Self> {
        Self::prepare_contract(tree, Contract::Compatible)
    }

    pub fn prepare_native(tree: &'a PreparedTree, enhancement: f32) -> Result<Self> {
        if !enhancement.is_finite() {
            return Err(VoiceError::new("Native DDS F0 enhancement must be finite"));
        }
        Self::prepare_contract(
            tree,
            Contract::Native {
                enhancement: enhancement.clamp(0.0, 1.0),
            },
        )
    }

    fn prepare_contract(tree: &'a PreparedTree, contract: Contract) -> Result<Self> {
        let wrapper = tree.node(tree.root_index())?;
        let payload = tree.payload(tree.root_index())?;
        if wrapper.type_id != F0_WRAPPER_TYPE_ID
            || wrapper.children.len() != 1
            || payload.len() != 16
        {
            return Err(VoiceError::new("Expected the compatible DDS F0 wrapper"));
        }
        let parameters: Vec<f32> = payload
            .as_chunks::<4>()
            .0
            .iter()
            .take(3)
            .map(|value| f32::from_le_bytes(*value))
            .collect();
        if parameters.iter().any(|value| !value.is_finite()) {
            return Err(VoiceError::new("Compatible DDS F0 controls must be finite"));
        }
        let root = tree.node(wrapper.children[0])?;
        if root.type_id != F0_ROOT_TYPE_ID
            || root.children.len() != 5
            || !tree.payload(wrapper.children[0])?.is_empty()
        {
            return Err(VoiceError::new(
                "Expected the five-child compatible DDS F0 model",
            ));
        }
        let frontend = Frontend::prepare(tree, root.children[0])?;
        let mut style = tree
            .tensor(root.children[1])?
            .vector()
            .ok_or_else(|| VoiceError::new("Compatible F0 style is not a float vector"))?
            .to_vec();
        // Enable the default scalar style flag. Add its learned contribution
        // to the stored style independently of position C.
        let mut runtime = ModelRuntime::new(tree);
        let projection =
            runtime.execute(root.children[2], &Sequence::new(1, 1, vec![1.0])?, true)?;
        if projection.frames != 1 || projection.channels != style.len() {
            return Err(VoiceError::new(
                "Compatible F0 style projection width differs from the stored style",
            ));
        }
        for (style, &projected) in style.iter_mut().zip(&projection.values) {
            *style += projected;
        }
        Ok(Self {
            tree,
            frontend,
            features: root.children[3],
            sampler: root.children[4],
            style,
            contract,
            defaults: controls::Defaults {
                temperature: match contract {
                    Contract::Compatible => 1.0,
                    Contract::Native { .. } => parameters[0],
                },
                flagged_temperature_scale: parameters[2],
                position_scale: match contract {
                    Contract::Compatible => 1.0,
                    Contract::Native { .. } => parameters[1],
                },
            },
        })
    }

    pub fn note_frames(&self, notes: &[FrontendNote]) -> Result<Vec<usize>> {
        match self.contract {
            Contract::Compatible => frontend::note_frame_counts(notes, self.frontend.frame_seconds),
            Contract::Native { .. } => {
                timing::note_frame_counts(notes, self.frontend.frame_seconds)
            }
        }
    }

    pub fn predict_default(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
    ) -> Result<PitchResult> {
        if !matches!(self.contract, Contract::Compatible) {
            return Err(VoiceError::new(
                "Native DDS F0 requires continuous phone durations",
            ));
        }
        // The SV2 adapter consumes the duration stage's corrected phone frame
        // counts directly; it does not quantize NOFS phone seconds.
        let phone_frames: Vec<_> = phones.iter().map(|phone| phone.frames).collect();
        self.predict(notes, phones, &phone_frames)
    }

    pub fn predict_native(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
        phone_seconds: &[f64],
    ) -> Result<PitchResult> {
        if !matches!(self.contract, Contract::Native { .. }) || phone_seconds.len() != phones.len()
        {
            return Err(VoiceError::new(
                "Invalid native DDS F0 phone timing request",
            ));
        }
        let phone_frames = timing::phone_frame_counts(phone_seconds, self.frontend.frame_seconds)?;
        self.predict(notes, phones, &phone_frames)
    }

    fn predict(
        &self,
        notes: &[FrontendNote],
        phones: &[TimedPhone],
        phone_frames: &[usize],
    ) -> Result<PitchResult> {
        if notes.is_empty() || phones.is_empty() {
            return Err(VoiceError::new(
                "Compatible F0 needs aligned note, phone and duration records",
            ));
        }
        let note_frames = self.note_frames(notes)?;
        let transpose = transposition(notes)?;
        let encoded =
            self.frontend
                .encode(notes, phones, phone_frames, transpose, self.contract)?;
        let features = features::execute(
            self.tree,
            self.features,
            &encoded,
            &note_frames,
            phone_frames,
            &self.style,
        )?;
        let controls = controls::prepare(
            notes,
            &note_frames,
            phones,
            phone_frames,
            self.defaults,
            self.contract,
        )?;
        if controls.positions.len() != features.context.frames {
            return Err(VoiceError::new(
                "Compatible F0 rounded phone clock extends beyond the note controls",
            ));
        }
        let sampled = sampler::sample(
            self.tree,
            self.sampler,
            &features.source,
            &features.context,
            &controls.positions,
            &controls.noise,
        )?;
        let inverse = |value| {
            self.frontend
                .normalizer
                .denormalize_scalar(2, value)
                .map_err(|error| VoiceError::new(error.to_string()))
        };
        let preliminary = sampled
            .preliminary
            .iter()
            .map(|&value| inverse(value))
            .collect::<Result<Vec<_>>>()?;
        let predicted_controls = statistics::estimate(
            &preliminary,
            &encoded.vowel_mask,
            notes,
            &note_frames,
            transpose,
        )?;
        let mut pitch = Vec::with_capacity(sampled.pitch.len());
        for frame in 0..sampled.pitch.len() {
            // Initial boundary checks are asymmetric: frame 0 contributes only
            // its right neighbor, and frame 1 has no left term.
            let left = if frame > 1 {
                sampled.pitch[frame - 1] * 0.25
            } else {
                0.0
            };
            let center = if frame > 0 {
                sampled.pitch[frame] * 0.5
            } else {
                0.0
            };
            let right = sampled.pitch.get(frame + 1).copied().unwrap_or(0.0) * 0.25;
            pitch.push(inverse(left + center + right)? - transpose);
        }
        if pitch
            .iter()
            .chain(&preliminary)
            .any(|value| !value.is_finite())
        {
            return Err(VoiceError::new(
                "Compatible DDS F0 produced non-finite pitch",
            ));
        }
        Ok(PitchResult {
            pitch,
            frame_seconds: f64::from(self.frontend.frame_seconds),
            predicted_controls,
        })
    }
}

fn transposition(notes: &[FrontendNote]) -> Result<f32> {
    let mut minimum = f32::INFINITY;
    let mut maximum = f32::NEG_INFINITY;
    for note in notes {
        let pitch = note.pitch + note.pitch_offset;
        if !pitch.is_finite() {
            return Err(VoiceError::new("Compatible F0 note pitch is not finite"));
        }
        // Round the upper/lower bounds outward before computing the
        // whole-phrase shift shared by all notes.
        minimum = minimum.min(pitch.floor());
        maximum = maximum.max(pitch.ceil());
    }
    Ok(if minimum < 36.0 {
        36.0 - minimum
    } else if maximum > 79.0 {
        79.0 - maximum
    } else {
        0.0
    })
}
