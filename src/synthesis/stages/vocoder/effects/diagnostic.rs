use std::{cell::RefCell,collections::BTreeMap,marker::PhantomData,rc::Rc};
use serde::Serialize;
use super::FrameEffect;
use crate::synthesis::voice::{Result,VoiceError};
#[derive(Serialize)]
pub struct Row {
    frame:usize,noise:bool,f0:f32,shape:f32,changed_shape:f32,
    shape_control:f32,primary_gain:f32,secondary_gain:f32,gender:f32,
    input:Vec<f32>,output:Vec<f32>,
}
thread_local! {static ACTIVE:RefCell<Option<BTreeMap<(usize,bool),Row>>>=const{RefCell::new(None)};}
pub struct TraceGuard {finished:bool,_same_thread:PhantomData<Rc<()>>}
impl TraceGuard {
    pub fn begin()->Result<Self> {
        ACTIVE.with(|slot| {let mut slot=slot.borrow_mut();if slot.is_some(){return Err(VoiceError::new("Nested vocoder probe"));}
            *slot=Some(BTreeMap::new());Ok(Self{finished:false,_same_thread:PhantomData})})
    }
    pub fn finish(mut self)->Result<Vec<Row>> {
        self.finished=true;ACTIVE.with(|slot|slot.borrow_mut().take()).map(|rows|rows.into_values().collect())
            .ok_or_else(||VoiceError::new("Missing vocoder probe"))
    }
}
impl Drop for TraceGuard {fn drop(&mut self){if !self.finished {ACTIVE.with(|slot|{slot.borrow_mut().take();});}}}
pub(super) fn selected(frame:usize,noise:bool)->bool {
    [0,100,200,400,800,1200,1600,2200,2700].contains(&frame)
        && ACTIVE.with(|slot|slot.borrow().as_ref().is_some_and(|rows|!rows.contains_key(&(frame,noise))))
}
pub(super) fn capture(effect:FrameEffect,noise:bool,input:Vec<f32>,output:&[f32]) {
    ACTIVE.with(|slot|if let Some(rows)=slot.borrow_mut().as_mut() {
        if input.len()<=2048 && output.len()==input.len() {
            let p=effect.parameters;
            rows.insert((effect.frame,noise),Row {frame:effect.frame,noise,f0:effect.f0,shape:effect.shape,
                changed_shape:super::changed_shape(effect.shape,p.shape_control),shape_control:p.shape_control,
                primary_gain:p.primary_gain,secondary_gain:p.secondary_gain,gender:p.gender_ratio,input,output:output.to_vec()});
        }
    });
}
