use crate::synthesis::{
    inference::noise::{
        NormalNoise,
        seeds::{SeedSource, spread},
    },
    voice::{Result, VoiceError},
};

#[derive(Clone, Debug)]
pub struct PitchSession {
    pub temperature: f32,
    pub note_flag_temperature: f32,
    pub position_b_scale: f32,
    pub expressiveness_offset: f32,
    pub expressiveness_scale: f32,
    pub maximum_pitch: i32,
    pub transpose_up: bool,
    pub position_a: f32,
    pub seed: i32,
    pub strength: f32,
    pub style_weight: f32,
    pub control_weight: f32,
    pub spread_note_seeds: bool,
}

impl Default for PitchSession {
    fn default() -> Self {

        Self {
            temperature: 1.0,
            note_flag_temperature: 1.0,
            position_b_scale: 0.5,
            expressiveness_offset: 0.0,
            expressiveness_scale: 1.0,
            maximum_pitch: 79,
            transpose_up: false,
            position_a: f32::from_bits(0x3f7e_b852),
            seed: 0,
            strength: 1.0,
            style_weight: 3.0,
            control_weight: 2.0,
            spread_note_seeds: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoteModulation {
    pub frames: usize,
    pub transpose: f32,
    pub kind: u32,
    pub flag: bool,
    pub pitch_seed: Option<u32>,

    pub pitch_center: Option<f32>,
    pub pitch_motion: Option<f32>,
    pub expression: Option<f32>,
    pub position_c: Option<f32>,
    pub position_b: Option<f32>,
    pub accent: Option<f32>,
}

pub struct FrameModulation {
    pub transpose: Vec<f32>,
    pub noise: Vec<f32>,
    pub expressiveness: Vec<f32>,
    pub strength: Vec<f32>,
    pub pitch_center: Vec<f32>,
    pub pitch_motion: Vec<f32>,
    pub absolute_control: Vec<f32>,
    pub control_weight: Vec<f32>,
    pub position_a: Vec<f32>,
    pub position_b: Vec<f32>,
    pub position_c: Vec<f32>,
    pub style_weight: Vec<f32>,
    pub accent: Vec<f32>,
}

impl PitchSession {

    pub fn transposition(&self, pitches: impl IntoIterator<Item = f32>) -> Result<f32> {
        let mut maximum = 0_i32;
        for pitch in pitches {
            if !pitch.is_finite() || !(-1_000_000.0..=1_000_000.0).contains(&pitch) {
                return Err(VoiceError::new("Invalid pitch before F0 transposition"));
            }
            maximum = maximum.max(pitch.ceil() as i32);
        }
        let shift = self.maximum_pitch - maximum;
        Ok(if self.transpose_up {
            shift
        } else {
            shift.min(0)
        } as f32)
    }

    pub fn condition(
        &self,
        notes: &[NoteModulation],
        phone_tokens: &[String],
        phone_frames: &[usize],
        absolute_controls: &[f32],
        expressiveness: &[f32],
        mut normalize_pitch: impl FnMut(&[f32]) -> Result<Vec<f32>>,
    ) -> Result<FrameModulation> {
        self.validate()?;
        let frames = notes
            .iter()
            .try_fold(0_usize, |sum, note| sum.checked_add(note.frames))
            .ok_or_else(|| VoiceError::new("F0 control frame count overflow"))?;
        let phone_total = phone_frames
            .iter()
            .try_fold(0_usize, |sum, &length| sum.checked_add(length))
            .ok_or_else(|| VoiceError::new("F0 phone frame count overflow"))?;
        if absolute_controls.len() != frames
            || expressiveness.len() != frames
            || phone_total != frames
        {
            return Err(VoiceError::new(
                "F0 note, phone and control frame counts differ",
            ));
        }
        if absolute_controls.iter().any(|v| v.is_infinite())
            || expressiveness.iter().any(|v| !v.is_finite())
        {
            return Err(VoiceError::new("Invalid F0 control values"));
        }
        let mut output = FrameModulation {
            transpose: Vec::with_capacity(frames),
            noise: vec![0.0; frames],
            expressiveness: expressiveness
                .iter()
                .map(|v| {
                    (v.clamp(0.0, 2.0) - 1.0) * self.expressiveness_scale
                        + self.expressiveness_offset
                })
                .collect(),
            strength: vec![self.strength; frames],
            pitch_center: Vec::with_capacity(frames),
            pitch_motion: Vec::with_capacity(frames),
            absolute_control: vec![0.0; frames],
            control_weight: vec![0.0; frames],
            position_a: vec![self.position_a; frames],
            position_b: Vec::with_capacity(frames),
            position_c: Vec::with_capacity(frames),
            style_weight: vec![self.style_weight; frames],
            accent: Vec::with_capacity(frames),
        };
        let mut temperature = Vec::with_capacity(frames);
        for note in notes {
            let optional = [
                note.pitch_center,
                note.pitch_motion,
                note.expression,
                note.position_c,
                note.position_b,
                note.accent,
            ];
            if !note.transpose.is_finite() || optional.iter().flatten().any(|v| !v.is_finite()) {
                return Err(VoiceError::new("Invalid per-note F0 modulation"));
            }
            let flag_scale = if note.flag {
                self.note_flag_temperature
            } else {
                1.0
            };
            let accent = note.accent.unwrap_or(0.0);
            let scale = self.temperature * flag_scale;
            let local_temperature =
                (1.0 - accent) * note.expression.map_or(scale, |v| (v + v) * scale);
            temperature.extend(std::iter::repeat_n(local_temperature, note.frames));
            output
                .transpose
                .extend(std::iter::repeat_n(note.transpose, note.frames));
            output.pitch_center.extend(std::iter::repeat_n(
                note.pitch_center.unwrap_or(f32::NAN),
                note.frames,
            ));
            output.pitch_motion.extend(std::iter::repeat_n(
                note.pitch_motion.unwrap_or(f32::NAN),
                note.frames,
            ));
            output.position_b.extend(std::iter::repeat_n(
                note.position_b.unwrap_or(1.0) * self.position_b_scale,
                note.frames,
            ));
            output.position_c.extend(std::iter::repeat_n(
                note.position_c.unwrap_or(0.0),
                note.frames,
            ));
            output
                .accent
                .extend(std::iter::repeat_n(accent, note.frames));
        }
        smooth(&mut output.transpose);
        smooth(&mut temperature);
        smooth(&mut output.position_a);
        smooth(&mut output.position_c);
        let mut controls = Vec::with_capacity(frames);
        for (index, &value) in absolute_controls.iter().enumerate() {
            controls.push(if value.is_nan() {
                0.0
            } else {
                value + output.transpose[index]
            });
        }
        let normalized = normalize_pitch(&controls)?;
        if normalized.len() != frames || normalized.iter().any(|v| !v.is_finite()) {
            return Err(VoiceError::new(
                "F0 scalar normalizer returned invalid values",
            ));
        }
        for (index, &value) in absolute_controls.iter().enumerate() {
            if !value.is_nan() {
                output.absolute_control[index] = normalized[index];
                output.control_weight[index] = self.control_weight;
            }
        }
        let lengths: Vec<_> = notes.iter().map(|n| n.frames).collect();
        let seeds: Vec<_> = notes.iter().map(|n| n.pitch_seed.unwrap_or(0)).collect();
        let (seeds, lengths) = if self.spread_note_seeds {
            let mask: Vec<_> = notes
                .iter()
                .map(|n| n.kind == 1 && n.pitch_seed.is_none())
                .collect();
            spread(&seeds, &lengths, &mask, 100).map_err(|e| VoiceError::new(e.to_string()))?
        } else {
            (seeds, lengths)
        };
        let mut note_seed =
            SeedSource::new(&seeds, &lengths).map_err(|e| VoiceError::new(e.to_string()))?;

        let mut text_seed =
            SeedSource::context(phone_tokens, phone_frames, "maigca cruva!", self.seed)
                .map_err(|e| VoiceError::new(e.to_string()))?;
        let mut current_note = 0;
        let mut current_text = 0;
        let mut noise = NormalNoise::default();
        for (target, &scale) in output.noise.iter_mut().zip(&temperature) {
            let note_event = note_seed.next_event();
            let text_event = text_seed.next_event();
            if let Some(seed) = note_event {
                current_note = seed;
            }
            if let Some(seed) = text_event {
                current_text = seed;
            }
            if note_event.is_some() || text_event.is_some() {
                noise.reseed(current_note ^ current_text);
            }
            *target = noise.next_sample() * scale;
        }
        Ok(output)
    }

    fn validate(&self) -> Result<()> {
        let values = [
            self.temperature,
            self.note_flag_temperature,
            self.position_b_scale,
            self.expressiveness_offset,
            self.expressiveness_scale,
            self.position_a,
            self.strength,
            self.style_weight,
            self.control_weight,
        ];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(VoiceError::new("F0 session contains non-finite controls"));
        }
        Ok(())
    }
}

fn smooth(values: &mut [f32]) {
    for index in 1..values.len() {
        values[index] = values[index] * 0.95 + values[index - 1] * f32::from_bits(0x3d4c_ccd0);
    }
    for index in (0..values.len().saturating_sub(1)).rev() {
        values[index] = values[index] * 0.95 + values[index + 1] * f32::from_bits(0x3d4c_ccd0);
    }
}

pub fn finish_pitch(denormalized: &[f32], transpose: &[f32]) -> Result<Vec<f32>> {
    if denormalized.len() != transpose.len()
        || denormalized.iter().chain(transpose).any(|v| !v.is_finite())
    {
        return Err(VoiceError::new(
            "F0 output and transposition lengths differ or are invalid",
        ));
    }
    Ok(denormalized
        .iter()
        .zip(transpose)
        .map(|(&pitch, &offset)| (pitch - offset).clamp(0.0, 128.0))
        .collect())
}
