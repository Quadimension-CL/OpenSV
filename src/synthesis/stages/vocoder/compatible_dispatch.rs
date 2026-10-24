use std::collections::VecDeque;
use super::{render::VocoderOutput,stream::NeuralStream};
use crate::synthesis::{inference::Sequence,voice::{Result,VoiceError}};

const BATCH:usize=4;
#[derive(Default)]
struct Policy {countdown:usize,previous_output_frames:usize}
impl Policy {
    fn select(&mut self,peak:f32,cached_frames:usize,flush:bool)->bool {
        if peak>=-8.0 {self.countdown=16_usize.div_ceil(BATCH);}
        else {self.countdown=self.countdown.saturating_sub(1);}
        let dispatch=self.countdown!=0||cached_frames==0||flush||cached_frames!=self.previous_output_frames;
        if dispatch {self.previous_output_frames=cached_frames;}
        dispatch
    }
}

pub(super) struct CompatibleDispatch {
    pending:VecDeque<[f32;72]>,
    policy:Policy,
    cached:Option<VocoderOutput>,
    gain:f32,
    input_frames:usize,
    output_frames:usize,
    finished:bool,
    #[cfg(feature="acoustic-diagnostic")]
    diagnostic:[Vec<f32>;4],
}
impl CompatibleDispatch {
    pub fn new()->Self {Self{pending:VecDeque::new(),policy:Policy::default(),cached:None,gain:0.0,input_frames:0,output_frames:0,finished:false,
        #[cfg(feature="acoustic-diagnostic")] diagnostic:std::array::from_fn(|_|Vec::new())}}

    pub fn process(&mut self,neural:&mut NeuralStream<'_>,physical:&Sequence,controls:Option<&Sequence>,final_batch:bool)->Result<VocoderOutput> {
        physical.validate()?;
        if self.finished||physical.channels!=72||controls.is_some()||physical.values.iter().any(|v|!v.is_finite()) {
            return Err(VoiceError::new("Compatible vocoder dispatch requires an open physical72 stream without metadata controls"));
        }
        self.input_frames=self.input_frames.checked_add(physical.frames).ok_or_else(||VoiceError::new("Compatible vocoder timeline overflows"))?;
        for row in physical.values.chunks_exact(72) {self.pending.push_back(row.try_into().unwrap());}
        let mut primary=Vec::new();let mut secondary=Vec::new();let mut metadata=Vec::new();let mut flushed=false;
        while self.pending.len()>=BATCH||(final_batch&&!self.pending.is_empty()) {
            let count=self.pending.len().min(BATCH);
            let flush=final_batch&&count==self.pending.len();
            let mut peak=-10.0_f32;let mut values=Vec::with_capacity(count*72);
            for _ in 0..count {
                let row=self.pending.pop_front().unwrap();
                for &value in &row[3..67] {peak=peak.max(value);}
                values.extend_from_slice(&row);
            }
            self.batch(neural,Sequence::from_values(72,values)?,peak,flush,&mut primary,&mut secondary,&mut metadata)?;
            flushed=flush;
        }
        if final_batch&&!flushed {
            self.batch(neural,Sequence::zeros(0,72)?,-10.0,true,&mut primary,&mut secondary,&mut metadata)?;
        }
        let frames=primary.len()/240;
        self.output_frames=self.output_frames.checked_add(frames).ok_or_else(||VoiceError::new("Compatible PCM timeline overflows"))?;
        if self.output_frames>self.input_frames||(final_batch&&self.output_frames!=self.input_frames) {
            return Err(VoiceError::new("Compatible vocoder dispatch changed the physical frame timeline"));
        }
        self.finished=final_batch;
        #[cfg(feature="acoustic-diagnostic")]
        if final_batch&&crate::synthesis::stages::f0::diagnostic::is_active() {
            for (index,(label,width)) in [("compatible_vocoder_dispatch",11),("compatible_vocoder_dispatched_physical",72),
                ("compatible_vocoder_neural_primary",240),("compatible_vocoder_neural_secondary",240)].into_iter().enumerate() {
                crate::synthesis::stages::f0::diagnostic::capture(label.into(),&Sequence::from_values(width,std::mem::take(&mut self.diagnostic[index]))?)?;
            }
        }
        Ok(VocoderOutput{primary:Sequence::from_values(240,primary)?,secondary:Sequence::from_values(240,secondary)?,metadata})
    }

    fn batch(&mut self,neural:&mut NeuralStream<'_>,physical:Sequence,peak:f32,flush:bool,primary:&mut Vec<f32>,secondary:&mut Vec<f32>,metadata:&mut Vec<super::output::FrameMetadata>)->Result<()> {
        let cached_frames=self.cached.as_ref().map_or(0,|v|v.primary.frames);
        #[cfg(feature="acoustic-diagnostic")]
        let before=(self.policy.countdown,self.policy.previous_output_frames,self.gain);
        let dispatch=self.policy.select(peak,cached_frames,flush);
        if dispatch {
            self.cached=Some(neural.process(&physical,None,flush)?);
            #[cfg(feature="acoustic-diagnostic")]
            if crate::synthesis::stages::f0::diagnostic::is_active() {
                self.diagnostic[1].extend_from_slice(&physical.values);
                self.diagnostic[2].extend_from_slice(&self.cached.as_ref().unwrap().primary.values);
                self.diagnostic[3].extend_from_slice(&self.cached.as_ref().unwrap().secondary.values);
            }
        }
        let output=self.cached.as_ref().expect("First batch always dispatches");
        for (&a,&b) in output.primary.values.iter().zip(&output.secondary.values) {
            self.gain*=0.995_f32;
            if self.policy.countdown!=0 {self.gain+=0.005_f32;}
            primary.push(a*self.gain);secondary.push(b*self.gain);
        }
        metadata.extend_from_slice(&output.metadata);
        #[cfg(feature="acoustic-diagnostic")]
        if crate::synthesis::stages::f0::diagnostic::is_active() {
            self.diagnostic[0].extend_from_slice(&[(self.input_frames-self.pending.len()) as f32,peak,before.0 as f32,self.policy.countdown as f32,
                cached_frames as f32,before.1 as f32,if dispatch {1.0}else{0.0},output.primary.frames as f32,before.2,self.gain,output.primary.values.len() as f32]);
        }
        Ok(())
    }
}
