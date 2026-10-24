use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

use super::{output::FrameMetadata, stage::VocoderStage};

#[derive(Clone)]
pub struct VocoderOutput {
    pub primary: Sequence,
    pub secondary: Sequence,
    pub metadata: Vec<FrameMetadata>,
}

impl VocoderStage<'_> {
    pub fn render(&mut self, physical: &Sequence, controls: Option<&Sequence>) -> Result<Vec<f32>> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.render");
        let output = self.render_components(physical, controls)?;
        let mut mono = output.primary.values;
        for (sample, secondary) in mono.iter_mut().zip(output.secondary.values) {
            *sample += secondary;
            if !sample.is_finite() {
                return Err(VoiceError::new("Vocoder produced non-finite mono samples"));
            }
        }
        Ok(mono)
    }

    pub fn render_components(
        &mut self,
        physical: &Sequence,
        controls: Option<&Sequence>,
    ) -> Result<VocoderOutput> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.render_components");
        self.clone()
            .into_stream()?
            .process(physical, controls, true)
    }
}
