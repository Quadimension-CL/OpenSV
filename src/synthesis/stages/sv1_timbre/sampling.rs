//! `_didsv0`: the single-pass acoustic latent head.

use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    voice::{PreparedModule, PreparedTree, Result, VoiceError, embedded::types},
};

pub(super) struct Sampler {
    input: usize,
    conditional: usize,
    output: usize,
    pub noise_channels: usize,
    embedding_channels: usize,
    embedding_scale: f32,
    pub context_channels: usize,
    pub feature_channels: usize,
}

pub(super) struct SamplerInput<'a> {
    pub context: &'a Sequence,
    pub features: &'a Sequence,
    pub noise: &'a Sequence,
    pub shape: &'a [f32],
    pub offset: &'a [f32],
}

impl Sampler {
    pub fn prepare(tree: &PreparedTree, index: usize) -> Result<Self> {
        let node = tree.node(index)?;
        let payload = tree.payload(index)?;
        if node.type_id != types::TIMBRE_DIFFUSION_TYPE_ID || payload.len() != 12 {
            return Err(VoiceError::new(
                "Unsupported compatible acoustic latent head",
            ));
        }
        let children: [usize; 3] = node.children.as_slice().try_into().map_err(|_| {
            VoiceError::new("Compatible acoustic latent head requires three children")
        })?;
        let word = |offset| u32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap());
        let noise_channels = word(0) as usize;
        let embedding_channels = word(4) as usize;
        let embedding_scale = f32::from_bits(word(8));
        let PreparedModule::Conditional(config) = &tree.node(children[1])?.module else {
            return Err(VoiceError::new(
                "Compatible acoustic latent head needs a conditional stack",
            ));
        };
        let context_channels = config
            .condition_channels
            .checked_sub(2 * embedding_channels + noise_channels)
            .filter(|&channels| channels > 0)
            .ok_or_else(|| VoiceError::new("Invalid compatible acoustic condition dimensions"))?;
        if noise_channels == 0
            || embedding_channels == 0
            || !embedding_channels.is_multiple_of(2)
            || !embedding_scale.is_finite()
            || config.input_channels != noise_channels
            || tree.affine(children[0])?.output_channels() != noise_channels
        {
            return Err(VoiceError::new(
                "Invalid compatible acoustic latent dimensions",
            ));
        }
        Ok(Self {
            input: children[0],
            conditional: children[1],
            output: children[2],
            noise_channels,
            embedding_channels,
            embedding_scale,
            context_channels,
            feature_channels: tree.affine(children[0])?.input_channels(),
        })
    }

    pub fn process(
        &self,
        runtime: &mut ModelRuntime<'_>,
        input: SamplerInput<'_>,
        flush: bool,
    ) -> Result<Sequence> {
        let SamplerInput {
            context,
            features,
            noise,
            shape,
            offset,
        } = input;
        context.validate()?;
        features.validate()?;
        noise.validate()?;
        if context.channels != self.context_channels
            || noise.channels != self.noise_channels
            || context.frames != features.frames
            || context.frames != noise.frames
            || shape.len() != context.frames
            || offset.len() != context.frames
        {
            return Err(VoiceError::new(
                "Compatible acoustic sampler input dimensions differ",
            ));
        }
        let input = runtime.execute(self.input, features, flush)?;
        let channels = self.context_channels + 2 * self.embedding_channels + self.noise_channels;
        let mut condition = Sequence::zeros(context.frames, channels)?;
        for (frame, row) in condition.values.chunks_exact_mut(channels).enumerate() {
            row[..self.context_channels].copy_from_slice(
                &context.values[frame * context.channels..(frame + 1) * context.channels],
            );
            let first = self.context_channels;
            let second = first + self.embedding_channels;
            let end = second + self.embedding_channels;
            self.embed(shape[frame], &mut row[first..second])?;
            self.embed(offset[frame], &mut row[second..end])?;
            row[end..].copy_from_slice(
                &noise.values[frame * noise.channels..(frame + 1) * noise.channels],
            );
        }
        let hidden = runtime.execute_conditioned(self.conditional, &input, &condition, flush)?;
        let output = runtime.execute(self.output, &hidden, flush)?;
        if output.channels != self.noise_channels {
            return Err(VoiceError::new(
                "Compatible acoustic latent output dimensions differ",
            ));
        }
        Ok(output)
    }

    fn embed(&self, value: f32, output: &mut [f32]) -> Result<()> {
        if !value.is_finite() {
            return Err(VoiceError::new(
                "Non-finite compatible acoustic sampling control",
            ));
        }
        let half = output.len() / 2;
        let rate = f32::from_bits(0xc1d4_9a78) / output.len() as f32;
        let scale = self.embedding_scale * value;
        for index in 0..half {
            let angle = (index as f32 * rate).exp2() * scale;
            output[index] = angle.sin();
            output[index + half] = angle.cos();
        }
        Ok(())
    }
}
