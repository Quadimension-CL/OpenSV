use std::{cell::RefCell, marker::PhantomData, rc::Rc};
use serde::Serialize;
use crate::synthesis::{inference::Sequence, voice::{Result, VoiceError}};

const ROWS: [usize; 7] = [0, 40, 140, 240, 340, 460, 499];
const WIDTHS: [(usize, usize); 3] = [(76, 76), (256, 8), (256, 64)];
const LABELS: [&str; 3] = ["0x1b2ed9", "0x1ca741", "0x1caba8"];
const MAX_WORDS: usize = 5152;

#[derive(Serialize)]
pub struct Pair {
    frame: usize,
    input: Vec<f32>,
    condition: Vec<f32>,
}

#[derive(Serialize)]
pub struct Boundary {
    native_return_label: &'static str,
    input_channels: usize,
    condition_channels: usize,
    consumed_frames: usize,
    rows: Vec<Pair>,
}

struct State {
    boundaries: [Boundary; 3],
    words: usize,
}

thread_local! {
    static ACTIVE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub struct TraceGuard {
    finished: bool,

    _same_thread: PhantomData<Rc<()>>,
}

impl TraceGuard {
    pub fn begin() -> Result<Self> {
        ACTIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_some() { return Err(error("Nested acoustic diagnostic scope")); }
            *slot = Some(State {
                boundaries: std::array::from_fn(|i| Boundary {
                    native_return_label: LABELS[i], input_channels: WIDTHS[i].0,
                    condition_channels: WIDTHS[i].1, consumed_frames: 0, rows: Vec::new(),
                }),
                words: 0,
            });
            Ok(Self { finished: false, _same_thread: PhantomData })
        })
    }

    pub fn finish(mut self) -> Result<[Boundary; 3]> {
        self.finished = true;
        let state = ACTIVE.with(|slot| slot.borrow_mut().take())
            .ok_or_else(|| error("Missing acoustic diagnostic scope"))?;
        if state.words != MAX_WORDS || state.boundaries.iter().any(|b|
            b.consumed_frames != 500 || b.rows.iter().map(|r| r.frame).ne(ROWS)) {
            return Err(error("Acoustic diagnostic did not cover exactly three 500-frame boundaries"));
        }
        Ok(state.boundaries)
    }
}

impl Drop for TraceGuard {
    fn drop(&mut self) {
        if !self.finished { ACTIVE.with(|slot| { slot.borrow_mut().take(); }); }
    }
}

pub(crate) fn capture(index: usize, first_frame: usize, input: &Sequence, condition: &Sequence) -> Result<()> {
    ACTIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(state) = slot.as_mut() else { return Ok(()); };
        input.validate()?;
        condition.validate()?;
        if first_frame != 0 || index >= WIDTHS.len() || input.frames != condition.frames
            || (input.channels, condition.channels) != WIDTHS[index] {
            return Err(error("Unexpected acoustic diagnostic origin or shape"));
        }
        let boundary = &mut state.boundaries[index];
        let begin = boundary.consumed_frames;
        let end = begin.checked_add(input.frames).filter(|&end| end <= 500)
            .ok_or_else(|| error("Acoustic diagnostic frame bound exceeded"))?;
        for frame in ROWS.into_iter().filter(|&frame| begin <= frame && frame < end) {
            let left = input.frame(frame - begin)?;
            let right = condition.frame(frame - begin)?;
            if left.iter().chain(right).any(|x| !x.is_finite()) {
                return Err(error("Nonfinite selected acoustic activation"));
            }
            state.words = state.words.checked_add(left.len() + right.len())
                .filter(|&n| n <= MAX_WORDS).ok_or_else(|| error("Acoustic diagnostic word bound exceeded"))?;
            boundary.rows.push(Pair { frame, input: left.to_vec(), condition: right.to_vec() });
        }
        boundary.consumed_frames = end;
        Ok(())
    })
}

fn error(text: &'static str) -> VoiceError { VoiceError::new(text) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_selected_rows_and_distinct_counters() {
        let guard = TraceGuard::begin().unwrap();
        assert!(TraceGuard::begin().is_err());
        for (i, (x, c)) in WIDTHS.into_iter().enumerate() {
            for n in [0, 113, 387] {
                capture(i, 0, &Sequence::zeros(n, x).unwrap(), &Sequence::zeros(n, c).unwrap()).unwrap();
            }
        }
        let result = guard.finish().unwrap();
        assert_eq!(result.iter().map(|b| b.rows.iter().map(|r|r.input.len()+r.condition.len()).sum::<usize>()).sum::<usize>(), MAX_WORDS);
    }

    #[test]
    fn off_by_default_cleanup_and_fail_closed_bounds() {
        capture(99, 123, &Sequence::zeros(1, 1).unwrap(), &Sequence::zeros(1, 1).unwrap()).unwrap();
        let guard = TraceGuard::begin().unwrap();
        assert!(capture(0, 0, &Sequence::zeros(501, 76).unwrap(), &Sequence::zeros(501, 76).unwrap()).is_err());
        drop(guard);
        assert!(TraceGuard::begin().unwrap().finish().is_err());
    }
}
