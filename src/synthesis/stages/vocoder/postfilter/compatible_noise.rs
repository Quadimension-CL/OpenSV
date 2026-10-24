const MULTIPLIER: u64 = 0x5851_f42d_4c95_7f2d;
const FRAME_INCREMENT: u64 = 0xda3e_39cb_94b9_5bdb;
const NORMAL_INCREMENT: u64 = 0x7fff_fffd;
const FIRST_FRAME_STATE: u64 = 0x08f5_dc87_e5c0_7d8a;
use std::sync::Arc;
use crate::synthesis::voice::{Result,VoiceError};

#[derive(Clone)]
pub(super) struct CompatibleNoise {
    frame_state: u64,
    state: u64,
    rows_per_frame: usize,
    remaining: usize,
    seeds:Option<Arc<[u64]>>,
    frame:usize,
    queued:Option<std::collections::VecDeque<u64>>,
}

impl CompatibleNoise {
    pub(super) fn new(rows_per_frame: usize) -> Self {
        assert!(rows_per_frame > 0);
        Self { frame_state: FIRST_FRAME_STATE, state: 0, rows_per_frame, remaining: 0, seeds:None, frame:0, queued:None }
    }

    #[cfg(test)]
    pub(super) fn seeded(rows_per_frame:usize,seeds:Arc<[u64]>)->Self {
        let mut noise=Self::new(rows_per_frame);noise.seeds=Some(seeds);noise
    }
    pub(super) fn reset(&mut self) {
        let seeds=self.seeds.clone();let queued=self.queued.is_some();*self = Self::new(self.rows_per_frame);self.seeds=seeds;
        if queued {self.queued=Some(std::collections::VecDeque::new());}
    }
    pub(super) fn streamed(rows_per_frame:usize)->Self {
        let mut noise=Self::new(rows_per_frame);noise.queued=Some(std::collections::VecDeque::new());noise
    }
    pub(super) fn append(&mut self,seeds:&[u64])->Result<()> {
        let queue=self.queued.as_mut().ok_or_else(||VoiceError::new("Postfilter noise is not a streamed seed consumer"))?;
        queue.extend(seeds.iter().copied());Ok(())
    }

    pub(super) fn fill(&mut self, output: &mut [f32]) -> Result<()> {

        if output.len()!=8 {return Err(VoiceError::new("Postfilter PCG requires eight bands"));}
        if self.remaining == 0 {
            self.state = if let Some(queue)=&mut self.queued {
                queue.pop_front().ok_or_else(||VoiceError::new("Postfilter frame seed queue underflow"))?
            }else if let Some(seeds)=&self.seeds {
                *seeds.get(self.frame).ok_or_else(||VoiceError::new("Postfilter frame seed timeline ended early"))?
            }else{self.frame_state};
            self.frame_state = self.frame_state.wrapping_mul(MULTIPLIER).wrapping_add(FRAME_INCREMENT);
            self.frame+=1;
            self.remaining = self.rows_per_frame;
        }
        self.remaining -= 1;
        for pair in output.chunks_exact_mut(2) { pair.copy_from_slice(&self.pair()); }
        Ok(())
    }

    fn word(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(NORMAL_INCREMENT);
        (((old >> 45) ^ (old >> 27)) as u32).rotate_right((old >> 59) as u32)
    }

    fn pair(&mut self) -> [f32; 2] {
        let (mut x, mut y, mut radius) = loop {
            let first = self.word();
            let second = self.word();
            let y = symmetric(second);
            if y == 0.0 { continue; }
            let x = symmetric(first);
            let radius = x * x + y * y;
            if x != 0.0 && radius < 1.0 { break (x, y, radius); }
        };
        let logarithm = if radius <= 0.0001 {
            let exponent = ((x.abs().max(y.abs()).to_bits() >> 23) & 0xff) as i32 - 127;
            let scale = 2.0_f32.powi(-exponent);
            x *= scale;
            y *= scale;
            radius = x * x + y * y;
            exponent as f32 * f32::from_bits(0x3fb1_7218) + radius.ln()
        } else { radius.ln() };
        let factor = (logarithm * -2.0 / radius).sqrt();
        [x * factor + 0.0, y * factor + 0.0]
    }
}

fn symmetric(word: u32) -> f32 {
    let uniform = (word >> 8) as f32 * f32::from_bits(0x3380_0000);
    uniform + uniform - 1.0
}
