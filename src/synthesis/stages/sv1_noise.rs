const WORDS: usize = 624;
const OFFSET: usize = 397;

pub(crate) struct Normal {
    words: [u32; WORDS],
    index: usize,
    spare: Option<f32>,
}

impl Normal {
    pub(crate) fn new(seed: u32) -> Self {
        let mut result = Self {
            words: [0; WORDS],
            index: 0,
            spare: None,
        };
        result.reseed(seed);
        result
    }

    pub(crate) fn reseed(&mut self, seed: u32) {
        self.words[0] = seed;
        for index in 1..WORDS {
            let previous = self.words[index - 1];
            self.words[index] = (previous ^ (previous >> 30))
                .wrapping_mul(1_812_433_253)
                .wrapping_add(index as u32);
        }
        self.index = 0;

    }

    fn word(&mut self) -> u32 {
        let next = (self.index + 1) % WORDS;
        let combined = (self.words[self.index] & 0x8000_0000) | (self.words[next] & 0x7fff_ffff);
        let mut value = self.words[(self.index + OFFSET) % WORDS]
            ^ (combined >> 1)
            ^ if combined & 1 != 0 { 0x9908_b0df } else { 0 };
        self.words[self.index] = value;
        self.index = next;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c_5680;
        value ^= (value << 15) & 0xefc6_0000;
        value ^ (value >> 18)
    }

    pub(crate) fn next(&mut self) -> f32 {
        if let Some(value) = self.spare.take() {
            return value;
        }
        let (mut x, mut y, mut radius) = loop {
            let x = self.word() as f32 * f32::from_bits(0x3000_0000) - 1.0;
            let y = self.word() as f32 * f32::from_bits(0x3000_0000) - 1.0;
            let radius = x * x + y * y;
            if radius < 1.0 && x != 0.0 && y != 0.0 {
                break (x, y, radius);
            }
        };
        let log_radius = if radius <= 0.0001 {
            let exponent = ((x.abs().max(y.abs()).to_bits() >> 23) & 0xff) as i32 - 127;
            let scale = 2.0_f32.powi(-exponent);
            x *= scale;
            y *= scale;
            radius = x * x + y * y;
            exponent as f32 * f32::from_bits(0x3fb1_7218) + radius.ln()
        } else {
            radius.ln()
        };
        let factor = (-2.0 * log_radius / radius).sqrt();
        self.spare = Some(y * factor);
        x * factor
    }
}
