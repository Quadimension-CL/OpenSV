use std::{cell::RefCell, marker::PhantomData, rc::Rc};
use serde::Serialize;
use crate::synthesis::voice::{Result, VoiceError};

#[derive(Serialize)]
pub struct Boundary {
    label: String,
    frames: usize,
    channels: usize,
    values: Vec<f32>,
    member: Option<Member>,
}
#[derive(Clone, Copy, Serialize)]
struct Member { index: usize, speaker: Option<usize> }
struct Trace { boundaries: Vec<Boundary>, limit: usize, member: Option<Member> }
thread_local! {
    static ACTIVE: RefCell<Option<Trace>> = const { RefCell::new(None) };
}
pub struct TraceGuard { finished: bool, _same_thread: PhantomData<Rc<()>> }
impl TraceGuard {
    pub fn begin() -> Result<Self> {
        Self::begin_members(1)
    }
    pub fn begin_members(members: usize) -> Result<Self> {
        if !(1..=16).contains(&members) {return Err(VoiceError::new("Invalid duration diagnostic member count"));}
        ACTIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_some() {return Err(VoiceError::new("Nested duration diagnostic"));}
            *slot = Some(Trace {boundaries:Vec::new(),limit:32*members,member:None});
            Ok(Self {finished:false, _same_thread:PhantomData})
        })
    }
    pub fn finish(mut self) -> Result<Vec<Boundary>> {
        self.finished = true;
        ACTIVE.with(|slot| slot.borrow_mut().take())
            .map(|trace|trace.boundaries)
            .ok_or_else(|| VoiceError::new("Missing duration diagnostic"))
    }
}

pub struct MemberScope(Option<Member>);
pub fn member_scope(index:usize,speaker:Option<usize>) -> MemberScope {
    MemberScope(ACTIVE.with(|slot| slot.borrow_mut().as_mut()
        .and_then(|trace|trace.member.replace(Member {index,speaker}))))
}
impl Drop for MemberScope {
    fn drop(&mut self) {ACTIVE.with(|slot| {if let Some(trace)=slot.borrow_mut().as_mut() {trace.member=self.0;}});}
}
impl Drop for TraceGuard {
    fn drop(&mut self) {
        if !self.finished {ACTIVE.with(|slot| {slot.borrow_mut().take();});}
    }
}
pub(crate) fn capture(label: &str, frames: usize, values: &[f32]) -> Result<()> {
    ACTIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(trace) = slot.as_mut() else {return Ok(())};
        if frames == 0 || frames > 4096 || !values.len().is_multiple_of(frames)
            || values.len()/frames > 512 || values.iter().any(|v| !v.is_finite())
            || trace.boundaries.len() >= trace.limit
            || trace.boundaries.iter().map(|b| b.values.len()).sum::<usize>()+values.len() > 2_097_152 {
            return Err(VoiceError::new("Duration diagnostic activation bound exceeded"));
        }
        trace.boundaries.push(Boundary {label:label.into(),frames,member:trace.member,
            channels:values.len()/frames,values:values.to_vec()});
        Ok(())
    })
}
