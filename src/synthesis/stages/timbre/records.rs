mod noise;

use crate::synthesis::voice::{Result, VoiceError};

#[derive(Clone, Copy, Debug)]
pub struct PhoneImpulse {

    pub value: f64,

    pub position: f64,
}

pub struct AcousticPhone<'a> {
    pub token: &'a str,
    pub frames: usize,
    pub impulse: Option<PhoneImpulse>,
}

#[derive(Clone, Copy, Debug)]
pub struct AcousticNote {

    pub frames: usize,

    pub timbre_seed: Option<u32>,

    pub kind: u32,

    pub factor: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameControl {
    pub pitch_ratio: f32,

    pub scalars: [f32; 2],
}

impl Default for FrameControl {
    fn default() -> Self {
        Self {
            pitch_ratio: 1.0,
            scalars: [0.0; 2],
        }
    }
}

pub struct RecordsInput<'a> {
    pub phones: &'a [AcousticPhone<'a>],
    pub notes: &'a [AcousticNote],

    pub midi_pitch: &'a [f32],
    pub controls: Option<&'a [FrameControl]>,
    pub requested_frame: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct RecordsConfig {
    pub stride: usize,
    pub ppg_width: usize,
    pub timbre_width: usize,
    pub ppg_noise_scale: f32,
    pub timbre_noise_scale: f32,
    pub ppg_shape: f32,
    pub timbre_shape: f32,
    pub ppg_intensity: f32,
    pub timbre_intensity: f32,
    pub minimum_midi: f32,
    pub maximum_midi: f32,
    pub spread_note_seeds: bool,
    pub retained_noise_only: bool,
}

