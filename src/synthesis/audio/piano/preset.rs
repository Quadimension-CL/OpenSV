use std::f64::consts::{PI, TAU};

pub(super) const PARTIALS: usize = 48;

#[derive(Clone, Copy, Default)]
pub(super) struct Partial {
    pub real: f32,
    pub imaginary: f32,
    pub rotation_real: f32,
    pub rotation_imaginary: f32,
}

pub(super) struct NotePreset {
    pub partials: [Partial; PARTIALS],
    pub count: usize,
    pub maximum_frames: u32,
    pub hammer_frames: u32,
}

impl NotePreset {
    pub fn new(note: u8, sample_rate: f64) -> Self {
        let frequency = 440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0);
        let decay_seconds = (4.8 * (220.0 / frequency).powf(0.32)).clamp(0.8, 9.0);
        let inharmonicity = (0.000_025 + 0.000_07 * (frequency / 440.0).powf(1.1)).min(0.003);
        let mut result = Self {
            partials: [Partial::default(); PARTIALS],
            count: 0,
            maximum_frames: (sample_rate * decay_seconds).ceil() as u32,
            hammer_frames: (sample_rate * 0.008).round().max(1.0) as u32,
        };
        let mut amplitude_sum = 0.0;
        for harmonic in 1..=24 {
            let harmonic = f64::from(harmonic);
            let amplitude = (0.7 + 0.3 * (PI * harmonic * 0.16).sin().abs()) / harmonic.powf(1.55);

            let lifetime = decay_seconds / (1.0 + 0.22 * (harmonic - 1.0).powf(1.15));
            let decay = (-6.907_755 / (sample_rate * lifetime)).exp();
            let stretched =
                frequency * harmonic * (1.0 + inharmonicity * harmonic * harmonic).sqrt();
            for cents in [-0.9, 0.9] {
                let frequency = stretched * 2.0_f64.powf(cents / 1200.0);

                if frequency >= sample_rate * 0.47 {
                    continue;
                }
                let phase = TAU * frequency / sample_rate;
                result.partials[result.count] = Partial {
                    real: amplitude as f32,
                    imaginary: 0.0,
                    rotation_real: (decay * phase.cos()) as f32,
                    rotation_imaginary: (decay * phase.sin()) as f32,
                };
                result.count += 1;
                amplitude_sum += amplitude;
            }
        }
        if amplitude_sum > 0.0 {
            let gain = (0.28 / amplitude_sum) as f32;
            for partial in &mut result.partials[..result.count] {
                partial.real *= gain;
            }
        }
        result
    }
}
