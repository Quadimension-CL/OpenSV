use std::{cell::RefCell,marker::PhantomData,rc::Rc};
use serde::Serialize;
use crate::synthesis::voice::{PreparedTree,Result,VoiceError};
mod frames;
pub use frames::FrameBoundary;
struct Active {rows:Vec<Boundary>,frames:Option<frames::Frames>}
pub struct Trace {pub boundaries:Vec<Boundary>,pub frames:Vec<FrameBoundary>}

#[derive(Serialize)]
pub struct Boundary {
    root_type:[u8;16],
    label:String,
    frames:usize,
    channels:usize,
    values:Vec<f32>,
}
thread_local! {static ACTIVE:RefCell<Option<Active>>=const {RefCell::new(None)};}
pub struct TraceGuard {finished:bool,_same_thread:PhantomData<Rc<()>>}
impl TraceGuard {
    pub fn begin()->Result<Self> {
        Self::begin_frames(None)
    }
    pub fn begin_with_frames(prefix:&std::path::Path)->Result<Self> {
        Self::begin_frames(Some(prefix))
    }
    fn begin_frames(prefix:Option<&std::path::Path>)->Result<Self> {
        ACTIVE.with(|slot|{let mut slot=slot.borrow_mut();if slot.is_some(){return Err(VoiceError::new("Nested shared feature diagnostic"))}
            *slot=Some(Active {rows:Vec::new(),frames:prefix.map(frames::Frames::new)});Ok(Self{finished:false,_same_thread:PhantomData})})
    }
    pub fn finish(mut self)->Result<Trace> {
        self.finished=true;
        ACTIVE.with(|slot|slot.borrow_mut().take()).map(|active|Trace {boundaries:active.rows,frames:active.frames.map_or_else(Vec::new,|frames|frames.rows)})
            .ok_or_else(||VoiceError::new("Missing shared feature diagnostic"))
    }
}
impl Drop for TraceGuard {fn drop(&mut self){if !self.finished{ACTIVE.with(|slot|{slot.borrow_mut().take();});}}}

pub(super) fn capture(tree:&PreparedTree,label:&str,frames:usize,values:&[f32])->Result<()> {
    ACTIVE.with(|slot|{
        let mut slot=slot.borrow_mut();let Some(active)=slot.as_mut() else{return Ok(())};
        let rows=&mut active.rows;
        if frames==0||frames>4096||!values.len().is_multiple_of(frames)
            ||values.len()/frames>512||values.iter().any(|v|!v.is_finite())||rows.len()>=32
            ||rows.iter().map(|row|row.values.len()).sum::<usize>()+values.len()>2_097_152 {
            return Err(VoiceError::new("Shared feature diagnostic activation bound exceeded"));
        }
        rows.push(Boundary{root_type:tree.node(tree.root_index())?.type_id,label:label.into(),frames,
            channels:values.len()/frames,values:values.to_vec()});Ok(())
    })
}

pub(super) fn capture_frames(tree:&PreparedTree,expanded:&super::ExpandedFeatures)->Result<()> {
    ACTIVE.with(|slot|{let mut slot=slot.borrow_mut();
        if let Some(active)=slot.as_mut() && let Some(frames)=active.frames.as_mut() {frames.capture(tree,expanded)?;}
        Ok(())
    })
}