impl Default for RecordsConfig {
    fn default() -> Self {
        Self {
            stride: 4,
            ppg_width: 16,
            timbre_width: 16,
            ppg_noise_scale: 1.0,
            timbre_noise_scale: 1.0,
            ppg_shape: f32::from_bits(0x3f7e_b852),
            timbre_shape: f32::from_bits(0x3f7e_b852),
            ppg_intensity: 0.2,
            timbre_intensity: 0.2,
            minimum_midi: 0.0,
            maximum_midi: 127.0,
            spread_note_seeds: false,
            retained_noise_only: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AcousticFrame {
    pub absolute_frame: usize,
    pub raw_midi: f32,
    pub raw_hz: f32,
    pub midi: f32,
    pub hz: f32,

    pub clamped_midi: f32,
    pub clamped_log_hz: f32,
    pub scalars: [f32; 2],
    pub impulse: f32,
    pub note_factor: f32,
}

pub struct AcousticRecords {
    pub first_frame: usize,
    pub total_frames: usize,
    pub stride: usize,
    pub frames: Vec<AcousticFrame>,

    pub low_rate_positions: Vec<usize>,
    pub ppg_noise: Vec<f32>,
    pub timbre_noise: Vec<f32>,
    pub ppg_shape: Vec<f32>,
    pub ppg_intensity: Vec<f32>,
    pub timbre_shape: Vec<f32>,
    pub timbre_intensity: Vec<f32>,
}

impl RecordsConfig {
    pub fn prepare(&self, input: RecordsInput<'_>) -> Result<AcousticRecords> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("timbre.records.prepare");
        self.validate()?;
        let total = total_frames(input.phones.iter().map(|p| p.frames))?;
        if total_frames(input.notes.iter().map(|n| n.frames))? != total
            || input.midi_pitch.len() != total
            || input.controls.is_some_and(|values| values.len() != total)
            || input.requested_frame > total
        {
            return Err(VoiceError::new(
                "Acoustic phone, note and pitch timelines differ",
            ));
        }
        let first_frame = input.requested_frame.saturating_sub(32) / self.stride * self.stride;

        if total - first_frame < self.stride {
            return Err(VoiceError::new(
                "Acoustic preparation requires a complete stride",
            ));
        }
        let mut factors = Vec::with_capacity(total);
        for note in input.notes {
            let factor = note.factor.filter(|v| !v.is_nan()).unwrap_or(1.0);
            if !factor.is_finite() {
                return Err(VoiceError::new(
                    "Acoustic note factor must be finite or unset",
                ));
            }
            factors.extend(std::iter::repeat_n(factor, note.frames));
        }
        let impulses = phone_impulses(input.phones, total)?;
        let mut frames = Vec::with_capacity(total - first_frame);
        let lower_log = midi_hz(self.minimum_midi).ln();
        let upper_log = midi_hz(self.maximum_midi).ln();
        for absolute_frame in first_frame..total {
            let raw_midi = input.midi_pitch[absolute_frame];
            let control = input
                .controls
                .map_or_else(FrameControl::default, |v| v[absolute_frame]);
            if !raw_midi.is_finite()
                || !control.pitch_ratio.is_finite()
                || control.pitch_ratio <= 0.0
            {
                return Err(VoiceError::new(
                    "Acoustic pitch and pitch ratio must be finite and valid",
                ));
            }
            let raw_hz = midi_hz(raw_midi);
            let hz = raw_hz * control.pitch_ratio;
            let midi = if control.pitch_ratio == 1.0 {
                raw_midi
            } else {
                (12.0 * (f64::from(hz) / 440.0).log2() + 69.0) as f32
            };
            let scalars = control.scalars.map(|v| if v.is_nan() { 0.0 } else { v });
            if !hz.is_finite()
                || hz <= 0.0
                || !midi.is_finite()
                || scalars.iter().any(|v| !v.is_finite())
            {
                return Err(VoiceError::new(
                    "Acoustic frame contains invalid pitch or controls",
                ));
            }
            frames.push(AcousticFrame {
                absolute_frame,
                raw_midi,
                raw_hz,
                midi,
                hz,
                clamped_midi: midi.clamp(self.minimum_midi, self.maximum_midi),
                clamped_log_hz: hz.ln().clamp(lower_log, upper_log),
                scalars,
                impulse: impulses[absolute_frame],
                note_factor: factors[absolute_frame],
            });
        }
        let low_rate_positions: Vec<_> = (first_frame..total)
            .filter(|frame| frame % self.stride == self.stride - 1)
            .collect();
        let ppg_noise = noise::generate(
            self,
            &input,
            first_frame,
            self.ppg_width,
            self.ppg_noise_scale,
            "pingpongpingpong",
        )?;
        let timbre_noise = noise::generate(
            self,
            &input,
            first_frame,
            self.timbre_width,
            self.timbre_noise_scale,
            "non terrae plus ultra",
        )?;
        let count = low_rate_positions.len();
        let ppg_intensity = low_rate_positions
            .iter()
            .map(|&i| self.ppg_intensity * factors[i])
            .collect();
        let timbre_intensity = low_rate_positions
            .iter()
            .map(|&i| self.timbre_intensity * factors[i])
            .collect();
        Ok(AcousticRecords {
            first_frame,
            total_frames: total,
            stride: self.stride,
            frames,
            low_rate_positions,
            ppg_noise,
            timbre_noise,
            ppg_shape: vec![self.ppg_shape; count],
            ppg_intensity,
            timbre_shape: vec![self.timbre_shape; count],
            timbre_intensity,
        })
    }

    fn validate(&self) -> Result<()> {
        if self.stride == 0
            || self.ppg_width == 0
            || self.timbre_width == 0
            || [
                self.ppg_noise_scale,
                self.timbre_noise_scale,
                self.ppg_shape,
                self.timbre_shape,
                self.ppg_intensity,
                self.timbre_intensity,
                self.minimum_midi,
                self.maximum_midi,
            ]
            .iter()
            .any(|v| !v.is_finite())
            || self.minimum_midi > self.maximum_midi
        {
            return Err(VoiceError::new("Invalid acoustic record configuration"));
        }
        Ok(())
    }
}

fn total_frames(mut lengths: impl Iterator<Item = usize>) -> Result<usize> {
    lengths
        .try_fold(0_usize, |sum, length| sum.checked_add(length))
        .filter(|&sum| sum <= i32::MAX as usize)
        .ok_or_else(|| VoiceError::new("Acoustic frame timeline exceeds signed 32-bit range"))
}

fn midi_hz(midi: f32) -> f32 {
    (440.0 * ((f64::from(midi) - 69.0) / 12.0).exp2()) as f32
}

fn phone_impulses(phones: &[AcousticPhone<'_>], total: usize) -> Result<Vec<f32>> {
    let mut values = vec![0.0; total];
    let mut start = 0;
    for phone in phones {
        if let Some(impulse) = phone.impulse {
            if !impulse.value.is_finite()
                || !impulse.position.is_finite()
                || !(0.0..=1.0).contains(&impulse.position)
                || !(impulse.value as f32).is_finite()
            {
                return Err(VoiceError::new("Invalid acoustic phone impulse"));
            }
            if phone.frames > 0 {
                let offset = ((phone.frames as f64 * impulse.position).round() as usize)
                    .min(phone.frames - 1);
                values[start + offset] = impulse.value as f32;
            }
        }
        start += phone.frames;
    }
    Ok(values)
}
