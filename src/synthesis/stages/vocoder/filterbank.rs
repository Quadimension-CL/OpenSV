use std::sync::Arc;

use crate::synthesis::{
    execution::checkpoint,
    voice::{PreparedTree, Result, VoiceError},
};

pub const BANDS: usize = 8;
pub const TAPS: usize = 65;
pub const HALF_TAPS: usize = TAPS / 2;

pub const ANALYSIS_TYPE_ID: [u8; 16] = [
    0x41, 0x13, 0x61, 0x8e, 0x11, 0x99, 0x35, 0xdc, 0x90, 0xf5, 0x39, 0xfc, 0x8e, 0x9e, 0x46, 0xde,
];
pub const SYNTHESIS_TYPE_ID: [u8; 16] = [
    0x0d, 0x49, 0x6d, 0x65, 0x5f, 0xfe, 0x23, 0xe8, 0x1b, 0x1e, 0x4d, 0xf0, 0x0f, 0xba, 0x72, 0x29,
];

#[derive(Clone)]
struct Kernels(Arc<[f32]>);

impl Kernels {
    fn prepare(tree: &PreparedTree, index: usize, type_id: [u8; 16]) -> Result<Self> {
        let node = tree.node(index)?;
        if node.type_id != type_id || node.payload_bytes != 0 || node.children.len() != BANDS {
            return Err(VoiceError::new(
                "Expected an eight-band vocoder filter bank",
            ));
        }
        let mut weights = vec![0.0; TAPS * BANDS];
        for (band, &child) in node.children.iter().enumerate() {
            let vector = tree
                .tensor(child)?
                .vector()
                .filter(|vector| vector.len() == TAPS)
                .ok_or_else(|| VoiceError::new("Vocoder filter must contain 65 float taps"))?;
            for (tap, &value) in vector.iter().enumerate() {
                if !value.is_finite() {
                    return Err(VoiceError::new("Vocoder filter contains a non-finite tap"));
                }
                weights[tap * BANDS + band] = value;
            }
        }
        Ok(Self(weights.into()))
    }
}

#[derive(Clone)]
pub struct AnalysisBank {
    kernels: Kernels,
    pending: Vec<f32>,
    finished: bool,
}

impl AnalysisBank {
    pub fn prepare(tree: &PreparedTree, index: usize) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.filterbank.analysis.prepare");
        Ok(Self {
            kernels: Kernels::prepare(tree, index, ANALYSIS_TYPE_ID)?,
            pending: vec![0.0; HALF_TAPS],
            finished: false,
        })
    }

    pub fn reset(&mut self) {
        self.pending.clear();
        self.pending.resize(HALF_TAPS, 0.0);
        self.finished = false;
    }

    pub fn process(&mut self, samples: &[f32], end: bool) -> Result<Vec<f32>> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.filterbank.analysis.process");
        validate_input(self.finished, samples)?;
        let padding = if end { HALF_TAPS } else { 0 };
        let additional = samples
            .len()
            .checked_add(padding)
            .ok_or_else(|| VoiceError::new("Analysis filter input length overflow"))?;
        self.pending
            .try_reserve(additional)
            .map_err(|_| VoiceError::new("Could not allocate analysis filter overlap"))?;
        self.pending.extend_from_slice(samples);
        self.pending.resize(self.pending.len() + padding, 0.0);
        let frames = if self.pending.len() < TAPS {
            0
        } else {
            (self.pending.len() - TAPS) / BANDS + 1
        };
        let mut output = zeroes(frames * BANDS)?;
        for (frame, target) in output.as_chunks_mut::<BANDS>().0.iter_mut().enumerate() {
            if frame.is_multiple_of(128) {
                checkpoint()?;
            }
            let window = &self.pending[frame * BANDS..][..TAPS];
            for (tap, &sample) in window.iter().enumerate() {
                for (value, &weight) in target
                    .iter_mut()
                    .zip(&self.kernels.0[tap * BANDS..][..BANDS])
                {
                    *value += sample * weight;
                }
            }
        }
        self.pending.drain(..frames * BANDS);
        self.finished = end;
        validate_output(&output)?;
        Ok(output)
    }
}

#[derive(Clone)]
pub struct SynthesisBank {
    kernels: Kernels,
    pending: Vec<f32>,

    advance: usize,

    skip: usize,
    finished: bool,
}

impl SynthesisBank {
    pub fn prepare(tree: &PreparedTree, index: usize) -> Result<Self> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.filterbank.synthesis.prepare");
        Ok(Self {
            kernels: Kernels::prepare(tree, index, SYNTHESIS_TYPE_ID)?,
            pending: vec![0.0; TAPS - BANDS],
            advance: 0,
            skip: HALF_TAPS,
            finished: false,
        })
    }

    pub fn reset(&mut self) {
        self.pending.clear();
        self.pending.resize(TAPS - BANDS, 0.0);
        self.advance = 0;
        self.skip = HALF_TAPS;
        self.finished = false;
    }

    pub fn process(&mut self, subbands: &[f32], end: bool) -> Result<Vec<f32>> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("vocoder.filterbank.synthesis.process");
        validate_input(self.finished, subbands)?;
        if !subbands.len().is_multiple_of(BANDS) {
            return Err(VoiceError::new("Incomplete eight-channel vocoder frame"));
        }
        self.pending
            .try_reserve(subbands.len())
            .map_err(|_| VoiceError::new("Could not allocate synthesis filter overlap"))?;
        self.pending
            .resize(self.pending.len() + subbands.len(), 0.0);
        for (frame, input) in subbands.as_chunks::<BANDS>().0.iter().enumerate() {
            if frame.is_multiple_of(128) {
                checkpoint()?;
            }
            let start = self.advance + frame * BANDS;
            for (tap, weights) in self.kernels.0.as_chunks::<BANDS>().0.iter().enumerate() {
                let target = &mut self.pending[start + tap];
                for (&value, &weight) in input.iter().zip(weights) {
                    *target += value * weight;
                }
            }
        }
        self.advance += subbands.len();
        if end {
            self.advance += TAPS - 1;
        }
        let skip = self.skip.min(self.advance);
        let count = (self.advance - skip).saturating_sub(HALF_TAPS);
        let consumed = skip + count;
        let output = self.pending[skip..consumed].to_vec();
        self.pending.drain(..consumed);
        self.advance -= consumed;
        self.skip -= skip;
        self.finished = end;
        validate_output(&output)?;
        Ok(output)
    }
}

fn validate_input(finished: bool, values: &[f32]) -> Result<()> {
    if finished {
        return Err(VoiceError::new(
            "Vocoder filter stream has already finished",
        ));
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(VoiceError::new("Non-finite vocoder filter input"));
    }
    Ok(())
}

fn validate_output(values: &[f32]) -> Result<()> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(VoiceError::new("Vocoder filter output overflow"));
    }
    Ok(())
}

fn zeroes(length: usize) -> Result<Vec<f32>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| VoiceError::new("Could not allocate vocoder filter output"))?;
    values.resize(length, 0.0);
    Ok(values)
}
