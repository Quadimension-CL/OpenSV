pub mod seeds;

const MULTIPLIER: u64 = 0x5851_f42d_4c95_7f2d;
const INCREMENT: u64 = 0xda3e_39cb_94b9_5bdb;
const INITIAL_STATE: u64 = 0x853c_49e6_748f_ea9b;
const RESET_OFFSET: u64 = 0x08f5_dc87_e5c0_7d8a;

#[derive(Clone, Debug)]
pub struct NormalNoise {
    state: u64,
    spare: Option<f32>,
    reverse_pair: bool,
}

impl Default for NormalNoise {
    fn default() -> Self {
        Self {
            state: INITIAL_STATE,
            spare: None,
            reverse_pair: false,
        }
    }
}

impl NormalNoise {

    pub fn reseed(&mut self, seed: u32) {
        self.state = u64::from(seed)
            .wrapping_mul(MULTIPLIER)
            .wrapping_add(RESET_OFFSET);
        self.spare = None;
    }

    pub fn set_reverse_pair(&mut self, reverse: bool) {
        self.reverse_pair = reverse;
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        let word = ((old >> 45) ^ (old >> 27)) as u32;
        word.rotate_right((old >> 59) as u32)
    }

    pub fn next_sample(&mut self) -> f32 {
        if let Some(value) = self.spare.take() {
            return value;
        }
        let (mut x, mut y, mut radius_squared) = loop {

            let first = self.next_u32();
            let second = self.next_u32();
            let y = uniform_signed(second);
            if y == 0.0 {
                continue;
            }
            let x = uniform_signed(first);
            let radius_squared = x * x + y * y;
            if x != 0.0 && radius_squared < 1.0 {
                break (x, y, radius_squared);
            }
        };
        let log_radius = if radius_squared <= 0.0001 {

            let largest = x.abs().max(y.abs());
            let exponent = ((largest.to_bits() >> 23) & 0xff) as i32 - 127;
            let scale = 2.0_f32.powi(-exponent);
            x *= scale;
            y *= scale;
            radius_squared = x * x + y * y;
            exponent as f32 * f32::from_bits(0x3fb1_7218) + radius_squared.ln()
        } else {
            radius_squared.ln()
        };
        let factor = (log_radius * -2.0 / radius_squared).sqrt();
        let (sample, spare) = if self.reverse_pair { (y, x) } else { (x, y) };
        self.spare = Some(spare * factor);
        factor * sample
    }

    pub fn frame(&mut self, output: &mut [f32], scale: f32) {
        for value in output {
            *value = self.next_sample() * scale;
        }
    }
}

fn uniform_signed(word: u32) -> f32 {
    let value = word as f32 * f32::from_bits(0x2f80_0000);
    value + value - 1.0
}
