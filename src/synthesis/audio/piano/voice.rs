use super::preset::{NotePreset, PARTIALS, Partial};

pub(super) struct Voice {
    partials: [Partial; PARTIALS],
    count: usize,
    age: u32,
    maximum_frames: u32,
    hammer_frames: u32,
    release_requested: bool,
    damper: f32,
    noise: u32,
    hammer: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            partials: [Partial::default(); PARTIALS],
            count: 0,
            age: 0,
            maximum_frames: 0,
            hammer_frames: 1,
            release_requested: false,
            damper: 1.0,
            noise: 0,
            hammer: 0.0,
        }
    }
}

impl Voice {
    pub fn strike(preset: &NotePreset, seed: u32) -> Self {
        Self {
            partials: preset.partials,
            count: preset.count,
            maximum_frames: preset.maximum_frames,
            hammer_frames: preset.hammer_frames,
            noise: seed ^ 0xa511_e9b3,
            ..Self::default()
        }
    }

    pub fn active(&self) -> bool {
        self.count > 0 && self.age < self.maximum_frames && self.damper >= 0.000_05
    }

    pub fn age(&self) -> u32 {
        self.age
    }

    pub fn release(&mut self) {
        self.release_requested = true;
    }

    pub fn prune_partials(&mut self) {
        let mut index = 0;
        while index < self.count {
            let partial = self.partials[index];
            if partial.real.abs() + partial.imaginary.abs() < 0.000_000_1 {
                self.count -= 1;
                self.partials[index] = self.partials[self.count];
            } else {
                index += 1;
            }
        }
    }

    pub fn sample(&mut self, attack_frames: u32, minimum_frames: u32, release_decay: f32) -> f32 {
        if !self.active() {
            return 0.0;
        }
        let mut sample = 0.0;
        for partial in &mut self.partials[..self.count] {
            let real = partial.real * partial.rotation_real
                - partial.imaginary * partial.rotation_imaginary;
            partial.imaginary = partial.real * partial.rotation_imaginary
                + partial.imaginary * partial.rotation_real;
            partial.real = real;
            sample += partial.imaginary;
        }
        if self.age < self.hammer_frames {
            self.noise = self
                .noise
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            let noise = (self.noise >> 8) as f32 / 8_388_608.0 - 1.0;
            self.hammer += 0.35 * (noise - self.hammer);
            let envelope = 1.0 - self.age as f32 / self.hammer_frames as f32;
            sample += self.hammer * envelope * envelope * 0.012;
        }
        if self.release_requested && self.age >= minimum_frames {
            self.damper *= release_decay;
        }
        let attack = (self.age as f32 / attack_frames as f32).min(1.0);
        self.age += 1;
        sample * attack * self.damper
    }
}
