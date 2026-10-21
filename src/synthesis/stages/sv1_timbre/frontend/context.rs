use super::PhoneInput;
use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    stages::f0::TimedPhone,
    voice::{
        PreparedModule, PreparedTree, Result, VoiceError, embedded::types::TIMBRE_CONTEXT_TYPE_ID,
    },
};

pub struct Context<'a> {
    tree: &'a PreparedTree,
    phone_projection: usize,
    pointwise: usize,
    convolution: usize,
    language: Option<usize>,
}

pub struct ContextInput<'a> {
    pub phones: &'a [TimedPhone],
    pub linguistic: &'a PhoneInput,
    pub normalized_pitch: &'a [f32],
    pub bank: &'a [f32],
    pub style: &'a [f32],
    pub frame_styles: Option<&'a Sequence>,
    pub frame_factors: Option<&'a [f32]>,
}

pub struct ContextOutput {
    pub convolution: Sequence,
    pub pointwise: Sequence,
}

impl<'a> Context<'a> {
    pub fn prepare(tree: &'a PreparedTree, index: usize) -> Result<Self> {
        let node = tree.node(index)?;
        if node.type_id != TIMBRE_CONTEXT_TYPE_ID
            || !matches!(node.children.len(), 3 | 4)
            || !tree.payload(index)?.is_empty()
        {
            return Err(VoiceError::new(
                "SV1 timbre context requires its typed three/four children",
            ));
        }
        let language = node.children.get(3).copied();
        if let Some(index) = language
            && !matches!(tree.node(index)?.module, PreparedModule::Embedding(_))
        {
            return Err(VoiceError::new(
                "SV1 acoustic language node is not an embedding",
            ));
        }
        Ok(Self {
            tree,
            phone_projection: node.children[0],
            pointwise: node.children[1],
            convolution: node.children[2],
            language,
        })
    }

    pub fn prepare_frames(&self, input: &ContextInput<'_>) -> Result<Sequence> {
        let frames = input
            .phones
            .iter()
            .try_fold(0_usize, |sum, phone| sum.checked_add(phone.frames))
            .ok_or_else(|| VoiceError::new("SV1 acoustic frame count overflows"))?;
        if let Some(styles) = input.frame_styles {
            styles.validate()?;
            if styles.frames != frames
                || styles.channels != input.style.len()
                || styles.values.iter().any(|value| !value.is_finite())
            {
                return Err(VoiceError::new(
                    "SV1 acoustic frame styles are not aligned with the context",
                ));
            }
        }
        if input.linguistic.normalized.frames != input.phones.len()
            || input.linguistic.language_ids.len() != input.phones.len()
            || input.normalized_pitch.len() != frames
            || input
                .frame_factors
                .is_some_and(|values| values.len() != frames)
            || input
                .normalized_pitch
                .iter()
                .chain(input.bank)
                .chain(input.style)
                .chain(input.frame_factors.into_iter().flatten())
                .any(|value| !value.is_finite())
        {
            return Err(VoiceError::new(
                "SV1 acoustic context inputs are not aligned",
            ));
        }
        let mut runtime = ModelRuntime::new(self.tree);
        let projected =
            runtime.execute(self.phone_projection, &input.linguistic.normalized, true)?;
        if projected.frames != input.phones.len() {
            return Err(VoiceError::new(
                "SV1 phone projection changed the record count",
            ));
        }
        let language = self
            .language
            .map(|index| {
                let ids = Sequence::from_values(
                    1,
                    input
                        .linguistic
                        .language_ids
                        .iter()
                        .map(|&id| id as f32)
                        .collect(),
                )?;
                runtime.execute(index, &ids, true)
            })
            .transpose()?;
        let language_width = language.as_ref().map_or(0, |values| values.channels);
        let channels = projected
            .channels
            .checked_add(8)
            .and_then(|width| width.checked_add(input.bank.len()))
            .and_then(|width| width.checked_add(language_width))
            .and_then(|width| width.checked_add(input.style.len()))
            .ok_or_else(|| VoiceError::new("SV1 acoustic conditioning width overflows"))?;
        let mut expanded = Sequence::zeros(frames, channels)?;
        let mut frame_index = 0;
        for (phone_index, phone) in input.phones.iter().enumerate() {
            let projection = projected.frame(phone_index)?;
            let language = language
                .as_ref()
                .map(|values| values.frame(phone_index))
                .transpose()?
                .unwrap_or(&[]);
            for position in 0..phone.frames {
                let factor = input
                    .frame_factors
                    .map_or(1.0, |values| values[frame_index]);
                let row =
                    &mut expanded.values[frame_index * channels..(frame_index + 1) * channels];
                for (target, &value) in row.iter_mut().zip(projection) {
                    *target = value * factor;
                }
                let fraction = if phone.frames == 1 {
                    0.5
                } else {
                    position as f32 * (1.0 / (phone.frames - 1) as f32)
                };
                let mut cursor = projected.channels;
                for scalar in [fraction, input.normalized_pitch[frame_index]] {
                    row[cursor..cursor + 4].copy_from_slice(&cosine_bank(scalar));
                    cursor += 4;
                }
                let style = input
                    .frame_styles
                    .map(|styles| styles.frame(frame_index))
                    .transpose()?
                    .unwrap_or(input.style);
                for values in [input.bank, language, style] {
                    row[cursor..cursor + values.len()].copy_from_slice(values);
                    cursor += values.len();
                }
                frame_index += 1;
            }
        }
        Ok(expanded)
    }

    pub fn process(
        &self,
        runtime: &mut ModelRuntime<'_>,
        expanded: &Sequence,
        flush: bool,
    ) -> Result<ContextOutput> {
        // Execute child slot 2 before slot 1. The convolution head retains its
        // right context between batches, so the caller aligns these
        // independently emitted branches by frame.
        let convolution = runtime.execute(self.convolution, expanded, flush)?;
        let pointwise = runtime.execute(self.pointwise, expanded, flush)?;
        Ok(ContextOutput {
            convolution,
            pointwise,
        })
    }
}

fn cosine_bank(value: f32) -> [f32; 4] {
    let angle = value * std::f32::consts::TAU;
    [
        std::f32::consts::FRAC_PI_2,
        0.0,
        -std::f32::consts::FRAC_PI_2,
        -std::f32::consts::PI,
    ]
    .map(|phase| (f64::from((angle + phase).cos()) * 0.5 + 0.5) as f32)
}
