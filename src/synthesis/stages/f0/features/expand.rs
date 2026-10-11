//! Duration expansion and raised-cosine positions.

use crate::synthesis::voice::{Result, VoiceError};

/// A borrowed row-major sequence; zero columns are allowed for empty features.
#[derive(Clone, Copy)]
pub struct FeatureRows<'a> {
    pub rows: usize,
    pub width: usize,
    pub values: &'a [f32],
}

#[derive(Clone, Copy)]
pub(super) struct ExpansionSpec {
    pub position_channels: usize,
    pub optional_channels: usize,
    pub condition_channels: usize,
}

pub struct ExpansionInput<'a> {
    /// Primary features gathered using the first-level frame mapping.
    pub first_primary: FeatureRows<'a>,
    /// Context rows that determine the number of first-level units.
    pub first_context: FeatureRows<'a>,
    /// Optional features with one row per first-level unit.
    pub optional: Option<FeatureRows<'a>>,
    pub first_durations: &'a [u32],
    /// An explicit position index for each frame, not an inferred local index.
    pub first_positions: &'a [u32],
    pub second: FeatureRows<'a>,
    pub second_durations: &'a [u32],
}

pub struct ExpandedFeatures {
    pub frames: usize,
    pub full_width: usize,
    /// [first_primary, first_context, first_position, second, second_position,
    /// optional, condition]. The condition region remains zero for the caller.
    pub full: Vec<f32>,
    pub second_width: usize,
    /// [second, second_position].
    pub second: Vec<f32>,
}

/// Build both expanded matrices, leaving the trailing condition region
/// for the caller to fill.
pub(super) fn expand(spec: ExpansionSpec, input: &ExpansionInput<'_>) -> Result<ExpandedFeatures> {
    validate_rows(input.first_primary)?;
    validate_rows(input.first_context)?;
    validate_rows(input.second)?;
    if input.first_durations.len() != input.first_context.rows
        || input.second_durations.len() != input.second.rows
        || input.first_primary.rows < input.first_context.rows
    {
        return Err(error("Feature rows do not match their duration sequence"));
    }
    if let Some(optional) = input.optional {
        validate_rows(optional)?;
        if optional.rows != input.first_context.rows || optional.width != spec.optional_channels {
            return Err(error("Optional frame features have an incompatible shape"));
        }
    }
    let frames = frame_count(input.second_durations)?;
    if frame_count(input.first_durations)? != frames || input.first_positions.len() != frames {
        return Err(error(
            "First-level and second-level expansions have different frame counts",
        ));
    }
    let second_width = checked_sum(&[input.second.width, spec.position_channels])?;
    let full_width = checked_sum(&[
        input.first_primary.width,
        input.first_context.width,
        spec.position_channels,
        second_width,
        spec.optional_channels,
        spec.condition_channels,
    ])?;
    let mut full = zero_matrix(frames, full_width)?;
    let mut second = zero_matrix(frames, second_width)?;

    // Iterate each unit to gather first-level features without allocating
    // an additional frame-to-unit lookup table.
    let mut frame = 0;
    for (unit, &duration) in input.first_durations.iter().enumerate() {
        for _ in 0..duration {
            let output = &mut full[frame * full_width..(frame + 1) * full_width];
            let mut column = copy_features(output, 0, input.first_primary, unit);
            column = copy_features(output, column, input.first_context, unit);
            fill_position(
                &mut output[column..column + spec.position_channels],
                input.first_positions[frame],
                duration,
            );
            column += spec.position_channels + second_width;
            if let Some(optional) = input.optional {
                copy_features(output, column, optional, unit);
            }
            frame += 1;
        }
    }
    let second_offset = checked_sum(&[
        input.first_primary.width,
        input.first_context.width,
        spec.position_channels,
    ])?;
    frame = 0;
    for (unit, &duration) in input.second_durations.iter().enumerate() {
        for position in 0..duration {
            let output = &mut second[frame * second_width..(frame + 1) * second_width];
            copy_features(output, 0, input.second, unit);
            fill_position(&mut output[input.second.width..], position, duration);
            let offset = frame * full_width + second_offset;
            full[offset..offset + second_width].copy_from_slice(output);
            frame += 1;
        }
    }
    Ok(ExpandedFeatures {
        frames,
        full_width,
        full,
        second_width,
        second,
    })
}

fn fill_position(output: &mut [f32], position: u32, duration: u32) {
    let relative = if duration > 1 {
        position as f32 / (duration - 1) as f32
    } else {
        0.5
    };
    let channels = output.len() as f32;
    for (channel, value) in output.iter_mut().enumerate() {
        let phase = (relative - (channel as f32 - 1.0) / channels) * std::f32::consts::TAU;
        *value = phase.cos() * 0.5 + 0.5;
    }
}

fn copy_features(output: &mut [f32], column: usize, source: FeatureRows<'_>, row: usize) -> usize {
    let end = column + source.width;
    output[column..end]
        .copy_from_slice(&source.values[row * source.width..(row + 1) * source.width]);
    end
}

fn validate_rows(sequence: FeatureRows<'_>) -> Result<()> {
    let expected = sequence
        .rows
        .checked_mul(sequence.width)
        .ok_or_else(|| error("Feature sequence dimensions overflow"))?;
    if sequence.values.len() != expected {
        return Err(error(
            "Feature sequence buffer does not match its dimensions",
        ));
    }
    Ok(())
}

fn frame_count(durations: &[u32]) -> Result<usize> {
    durations
        .iter()
        .try_fold(0_u32, |sum, &duration| sum.checked_add(duration))
        .map(|sum| sum as usize)
        .ok_or_else(|| error("Expanded frame count exceeds uint32 indexing"))
}

fn checked_sum(widths: &[usize]) -> Result<usize> {
    widths
        .iter()
        .try_fold(0_usize, |sum, &width| sum.checked_add(width))
        .ok_or_else(|| error("Expanded feature width overflows"))
}

fn zero_matrix(rows: usize, width: usize) -> Result<Vec<f32>> {
    let count = rows
        .checked_mul(width)
        .ok_or_else(|| error("Expanded feature matrix dimensions overflow"))?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| error("Could not allocate expanded feature matrix"))?;
    values.resize(count, 0.0);
    Ok(values)
}

fn error(message: &'static str) -> VoiceError {
    VoiceError::new(message)
}
