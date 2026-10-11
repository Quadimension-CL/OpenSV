use super::{
    Contract, Sv1TimbreNetwork,
    frontend::ContextInput,
    pitch,
    records::{self, Records},
};
use crate::synthesis::{
    inference::Sequence,
    stages::{f0::PitchResult, timbre::TimbreRequest},
    voice::{Result, VoiceError},
};

/// Owner-wide conditioning shared by all preview cursors. Temporal phone
/// projection retains the complete owner context before any seek is applied.
pub struct PreparedSv1TimbreFrames {
    pub(super) contract: Contract,
    pub(super) expanded: Sequence,
    pub(super) records: Records,
    pub(super) phone_frames: Vec<usize>,
    pub(super) noise_ids: Vec<i32>,
}

impl PreparedSv1TimbreFrames {
    pub(super) fn prepare(
        network: &Sv1TimbreNetwork<'_>,
        request: &TimbreRequest<'_>,
        pitch: &PitchResult,
    ) -> Result<Self> {
        let phone_frames: Vec<_> = request.phones.iter().map(|phone| phone.frames).collect();
        let frames = phone_frames
            .iter()
            .try_fold(0_usize, |total, &count| total.checked_add(count))
            .ok_or_else(|| VoiceError::new("DDS acoustic phone timeline overflows"))?;
        let log_hz = pitch::sample_log_hz(
            pitch,
            frames,
            network.frontend.frame_seconds,
            network.contract,
        )?;
        let records = records::prepare(request, network.contract, log_hz)?;
        let linguistic = network.frontend.linguistic(request.phones)?;
        let normalized_pitch = network.frontend.normalize_pitch(&records.log_hz)?;
        let expanded = network.context.prepare_frames(&ContextInput {
            phones: request.phones,
            linguistic: &linguistic,
            normalized_pitch: &normalized_pitch,
            bank: &network.bank,
            style: &network.styles.neutral,
            phone_factors: None,
        })?;
        expanded.validate()?;
        if expanded.frames != frames
            || records.log_hz.len() != frames
            || linguistic.noise_ids.len() != phone_frames.len()
        {
            return Err(VoiceError::new(
                "Compatible prepared frame and phone timelines differ",
            ));
        }
        Ok(Self {
            contract: network.contract,
            expanded,
            records,
            phone_frames,
            noise_ids: linguistic.noise_ids,
        })
    }

    pub fn len(&self) -> usize {
        self.expanded.frames
    }

    pub fn is_empty(&self) -> bool {
        self.expanded.frames == 0
    }

    /// Start at a phone boundary with 512 warmup frames for both SV1 and
    /// SV2-compatible models.
    /// An exact boundary retains the preceding phone, including its full state
    /// preparation. Noise is restarted from this phone's seed queue; it is not
    /// a slice of a previously generated owner-wide noise sequence.
    pub(super) fn first_phone(&self, requested_frame: usize) -> Result<(usize, usize)> {
        if requested_frame > self.len() {
            return Err(VoiceError::new(
                "Compatible acoustic seek is outside the prepared timeline",
            ));
        }
        let mut remaining = requested_frame.saturating_sub(512);
        let mut first_phone = 0;
        let mut first_frame = 0;
        while remaining > 0 {
            let &count = self
                .phone_frames
                .get(first_phone)
                .ok_or_else(|| VoiceError::new("Compatible acoustic seek has no matching phone"))?;
            if count >= remaining {
                break;
            }
            remaining -= count;
            first_frame += count;
            first_phone += 1;
        }
        Ok((first_phone, first_frame))
    }
}
