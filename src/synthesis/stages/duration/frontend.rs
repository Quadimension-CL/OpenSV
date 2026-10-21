//! Explicit-phone duration frontend and frame allocation. All note times
//! here are seconds.

use serde::{Deserialize, Serialize};

use crate::synthesis::{
    inference::ops::softmax,
    voice::{PreparedTree, Result, VoiceError},
};

const FRONTEND_TYPE: [u8; 16] = [
    0x94, 0xe2, 0xba, 0x14, 0x8e, 0xe1, 0xf6, 0x8d, 0x07, 0x43, 0x4d, 0xcb, 0xf5, 0xa4, 0x04, 0xab,
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExplicitPhone {
    pub language: String,
    pub token: String,
    #[serde(default)]
    pub source: Option<crate::model::SourcePhone>,
    #[serde(default)]
    pub attributes: crate::model::PhonemeAttributes,
    /// Global seconds of the source note onset, before context padding.
    #[serde(default)]
    pub source_note_onset_seconds: Option<f64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct DurationNote {
    /// Sample rate from the engine configuration.
    pub sample_rate: u32,
    /// Frame hop in samples from the engine configuration.
    pub hop_samples: f64,
    /// Note duration before any record subdivision.
    pub duration_seconds: f64,
    pub pitch: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DurationRecord {
    pub phones: Vec<ExplicitPhone>,
    pub note: DurationNote,
    /// Durations of the linked notes.
    pub linked_durations_seconds: Vec<f64>,
    /// Number of equal temporal partitions and the selected partition index.
    pub parts: u32,
    pub part_index: u32,
    pub pitch_offset: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct PhoneFeatures {
    pub unified_phone_index: usize,
    pub language_index: usize,
    pub left_index: usize,
    pub right_index: usize,
    pub pitch: f32,
    pub duration: f32,
    pub record_index: usize,
    pub used_silence_fallback: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PhoneDuration {
    pub record_index: usize,
    pub phone_index: usize,
    /// SV1 retains continuous allocation seconds; SV2 adapters retain
    /// their frame-rounded allocation.
    pub duration_seconds: f64,
    /// Authoritative acoustic frame count. DDS F0 derives a separate rounded
    /// clock from duration_seconds; SV2 uses this corrected count.
    pub frames: usize,
}

pub(crate) struct PhoneAllocations {
    pub default_durations: Vec<PhoneDuration>,
    pub durations: Vec<PhoneDuration>,
}

impl DurationRecord {
    fn timing(&self) -> Result<(f64, f64)> {
        if self.note.sample_rate == 0
            || !self.note.hop_samples.is_finite()
            || self.note.hop_samples <= 0.0
            || !self.note.duration_seconds.is_finite()
            || self.note.duration_seconds < 0.0
            || self.parts == 0
            || self.part_index >= self.parts
            || self
                .linked_durations_seconds
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err(VoiceError::new("Invalid duration frontend record timing"));
        }
        let step = self.note.hop_samples / f64::from(self.note.sample_rate);
        let part = f64::from(self.part_index);
        // Preserve multiplication/division order to keep boundary rounding stable.
        let start =
            rounded_frames(self.note.duration_seconds * (part / f64::from(self.parts)) / step);
        let end = rounded_frames(
            self.note.duration_seconds * ((part + 1.0) / f64::from(self.parts)) / step,
        );
        let seconds = self
            .linked_durations_seconds
            .iter()
            .fold((end - start) * step, |sum, v| sum + v);
        if !seconds.is_finite() || seconds / step > i32::MAX as f64 {
            return Err(VoiceError::new(
                "Duration frontend record exceeds supported frame range",
            ));
        }
        Ok((step, seconds))
    }
}

fn frontend_node(tree: &PreparedTree, expected_type: [u8; 16]) -> Result<usize> {
    let root = tree.node(tree.root_index())?;
    let index = *root
        .children
        .first()
        .ok_or_else(|| VoiceError::new("Duration tree lacks its note frontend"))?;
    if tree.node(index)?.type_id != expected_type {
        return Err(VoiceError::new(
            "Duration root child 0 is not its typed note frontend",
        ));
    }
    Ok(index)
}

pub fn prepare_explicit(
    tree: &PreparedTree,
    records: &[DurationRecord],
) -> Result<Vec<PhoneFeatures>> {
    prepare_explicit_typed(tree, records, FRONTEND_TYPE)
}

/// Explicit-phone feature contract shared by model versions 1 and 2.
pub(crate) fn prepare_explicit_typed(
    tree: &PreparedTree,
    records: &[DurationRecord],
    expected_type: [u8; 16],
) -> Result<Vec<PhoneFeatures>> {
    let frontend = frontend_node(tree, expected_type)?;
    let node = tree.node(frontend)?;
    if node.children.len() != 4 {
        return Err(VoiceError::new("Duration frontend requires four children"));
    }
    let payload = tree.payload(frontend)?;
    if payload.len() != 8 {
        return Err(VoiceError::new(
            "Duration frontend requires two pitch bounds",
        ));
    }
    let low = i32::from_le_bytes(payload[..4].try_into().unwrap());
    let high = i32::from_le_bytes(payload[4..].try_into().unwrap());
    if high <= low {
        return Err(VoiceError::new(
            "Duration frontend pitch bounds are not increasing",
        ));
    }
    let mut output = Vec::new();
    for (record_index, record) in records.iter().enumerate() {
        let (_, seconds) = record.timing()?;
        if record.phones.is_empty() {
            continue;
        }
        let pitch =
            ((record.note.pitch + record.pitch_offset) - low as f32) / (high as f32 - low as f32);
        if !pitch.is_finite() {
            return Err(VoiceError::new("Non-finite duration frontend pitch"));
        }
        let normalizer = tree.range_normalizer(node.children[2])?;
        if normalizer.channels() != 1 {
            return Err(VoiceError::new(
                "Duration frontend normalizer must have one channel",
            ));
        }
        let duration = normalizer
            .normalize_scalar(0, (seconds + 0.01).ln() as f32, true)
            .map_err(|error| VoiceError::new(error.to_string()))?;
        for (left_index, phone) in record.phones.iter().enumerate() {
            let resolved = tree.resolve_phone(&phone.language, &phone.token)?;
            output.push(PhoneFeatures {
                unified_phone_index: resolved.unified_phone_index,
                language_index: resolved.language_index,
                left_index,
                right_index: record.phones.len() - left_index - 1,
                pitch,
                duration,
                record_index,
                used_silence_fallback: resolved.used_silence_fallback,
            });
        }
    }
    Ok(output)
}

/// Restore seconds and frame boundaries from the duration network's one-scalar
/// output. This operates on the supplied records in their chronological order.
pub(crate) fn postprocess(
    tree: &PreparedTree,
    records: &[DurationRecord],
    raw_scalar: &[f32],
    phrase_start_seconds: f64,
) -> Result<PhoneAllocations> {
    postprocess_typed(
        tree,
        records,
        raw_scalar,
        FRONTEND_TYPE,
        phrase_start_seconds,
    )
}

/// Both model versions use inverse normalization, per-record softmax,
/// rounded boundaries and phoneme onset adjustments.
pub(crate) fn postprocess_typed(
    tree: &PreparedTree,
    records: &[DurationRecord],
    raw_scalar: &[f32],
    expected_type: [u8; 16],
    phrase_start_seconds: f64,
) -> Result<PhoneAllocations> {
    let frontend = tree.node(frontend_node(tree, expected_type)?)?;
    if frontend.children.len() != 4 {
        return Err(VoiceError::new("Duration frontend requires four children"));
    }
    let count = records
        .iter()
        .try_fold(0_usize, |sum, record| sum.checked_add(record.phones.len()))
        .ok_or_else(|| VoiceError::new("Duration phone count overflows"))?;
    if raw_scalar.len() != count || raw_scalar.iter().any(|v| !v.is_finite()) {
        return Err(VoiceError::new(
            "Duration scalar output does not match phone records",
        ));
    }
    if records.is_empty() {
        return Ok(PhoneAllocations {
            default_durations: Vec::new(),
            durations: Vec::new(),
        });
    }
    let normalizer = tree.range_normalizer(frontend.children[3])?;
    if normalizer.channels() != 1 {
        return Err(VoiceError::new(
            "Duration output normalizer must have one channel",
        ));
    }
    let mut transformed = vec![0.0; raw_scalar.len()];
    normalizer
        .denormalize(raw_scalar, &mut transformed)
        .map_err(|error| VoiceError::new(error.to_string()))?;
    if transformed.len() != count || transformed.iter().any(|v| !v.is_finite()) {
        return Err(VoiceError::new(
            "Duration output normalization returned invalid scalars",
        ));
    }
    let mut output = Vec::with_capacity(count);
    let mut cursor = 0;
    for (record_index, record) in records.iter().enumerate() {
        let (step, seconds) = record.timing()?;
        let scalars = &transformed[cursor..cursor + record.phones.len()];
        cursor += record.phones.len();
        if scalars.is_empty() {
            continue;
        }
        let total_frames = seconds / step;
        let mut weights = scalars.to_vec();
        softmax::execute(&mut weights).map_err(|error| VoiceError::new(error.to_string()))?;
        let mut accumulated = 0.0;
        let mut previous = 0_usize;
        for (phone_index, weight) in weights.iter().enumerate() {
            accumulated += rounded_frames(total_frames) * f64::from(*weight);
            let end = if phone_index + 1 == weights.len() {
                rounded_frames(total_frames)
            } else {
                rounded_frames(accumulated)
            };
            let end = end as usize;
            let frames = end
                .checked_sub(previous)
                .ok_or_else(|| VoiceError::new("Duration allocation has decreasing boundaries"))?;
            previous = end;
            output.push(PhoneDuration {
                record_index,
                phone_index,
                duration_seconds: frames as f64 * step,
                frames,
            });
        }
    }
    let (step, _) = records[0].timing()?;
    super::controls::allocate_boundaries(records, output, step, phrase_start_seconds)
}

pub(super) fn rounded_frames(value: f64) -> f64 {
    // Add a sign-matched bias and truncate; replacing this with round()
    // would change the tie rule.
    (value + f64::from_bits(0x3fdf_ffff_ffff_ffff).copysign(value)).trunc()
}
