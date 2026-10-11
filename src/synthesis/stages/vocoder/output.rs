//! CPU readback of the two vocoder outputs and renderer latency trimming.
//!
//! Both tensors are sliced into fixed-size frames, removing a prefix
//! configured by the timbre processor.
//! These are two internal synthesis components, not left/right stereo audio.

use std::{collections::VecDeque, fmt};

pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME_SAMPLES: usize = 240;

/// Metadata carried alongside the source frames, not inferred from the PCM.
#[derive(Clone, Copy, Debug)]
pub struct FrameMetadata {
    pub values: [f32; 2],
}

impl Default for FrameMetadata {
    fn default() -> Self {
        Self {
            values: [f32::NAN; 2],
        }
    }
}

#[derive(Clone, Debug)]
pub struct OutputFrame {
    /// Consecutive public output index, after the processor's warm-up prefix.
    pub index: u64,
    pub primary: [f32; FRAME_SAMPLES],
    pub secondary: [f32; FRAME_SAMPLES],
    pub metadata: FrameMetadata,
}

impl OutputFrame {
    /// Unity-gain mono downmix of the two internal synthesis components.
    /// Both are added into the same destination for mono output.
    /// There is no division by two, peak normalization or limiter at
    /// this boundary. Track gain and stereo panning belong to the mixer.
    pub fn mono(&self) -> Result<[f32; FRAME_SAMPLES], OutputError> {
        let mut samples = [0.0; FRAME_SAMPLES];
        for (sample, target) in samples.iter_mut().enumerate() {
            *target = self.primary[sample] + self.secondary[sample];
            if !target.is_finite() {
                return Err(OutputError::NonFiniteMix { sample });
            }
        }
        Ok(samples)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputError {
    AlreadyFinished,
    ComponentLengths { primary: usize, secondary: usize },
    IncompleteFrame { samples: usize },
    NonFiniteSample { component: usize, sample: usize },
    NonFiniteMix { sample: usize },
    MissingMetadata { frames: usize, pending: usize },
    IncompleteFinal { frames: usize, pending: usize },
    IndexOverflow,
    AllocationFailed,
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyFinished => f.write_str("Vocoder output stream has already finished"),
            Self::ComponentLengths { primary, secondary } => write!(
                f,
                "Vocoder output component lengths differ: {primary} and {secondary}"
            ),
            Self::IncompleteFrame { samples } => write!(
                f,
                "Vocoder output has {samples} samples, not a multiple of {FRAME_SAMPLES}"
            ),
            Self::NonFiniteSample { component, sample } => {
                write!(
                    f,
                    "Non-finite vocoder output component {component} sample {sample}"
                )
            }
            Self::NonFiniteMix { sample } => {
                write!(f, "Non-finite mono mix at vocoder frame sample {sample}")
            }
            Self::MissingMetadata { frames, pending } => write!(
                f,
                "Vocoder produced {frames} frames with only {pending} pending metadata records"
            ),
            Self::IncompleteFinal { frames, pending } => write!(
                f,
                "Final vocoder output has {frames} frames for {pending} pending metadata records"
            ),
            Self::IndexOverflow => f.write_str("Vocoder output frame index overflow"),
            Self::AllocationFailed => f.write_str("Could not allocate vocoder output frames"),
        }
    }
}

impl std::error::Error for OutputError {}

/// Stateful adapter shared by synchronous and queued vocoder execution.
/// The caller queues metadata when accepting input, then supplies completed
/// tensor batches in the same order. Only the explicit final batch closes it.
#[derive(Debug)]
pub struct OutputAssembler {
    metadata: VecDeque<FrameMetadata>,
    warmup_remaining: usize,
    emitted_frames: u64,
    finished: bool,
}

impl OutputAssembler {
    /// `warmup_frames` comes from the prepared processor's output latency.
    /// It is not a hardcoded edit-context radius or an audio fade duration.
    pub fn new(warmup_frames: usize) -> Self {
        Self {
            metadata: VecDeque::new(),
            warmup_remaining: warmup_frames,
            emitted_frames: 0,
            finished: false,
        }
    }

    pub fn queue_metadata(&mut self, frames: &[FrameMetadata]) -> Result<(), OutputError> {
        if self.finished {
            return Err(OutputError::AlreadyFinished);
        }
        self.metadata
            .try_reserve(frames.len())
            .map_err(|_| OutputError::AllocationFailed)?;
        self.metadata.extend(frames.iter().copied());
        Ok(())
    }

    /// No normalization, clipping, stereo mapping, tail padding or filtering is
    /// applied here.
    /// Validation and allocation finish before persistent state is modified.
    pub fn accept(
        &mut self,
        primary: &[f32],
        secondary: &[f32],
        final_batch: bool,
    ) -> Result<Vec<OutputFrame>, OutputError> {
        if self.finished {
            return Err(OutputError::AlreadyFinished);
        }
        if primary.len() != secondary.len() {
            return Err(OutputError::ComponentLengths {
                primary: primary.len(),
                secondary: secondary.len(),
            });
        }
        if !primary.len().is_multiple_of(FRAME_SAMPLES) {
            return Err(OutputError::IncompleteFrame {
                samples: primary.len(),
            });
        }
        for (component, samples) in [primary, secondary].into_iter().enumerate() {
            if let Some(sample) = samples.iter().position(|value| !value.is_finite()) {
                return Err(OutputError::NonFiniteSample { component, sample });
            }
        }
        let frames = primary.len() / FRAME_SAMPLES;
        let pending = self.metadata.len();
        if frames > pending {
            return Err(OutputError::MissingMetadata { frames, pending });
        }
        if final_batch && frames != pending {
            return Err(OutputError::IncompleteFinal { frames, pending });
        }
        let discard = self.warmup_remaining.min(frames);
        let visible = frames - discard;
        let next_index = self
            .emitted_frames
            .checked_add(visible as u64)
            .ok_or(OutputError::IndexOverflow)?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(visible)
            .map_err(|_| OutputError::AllocationFailed)?;
        for frame in discard..frames {
            let start = frame * FRAME_SAMPLES;
            let end = start + FRAME_SAMPLES;
            let mut first = [0.0; FRAME_SAMPLES];
            let mut second = [0.0; FRAME_SAMPLES];
            first.copy_from_slice(&primary[start..end]);
            second.copy_from_slice(&secondary[start..end]);
            output.push(OutputFrame {
                index: self.emitted_frames + (frame - discard) as u64,
                primary: first,
                secondary: second,
                metadata: self.metadata[frame],
            });
        }
        self.metadata.drain(..frames);
        self.warmup_remaining -= discard;
        self.emitted_frames = next_index;
        self.finished = final_batch;
        Ok(output)
    }
}
