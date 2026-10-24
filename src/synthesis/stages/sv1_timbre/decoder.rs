use super::queue::{Rows, align};
use crate::synthesis::{
    inference::{ModelRuntime, Sequence},
    voice::Result,
};

pub(super) struct Decoder {
    spectral_context: Rows,
    latent: Rows,
    auxiliary_context: Rows,
    spectral_auxiliary: Rows,
    spectral_output: Rows,
    auxiliary_output: Rows,
}

impl Decoder {
    pub fn new(context_channels: usize, latent_channels: usize) -> Self {
        Self {
            spectral_context: Rows::new(context_channels),
            latent: Rows::new(latent_channels),
            auxiliary_context: Rows::new(context_channels),
            spectral_auxiliary: Rows::new(64),
            spectral_output: Rows::new(64),
            auxiliary_output: Rows::new(7),
        }
    }

    pub fn process(
        &mut self,
        runtime: &mut ModelRuntime<'_>,
        nodes: [usize; 4],
        context: &Sequence,
        latent: &Sequence,
        flush: bool,
    ) -> Result<Sequence> {
        self.spectral_context.push(context)?;
        self.auxiliary_context.push(context)?;
        self.latent.push(latent)?;
        let (context, latent) = align(&mut self.spectral_context, &mut self.latent, flush)?;
        let hidden = runtime.execute_conditioned(nodes[0], &context, &latent, flush)?;
        let spectral = runtime.execute(nodes[1], &hidden, flush)?;
        self.spectral_auxiliary.push(&spectral)?;
        self.spectral_output.push(&spectral)?;
        let (context, spectral) = align(
            &mut self.auxiliary_context,
            &mut self.spectral_auxiliary,
            flush,
        )?;
        let hidden = runtime.execute_conditioned(nodes[2], &context, &spectral, flush)?;
        self.auxiliary_output
            .push(&runtime.execute(nodes[3], &hidden, flush)?)?;
        let (spectral, auxiliary) =
            align(&mut self.spectral_output, &mut self.auxiliary_output, flush)?;
        let mut output = Sequence::zeros(spectral.frames, 71)?;
        for ((row, spectral), auxiliary) in output
            .values
            .as_chunks_mut::<71>()
            .0
            .iter_mut()
            .zip(spectral.values.as_chunks::<64>().0.iter())
            .zip(auxiliary.values.as_chunks::<7>().0.iter())
        {
            row[..2].copy_from_slice(&auxiliary[..2]);
            row[2..66].copy_from_slice(spectral);
            row[66..].copy_from_slice(&auxiliary[2..]);
        }
        Ok(output)
    }
}
