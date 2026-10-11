use super::{BLOCK, Clock, LOOKAHEAD, PcmStore, RATE};

#[derive(Clone, Debug)]
pub struct SegmentPlan {
    pub requested_seconds: f64,
    pub(super) clock: Clock,
    pub(super) first_sample: i64,
    pub(super) has_head: bool,
    pub(super) tail: Option<(i64, i64)>,
    pub(super) natural_guard: Option<i64>,
}

impl PcmStore {
    pub fn start_plan(&self, target_seconds: f64) -> Option<SegmentPlan> {
        let requested = (target_seconds * RATE).floor() as i64;
        let target_block = requested.div_euclid(BLOCK);
        let target = usize::try_from(target_block - self.first_block).ok();
        let first_index = target
            .filter(|&index| self.ready.get(index) == Some(&false))
            .or_else(|| self.ready.iter().position(|ready| !ready))?;
        let first = self.first_block + first_index as i64;
        let has_head =
            first != 0 && self.block_ready(first - 1) && (first - 1) * BLOCK >= self.clock.origin;
        let first_sample = if has_head {
            (first - 1) * BLOCK
        } else {
            first * BLOCK
        }
        .max(self.clock.origin);
        let mut stop_index = first_index + 1;
        let tail = loop {
            while stop_index < self.ready.len() && !self.ready[stop_index] {
                stop_index += 1;
            }
            if stop_index == self.ready.len() {
                break None;
            }
            let stop = self.first_block + stop_index as i64;
            // Carry an extra block after a head join, then match the
            // remaining three of the four lookahead blocks.
            let tail_begin = (stop + i64::from(has_head)) * BLOCK;
            let tail_end = (stop + 4) * BLOCK;
            if tail_end + LOOKAHEAD > self.clock.end {
                break None;
            }
            if (stop..stop + 4).all(|block| self.block_ready(block)) {
                break Some((tail_begin, tail_end));
            }
            // A shorter ready island cannot support a phase match. Regenerate
            // through it and join the next sufficiently long complete island.
            while stop_index < self.ready.len() && self.ready[stop_index] {
                stop_index += 1;
            }
        };
        let natural_guard = if tail.is_none() {
            self.ready[first_index..]
                .iter()
                .rposition(|ready| !ready)
                .map(|index| first_index + index)
                .filter(|&index| index + 1 < self.ready.len())
                .map(|index| ((self.first_block + index as i64) * BLOCK).max(self.clock.origin))
        } else {
            None
        };
        Some(SegmentPlan {
            requested_seconds: ((first - 1) * BLOCK).max(self.clock.origin) as f64 / RATE,
            clock: self.clock,
            first_sample,
            has_head,
            tail,
            natural_guard,
        })
    }
}
