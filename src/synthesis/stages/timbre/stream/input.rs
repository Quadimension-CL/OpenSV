use super::super::{TimbreConditions, TimbreNetwork, records::AcousticRecords};
use super::features::Features;
use crate::synthesis::{
    inference::Sequence,
    voice::{Result, VoiceError},
};

pub(super) struct Input {
    pub first_frame: usize,
    pub features: Features,
    pub normalized_pitch: Vec<f32>,
    pub refinement_pitch: Vec<f32>,
    pub impulse: Vec<f32>,
    pub pitch_hz: Vec<f32>,
    pub controls: Sequence,
    pub ppg_noise: Sequence,
    pub timbre_noise: Sequence,
    pub ppg_shape: Vec<f32>,
    pub ppg_intensity: Vec<f32>,
    pub timbre_intensity: Vec<f32>,
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
        for values in [input.ppg_shape, input.ppg_intensity, input.timbre_intensity] {
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
            refinement_pitch: refinement_pitch(network, input.pitch_hz.iter().map(|hz| hz.ln()))?,
            impulse: input.phone_impulse.to_vec(),
            pitch_hz: input.pitch_hz.to_vec(),
            controls: input.controls.clone(),
            ppg_noise: input.ppg_noise.clone(),
            timbre_noise: input.timbre_noise.clone(),
            ppg_shape: input.ppg_shape.to_vec(),
            ppg_intensity: input.ppg_intensity.to_vec(),
            timbre_intensity: input.timbre_intensity.to_vec(),
            primary_style: style(input.primary_style)?,
            secondary_style: style(input.secondary_style)?,
        })
    }

    pub fn from_records(
        network: &TimbreNetwork<'_>,
        features: Features,
        records: &AcousticRecords,
        styles: Option<&super::super::styles::AcousticStyles>,
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
        let frontend = crate::synthesis::stages::f0::frontend::NoteFrontend::prepare(
            network.tree,
            network.layout.frontend,
        )?;
        let normalizer = network.tree.range_normalizer(
            frontend.pitch_normalizer_node,
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
            &records.timbre_intensity,
        ] {
            scalar(values, low)?;
        }
        require(&controls, frames, 2)?;
        require(&ppg_noise, low, network.layout.ppg_noise_channels)?;
        require(&timbre_noise, low, network.layout.timbre_noise_channels)?;
        let neutral = Sequence::new(
            low,
            network.styles.neutral.len(),
            network.styles.neutral.repeat(low),
        )?;
        let (primary_style, secondary_style) = if let Some(styles) = styles {
            require(&styles.primary,low,network.styles.neutral.len())?;
            require(&styles.secondary,low,network.styles.neutral.len())?;
            (styles.primary.clone(),styles.secondary.clone())
        } else {
            (neutral.clone(), neutral)
        };
        Ok(Self {
            first_frame: records.first_frame,
            features,
            normalized_pitch,
            refinement_pitch: refinement_pitch(
                network,
                records.frames.iter().map(|frame| frame.clamped_log_hz),
            )?,
            impulse,
            pitch_hz,
            controls,
            ppg_noise,
            timbre_noise,
            ppg_shape: records.ppg_shape.clone(),
            ppg_intensity: records.ppg_intensity.clone(),
            timbre_intensity: records.timbre_intensity.clone(),
            primary_style,
            secondary_style,
        })
    }
}

fn refinement_pitch(
    network: &TimbreNetwork<'_>,
    values: impl Iterator<Item = f32>,
) -> Result<Vec<f32>> {
    if !matches!(
        network.layout.architecture,
        super::super::config::TimbreArchitecture::Sixteen
    ) {
        return Ok(Vec::new());
    }
    let normalizer = network
        .tree
        .range_normalizer(network.layout.log_pitch_normalizer)?;
    values
        .map(|value| {
            normalizer
                .normalize_scalar(0, value, true)
                .map_err(|error| VoiceError::new(error.to_string()))
        })
        .collect()
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
