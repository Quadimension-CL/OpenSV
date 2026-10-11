//! A single continuous vocoder execution, yielding only stable fresh PCM.

use std::collections::VecDeque;

use super::{
    output::FrameMetadata, render::VocoderOutput, stage::VocoderStage,
    stream_parameters::ParameterStream, stream_signals::SignalStream,
};
use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

pub struct VocoderStream<'a> {
    parameters: ParameterStream<'a>,
    signals: SignalStream,
    metadata: VecDeque<FrameMetadata>,
    input_frames: usize,
    output_frames: usize,
    finished: bool,
}

impl<'a> VocoderStage<'a> {
    pub fn into_stream(self) -> Result<VocoderStream<'a>> {
        let signals = SignalStream::new(&self)?;
        Ok(VocoderStream {
            parameters: ParameterStream::new(self)?,
            signals,
            metadata: VecDeque::new(),
            input_frames: 0,
            output_frames: 0,
            finished: false,
        })
    }
}

impl VocoderStream<'_> {
    /// Both inputs contain the next acoustic frames in order. Only a natural
    /// end of this model context may set `final_batch`; chunk boundaries do
    /// not reset phase, recurrent state, convolution history or overlap-add.
    /// The returned rows follow prior output rows and may be empty while the
    /// model accumulates its real right context.
    pub fn process(
        &mut self,
        physical: &Sequence,
        controls: Option<&Sequence>,
        final_batch: bool,
    ) -> Result<VocoderOutput> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.stream.process");
        if self.finished {
            return Err(VoiceError::new("Vocoder stream has already finished"));
        }
        let parameters = self.parameters.process(physical, controls, final_batch)?;
        self.input_frames = self
            .input_frames
            .checked_add(physical.frames)
            .ok_or_else(|| VoiceError::new("Vocoder input frame count overflow"))?;
        // The DSP source's first frame primes its overlap. There is no tail
        // at all for an empty context, so do not flush an unstarted source.
        if self.input_frames == 0 {
            self.finished = final_batch;
            return Ok(VocoderOutput {
                primary: Sequence::zeros(0, self.parameters.stage.condition.frame_samples)?,
                secondary: Sequence::zeros(0, self.parameters.stage.condition.frame_samples)?,
                metadata: Vec::new(),
            });
        }
        self.metadata.extend(parameters.metadata.iter().copied());
        let (primary, secondary) =
            self.signals
                .process(&mut self.parameters.runtime, parameters, final_batch)?;
        if primary.frames != secondary.frames || primary.frames > self.metadata.len() {
            return Err(VoiceError::new(
                "Vocoder PCM and metadata frames do not align",
            ));
        }
        self.output_frames = self
            .output_frames
            .checked_add(primary.frames)
            .ok_or_else(|| VoiceError::new("Vocoder output frame count overflow"))?;
        if final_batch
            && (self.output_frames != self.input_frames || primary.frames != self.metadata.len())
        {
            return Err(VoiceError::new(
                "Vocoder ended with pending acoustic frames",
            ));
        }
        let metadata = self.metadata.drain(..primary.frames).collect();
        self.finished = final_batch;
        Ok(VocoderOutput {
            primary,
            secondary,
            metadata,
        })
    }
}
