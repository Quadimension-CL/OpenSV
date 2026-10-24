use super::{
    BLOCK, LOOKAHEAD, PREVIEW_BLOCK_SAMPLES, PREVIEW_SAMPLE_RATE, PcmComponents, PcmStore,
    RenderedAudio, Result, SegmentPlan, VoiceError, buffer::Buffer,
};
use crate::synthesis::rendered::patch::{splice_head, splice_tail};

pub struct Segment {
    plan: SegmentPlan,
    buffer: Buffer,
    next_sample: i64,
    shift: Option<i32>,
    finished: bool,
}

impl Segment {
    pub(super) fn new(plan: SegmentPlan) -> Self {
        Self {
            next_sample: plan.first_sample,
            shift: (!plan.has_head).then_some(0),
            plan,
            buffer: Buffer::default(),
            finished: false,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn covers_demand(&self, seconds: f64) -> bool {
        if self.finished || !seconds.is_finite() {
            return false;
        }
        let sample = (seconds * super::RATE).floor() as i64;
        let end = self.plan.tail.map_or(self.plan.clock.end, |(_, end)| end);
        (self.next_sample..end).contains(&sample)
    }
}

impl PcmStore {
    pub fn push(
        &mut self,
        segment: &mut Segment,
        fresh: &PcmComponents,
        natural_final: bool,
    ) -> Result<Vec<RenderedAudio>> {
        if segment.finished || segment.plan.clock != self.clock {
            return Err(VoiceError::new(
                "Progressive PCM segment is finished or has another clock",
            ));
        }
        if segment.buffer.received_end().is_none()
            && !segment.plan.has_head
            && (fresh.start_seconds * super::RATE - self.clock.origin as f64).abs() < 0.0001
            && (self.first_block..segment.plan.first_sample.div_euclid(BLOCK))
                .all(|block| !self.block_ready(block))
        {

            segment.plan.first_sample = self.clock.origin;
            segment.next_sample = self.clock.origin;
        }
        segment.buffer.append(fresh)?;
        if fresh.primary.is_empty() && !natural_final {
            return Ok(Vec::new());
        }
        if segment
            .buffer
            .received_end()
            .is_some_and(|end| end > self.clock.end)
        {
            return Err(VoiceError::new(
                "Vocoder PCM extends beyond the complete model clock",
            ));
        }
        if natural_final {
            let padding_end =
                (self.clock.end + LOOKAHEAD).max(segment.plan.first_sample + BLOCK * 2);
            segment
                .buffer
                .natural_padding(self.clock.end, padding_end)?;
        }
        let mut updates = Vec::new();
        if segment.shift.is_none() && !self.join_head(segment, &mut updates)? {
            return Ok(updates);
        }
        let shift = i64::from(segment.shift.unwrap_or(0));
        let body_end = segment.plan.tail.map_or_else(
            || {

                if natural_final {
                    self.clock.end
                } else {
                    segment.plan.natural_guard.unwrap_or(self.clock.end)
                }
            },
            |(begin, _)| {

                (begin - BLOCK * (1 + i64::from(segment.plan.has_head))).max(self.clock.origin)
            },
        );
        let available_end = (segment.buffer.end() - shift).min(body_end);
        let stable_end = if natural_final && body_end == self.clock.end && available_end == body_end
        {
            body_end
        } else {
            available_end.div_euclid(BLOCK) * BLOCK
        };
        if stable_end > segment.next_sample {
            let range = segment.next_sample..stable_end;
            let source = segment
                .buffer
                .slices(range.start + shift..range.end + shift)
                .ok_or_else(|| VoiceError::new("Progressive PCM lost unpublished body samples"))?;
            if let Some(audio) = self.write(range, source)? {
                updates.push(audio);
            }
            segment.next_sample = stable_end;
            segment.buffer.discard_before(stable_end + shift);
        }
        if let Some((tail_begin, tail_end)) = segment.plan.tail {
            if segment.next_sample == body_end {
                self.join_tail(segment, tail_begin, tail_end, shift, &mut updates)?;
            }
        } else if natural_final && segment.next_sample == self.clock.end {
            segment.finished = true;
        }
        Ok(updates)
    }

    fn join_head(
        &mut self,
        segment: &mut Segment,
        updates: &mut Vec<RenderedAudio>,
    ) -> Result<bool> {
        let start = segment.plan.first_sample;
        let Some(fresh) = segment.buffer.slices(start..start + BLOCK * 2) else {
            return Ok(false);
        };
        let old = self.clock.range(start..start + BLOCK)?;
        let seam = splice_head(
            [&self.samples[0][old.clone()], &self.samples[1][old]],
            fresh,
            PREVIEW_BLOCK_SAMPLES,
        )?;
        if let Some(audio) = self.write(
            start..start + BLOCK,
            [
                &seam.samples[0][..PREVIEW_BLOCK_SAMPLES],
                &seam.samples[1][..PREVIEW_BLOCK_SAMPLES],
            ],
        )? {
            updates.push(audio);
        }
        segment.shift = Some(seam.shift_samples);
        segment.next_sample = start + BLOCK;
        segment
            .buffer
            .discard_before(segment.next_sample + i64::from(seam.shift_samples));
        Ok(true)
    }

    fn join_tail(
        &mut self,
        segment: &mut Segment,
        begin: i64,
        end: i64,
        shift: i64,
        updates: &mut Vec<RenderedAudio>,
    ) -> Result<()> {
        let Some(fresh) = segment.buffer.slices(begin + shift..end + shift) else {
            return Ok(());
        };
        let old = self.clock.range(begin..end)?;
        let seam = splice_tail(
            [&self.samples[0][old.clone()], &self.samples[1][old]],
            fresh,
            PREVIEW_SAMPLE_RATE,
            PREVIEW_BLOCK_SAMPLES,

            0.0,
        )?;
        let pending = segment
            .buffer
            .slices(segment.next_sample + shift..begin + shift)
            .ok_or_else(|| VoiceError::new("Progressive PCM lost its pending tail body"))?;
        let replacement: [Vec<f32>; 2] = std::array::from_fn(|channel| {
            let mut samples =
                Vec::with_capacity(pending[channel].len() + seam.samples[channel].len());
            samples.extend_from_slice(pending[channel]);
            samples.extend_from_slice(&seam.samples[channel]);
            samples
        });
        let end = begin + seam.samples[0].len() as i64;
        if let Some(audio) =
            self.write(segment.next_sample..end, [&replacement[0], &replacement[1]])?
        {
            updates.push(audio);
        }
        segment.next_sample = end;
        segment.finished = true;
        Ok(())
    }
}
