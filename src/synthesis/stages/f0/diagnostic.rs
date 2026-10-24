use std::{cell::RefCell,marker::PhantomData,rc::Rc};
use serde::Serialize;
use crate::synthesis::{inference::Sequence,voice::{Result,VoiceError}};

#[derive(Serialize)]
pub struct Row {frame:usize,values:Vec<f32>}
#[derive(Serialize)]
pub struct Boundary {label:String,frames:usize,channels:usize,rows:Vec<Row>,
    #[serde(skip_serializing_if="Option::is_none")] values:Option<Vec<f32>>}
thread_local! {static ACTIVE:RefCell<Option<Vec<Boundary>>>=const {RefCell::new(None)};}
pub struct TraceGuard {finished:bool,_same_thread:PhantomData<Rc<()>>}
pub(crate) fn is_active()->bool {ACTIVE.with(|slot|slot.borrow().is_some())}
impl TraceGuard {
    pub fn begin()->Result<Self> {
        ACTIVE.with(|slot|{let mut slot=slot.borrow_mut();if slot.is_some(){return Err(VoiceError::new("Nested F0 sampler diagnostic"))}
            *slot=Some(Vec::new());Ok(Self{finished:false,_same_thread:PhantomData})})
    }
    pub fn finish(mut self)->Result<Vec<Boundary>> {
        self.finished=true;ACTIVE.with(|slot|slot.borrow_mut().take()).ok_or_else(||VoiceError::new("Missing F0 sampler diagnostic"))
    }
}
impl Drop for TraceGuard {fn drop(&mut self){if !self.finished{ACTIVE.with(|slot|{slot.borrow_mut().take();});}}}
pub(crate) fn capture(label:String,sequence:&Sequence)->Result<()> {
    ACTIVE.with(|slot|{let mut slot=slot.borrow_mut();let Some(boundaries)=slot.as_mut() else{return Ok(())};
        sequence.validate()?;
        if sequence.frames>120_000||sequence.channels>512||boundaries.len()>=48{return Err(VoiceError::new("F0 sampler diagnostic bound exceeded"))}
        let mut selected=vec![0,1,40,140,240,340,460,499,674,1000,2000];
        if sequence.frames>0 {selected.push(sequence.frames-1);}
        selected.sort_unstable();selected.dedup();
        let rows=selected.into_iter().filter(|i|*i<sequence.frames).map(|frame|{
            Ok(Row{frame,values:sequence.frame(frame)?.to_vec()})
        }).collect::<Result<Vec<_>>>()?;
        if rows.iter().flat_map(|row|&row.values).any(|v|!v.is_finite()){return Err(VoiceError::new("Nonfinite F0 sampler diagnostic"))}
        let full=sequence.frames<=4096 && (label.starts_with("compatible_") || matches!(label.as_str(),"sampler_0_input"|"sampler_9_condition"|"sampler_10_condition"|"sampler_12_output"));
        let values=full.then(||sequence.values.clone());
        boundaries.push(Boundary{label,frames:sequence.frames,channels:sequence.channels,rows,values});Ok(())
    })
}
