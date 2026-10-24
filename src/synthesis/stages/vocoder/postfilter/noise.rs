#[derive(Clone)]
pub(super) struct PostfilterNoise {
    state: u32,
    compatible: Option<super::compatible_noise::CompatibleNoise>,
}

impl Default for PostfilterNoise {
    fn default() -> Self {
        Self { state: 1, compatible: None }
    }
}

impl PostfilterNoise {
    pub(super) fn set_streamed(&mut self, rows_per_frame:usize) {
        self.compatible=Some(super::compatible_noise::CompatibleNoise::streamed(rows_per_frame));
    }
    pub(super) fn append_frame_seeds(&mut self,seeds:&[u64])->crate::synthesis::voice::Result<()> {
        self.compatible.as_mut().ok_or_else(||crate::synthesis::voice::VoiceError::new("Postfilter seed mode is absent"))?.append(seeds)
    }
    pub(super) fn set_compatible(&mut self, rows_per_frame: usize) {
        self.compatible = Some(super::compatible_noise::CompatibleNoise::new(rows_per_frame));
    }

    pub(super) fn reset(&mut self) {
        self.state = 1;
        if let Some(noise) = &mut self.compatible { noise.reset(); }
    }

    pub(super) fn fill(&mut self, output: &mut [f32]) -> crate::synthesis::voice::Result<()> {
        if let Some(noise) = &mut self.compatible { return noise.fill(output); }
        for chunk in output.chunks_mut(2) {
            let [x, y] = self.pair();
            chunk[0] = x;
            if let Some(sample) = chunk.get_mut(1) {
                *sample = y;
            }
        }
        Ok(())
    }

    fn symmetric_uniform(&mut self) -> f32 {
        self.state = (u64::from(self.state) * 48_271 % 2_147_483_647) as u32;

        let uniform = (self.state - 1) as f32 * f32::from_bits(0x3000_0000);
        uniform + uniform - 1.0
    }

    fn pair(&mut self) -> [f32; 2] {
        loop {
            let x = self.symmetric_uniform();
            let y = self.symmetric_uniform();
            let radius_squared = x * x + y * y;

            if radius_squared > 1.0 || radius_squared == 0.0 {
                continue;
            }
            let scale = ((radius_squared.ln() * -2.0) / radius_squared).sqrt();

            return [x * scale + 0.0, y * scale + 0.0];
        }
    }
}
