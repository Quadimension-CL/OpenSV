use std::{cell::RefCell, collections::BTreeMap, marker::PhantomData, rc::Rc};
use serde::Serialize;
use crate::synthesis::{inference::Sequence, voice::{Result, VoiceError}};

const ROWS: [usize; 14] = [0, 1, 40, 140, 240, 340, 460, 499, 700, 1000, 1500, 2000, 2500, 2749];
#[derive(Serialize)]
pub struct Row { frame: usize, input: Vec<f32>, condition: Vec<f32> }
#[derive(Default, Serialize)]
pub struct Boundary { consumed_frames: usize, input_channels: usize, condition_channels: usize, rows: Vec<Row> }
thread_local! { static ACTIVE: RefCell<Option<BTreeMap<&'static str, Boundary>>> = const { RefCell::new(None) }; }
pub struct TraceGuard { finished: bool, _same_thread: PhantomData<Rc<()>> }
impl TraceGuard {
    pub fn begin() -> Result<Self> {
        ACTIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_some() { return Err(VoiceError::new("Nested acoustic probe")); }
            *slot = Some(BTreeMap::new());
            Ok(Self { finished: false, _same_thread: PhantomData })
        })
    }
    pub fn finish(mut self) -> Result<BTreeMap<&'static str, Boundary>> {
        self.finished = true;
        ACTIVE.with(|slot| slot.borrow_mut().take()).ok_or_else(|| VoiceError::new("Missing acoustic probe"))
    }
}
impl Drop for TraceGuard {
    fn drop(&mut self) { if !self.finished { ACTIVE.with(|slot| { slot.borrow_mut().take(); }); } }
}

pub(crate) fn capture_noise(label: &'static str, first: usize, stride: usize, channels: usize, values: &[f32]) -> Result<()> {
    ACTIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(boundaries) = slot.as_mut() else { return Ok(()); };
        if channels == 0 || channels > 512 || !values.len().is_multiple_of(channels)
            || values.len() / channels > 4096 || values.iter().any(|v| !v.is_finite())
            || boundaries.contains_key(label) {
            return Err(VoiceError::new("Computed acoustic noise diagnostic bound exceeded"));
        }
        let rows = values.chunks_exact(channels).enumerate().map(|(frame, row)| Row {
            frame: first / stride + frame, input: row.to_vec(), condition: Vec::new(),
        }).collect::<Vec<_>>();
        boundaries.insert(label, Boundary { consumed_frames: rows.len(), input_channels: channels, condition_channels: 0, rows });
        Ok(())
    })
}
pub(crate) fn capture(label: &'static str, input: &Sequence, condition: &Sequence) -> Result<()> {
    ACTIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(boundaries) = slot.as_mut() else { return Ok(()); };
        input.validate()?; condition.validate()?;
        if input.frames != condition.frames || input.channels > 512 || condition.channels > 512 || boundaries.len() > 9 {
            return Err(VoiceError::new("Acoustic probe shape bound exceeded"));
        }
        let boundary = boundaries.entry(label).or_default();
        if boundary.consumed_frames != 0 && (boundary.input_channels != input.channels || boundary.condition_channels != condition.channels) {
            return Err(VoiceError::new("Acoustic probe channels changed"));
        }
        boundary.input_channels = input.channels; boundary.condition_channels = condition.channels;
        let begin = boundary.consumed_frames;

        let end = begin.checked_add(input.frames).filter(|end| *end <= 120_000)
            .ok_or_else(|| VoiceError::new("Acoustic probe frame bound exceeded"))?;
        for frame in ROWS.into_iter().filter(|frame| begin <= *frame && *frame < end) {
            let left = input.frame(frame - begin)?; let right = condition.frame(frame - begin)?;
            if left.iter().chain(right).any(|v| !v.is_finite()) { return Err(VoiceError::new("Nonfinite acoustic probe row")); }
            boundary.rows.push(Row { frame, input: left.to_vec(), condition: right.to_vec() });
        }
        boundary.consumed_frames = end;
        Ok(())
    })
}
