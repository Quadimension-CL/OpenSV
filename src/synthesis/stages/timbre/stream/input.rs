use super::super::{TimbreConditions, TimbreNetwork, records::AcousticRecords};
use super::features::Features;
use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

/// Full-owner conditions keep absolute noise and feature identities unchanged
/// while the acoustic operators consume their rows incrementally.
pub(super) struct Input {
    pub first_frame: usize,
    pub features: Features,
    pub normalized_pitch: Vec<f32>,
    pub impulse: Vec<f32>,
    pub pitch_hz: Vec<f32>,
    pub controls: Sequence,
    pub ppg_noise: Sequence,
    pub timbre_noise: Sequence,
    pub ppg_shape: Vec<f32>,
    pub ppg_intensity: Vec<f32>,
    pub timbre_shape: Vec<f32>,
    pub primary_style: Sequence,
    pub secondary_style: Sequence,
}

impl Input {
    pub fn from_conditions(
        network: &TimbreNetwork<'_>,
        input: TimbreConditions<'_>,
    ) -> Result<Self> {
        let frames = input.frame_features.frames;
        let low = frames / network.layout.stride;
        require(input.frame_features, frames, 128)?;
        require(input.controls, frames, 2)?;
        require(input.ppg_noise, low, network.layout.ppg_noise_channels)?;
        require(
            input.timbre_noise,
            low,
            network.layout.timbre_noise_channels,
        )?;
        if frames < network.layout.stride {
            return Err(VoiceError::new(
                "Timbre segment is shorter than its temporal stride",
            ));
        }
        for values in [input.normalized_pitch, input.phone_impulse, input.pitch_hz] {
            scalar(values, frames)?;
        }
        for values in [input.ppg_shape, input.ppg_intensity, input.timbre_shape] {
            scalar(values, low)?;
        }
        let style = |source: Option<&Sequence>| -> Result<Sequence> {
            if let Some(source) = source {
                require(source, low, network.styles.neutral.len())?;
                Ok(source.clone())
            } else {
                Sequence::new(
                    low,
                    network.styles.neutral.len(),
                    network.styles.neutral.repeat(low),
                )
            }
        };
        Ok(Self {
            first_frame: 0,
            features: Features::Dense(input.frame_features.clone()),
            normalized_pitch: input.normalized_pitch.to_vec(),
            impulse: input.phone_impulse.to_vec(),
            pitch_hz: input.pitch_hz.to_vec(),
            controls: input.controls.clone(),
            ppg_noise: input.ppg_noise.clone(),
            timbre_noise: input.timbre_noise.clone(),
            ppg_shape: input.ppg_shape.to_vec(),
            ppg_intensity: input.ppg_intensity.to_vec(),
            timbre_shape: input.timbre_shape.to_vec(),
            primary_style: style(input.primary_style)?,
            secondary_style: style(input.secondary_style)?,
        })
    }

    pub fn from_records(
        network: &TimbreNetwork<'_>,
        features: Features,
        records: &AcousticRecords,
        primary_style_weights: Option<&Sequence>,
        secondary_style_weights: Option<&Sequence>,
    ) -> Result<Self> {
        let frames = records.total_frames - records.first_frame;
        if features.frames() != frames
            || records.frames.len() != frames
            || records.stride != network.layout.stride
        {
            return Err(VoiceError::new(
                "Timbre stream records differ from their feature timeline",
            ));
        }
        let frontend = network.tree.node(network.layout.frontend)?;
        let normalizer = network.tree.range_normalizer(
            *frontend
                .children
                .get(3)
                .ok_or_else(|| VoiceError::new("Timbre frontend has no pitch normalizer"))?,
        )?;
        let normalized_pitch = records
            .frames
            .iter()
            .map(|f| {
                normalizer
                    .normalize_scalar(1, f.clamped_midi, true)
                    .map_err(|e| VoiceError::new(e.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        let impulse = records.frames.iter().map(|f| f.impulse).collect::<Vec<_>>();
        let pitch_hz = records.frames.iter().map(|f| f.raw_hz).collect::<Vec<_>>();
        let controls =
            Sequence::from_values(2, records.frames.iter().flat_map(|f| f.scalars).collect())?;
        let ppg_noise =
            Sequence::from_values(network.layout.ppg_noise_channels, records.ppg_noise.clone())?;
        let timbre_noise = Sequence::from_values(
            network.layout.timbre_noise_channels,
            records.timbre_noise.clone(),
        )?;
        let low = frames / network.layout.stride;
        if frames < network.layout.stride {
            return Err(VoiceError::new(
                "Timbre segment is shorter than its temporal stride",
            ));
        }
        if let Features::Dense(rows) = &features {
            require(rows, frames, 128)?;
        }
        for values in [&normalized_pitch, &impulse, &pitch_hz] {
            scalar(values, frames)?;
        }
        for values in [
            &records.ppg_shape,
            &records.ppg_intensity,
            &records.timbre_shape,
        ] {
            scalar(values, low)?;
        }
        require(&controls, frames, 2)?;
        require(&ppg_noise, low, network.layout.ppg_noise_channels)?;
        require(&timbre_noise, low, network.layout.timbre_noise_channels)?;
        let style = |weights| {
            network
                .styles
                .sample_frames(weights, records.total_frames, &records.low_rate_positions)
        };
        Ok(Self {
            first_frame: records.first_frame,
            features,
            normalized_pitch,
            impulse,
            pitch_hz,
            controls,
            ppg_noise,
            timbre_noise,
            ppg_shape: records.ppg_shape.clone(),
            ppg_intensity: records.ppg_intensity.clone(),
            timbre_shape: records.timbre_shape.clone(),
            primary_style: style(primary_style_weights)?,
            secondary_style: style(secondary_style_weights)?,
        })
    }
}

fn scalar(values: &[f32], count: usize) -> Result<()> {
    if values.len() != count || values.iter().any(|x| !x.is_finite()) {
        return Err(VoiceError::new("Invalid timbre scalar timeline"));
    }
    Ok(())
}

fn require(rows: &Sequence, frames: usize, channels: usize) -> Result<()> {
    rows.validate()?;
    if rows.frames != frames
        || rows.channels != channels
        || rows.values.iter().any(|x| !x.is_finite())
    {
        return Err(VoiceError::new("Invalid timbre condition shape or value"));
    }
    Ok(())
}
