//! The postfilter's independent `minstd_rand` / polar-normal stream.
//!
//! Only the integer seed persists between calls. The cached second normal
//! is local to each call; unlike the acoustic
//! sampler, this generator is neither PCG nor a persistent normal cache.

#[derive(Clone)]
pub(super) struct PostfilterNoise {
    state: u32,
}

impl Default for PostfilterNoise {
    fn default() -> Self {
        Self { state: 1 }
    }
}

impl PostfilterNoise {
    /// Reset the deterministic random stream to its initial seed.
    pub(super) fn reset(&mut self) {
        self.state = 1;
    }

    /// Fill the noise portion of a process batch in frame/channel order.
    /// A final unconsumed pair member is discarded when this call returns,
    /// since normal-distribution state is local to the call. Eight bands
    /// make each frame even, so filling consecutive frames is identical
    /// to filling their concatenated batch.
    pub(super) fn fill(&mut self, output: &mut [f32]) {
        for chunk in output.chunks_mut(2) {
            let [x, y] = self.pair();
            chunk[0] = x;
            if let Some(sample) = chunk.get_mut(1) {
                *sample = y;
            }
        }
    }

    fn symmetric_uniform(&mut self) -> f32 {
        self.state = (u64::from(self.state) * 48_271 % 2_147_483_647) as u32;
        // Integer -> f32 precedes scaling by exactly 2^-31;
        // do not combine this in f64.
        let uniform = (self.state - 1) as f32 * f32::from_bits(0x3000_0000);
        uniform + uniform - 1.0
    }

    fn pair(&mut self) -> [f32; 2] {
        loop {
            let x = self.symmetric_uniform();
            let y = self.symmetric_uniform();
            let radius_squared = x * x + y * y;
            // A squared radius of exactly one is accepted.
            if radius_squared > 1.0 || radius_squared == 0.0 {
                continue;
            }
            let scale = ((radius_squared.ln() * -2.0) / radius_squared).sqrt();
            // Preserve pair order and normalize negative zero in each output.
            return [x * scale + 0.0, y * scale + 0.0];
        }
    }
}
