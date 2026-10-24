use std::sync::Arc;
use crate::synthesis::{inference::Sequence,voice::{Result,VoiceError}};
use super::{render::VocoderOutput,stream::NeuralStream};

const PACKET_FRAMES:usize=35;
pub(super) struct Sv2Packets {
    seeds:Arc<[u64]>,
    physical:Vec<f32>,
    controls:Vec<f32>,
    has_controls:Option<bool>,
    input_frames:usize,
    output_frames:usize,
    finished:bool,
}

impl Sv2Packets {
    pub(super) fn new(seeds:Arc<[u64]>)->Self {
        Self{seeds,physical:Vec::new(),controls:Vec::new(),has_controls:None,input_frames:0,output_frames:0,finished:false}
    }
    pub(super) fn process(&mut self,neural:&mut NeuralStream<'_>,physical:&Sequence,controls:Option<&Sequence>,end:bool)->Result<VocoderOutput> {
        physical.validate()?;
        if self.finished || physical.channels!=72 || controls.is_some_and(|c|c.channels!=2 || c.frames!=physical.frames) {
            return Err(VoiceError::new("Invalid AI2 vocoder packet input"));
        }
        if let Some(controls)=controls {controls.validate()?;}
        if physical.frames>0 {
            let present=controls.is_some();
            if self.has_controls.is_some_and(|previous|previous!=present) {return Err(VoiceError::new("AI2 metadata presence changed within a context"));}
            self.has_controls=Some(present);
        }
        self.input_frames=self.input_frames.checked_add(physical.frames).ok_or_else(||VoiceError::new("AI2 packet frame count overflow"))?;
        if self.input_frames>self.seeds.len() {return Err(VoiceError::new("AI2 physical input exceeds its record seed source"));}
        self.physical.extend_from_slice(&physical.values);
        if let Some(controls)=controls {self.controls.extend_from_slice(&controls.values);}
        let mut primary=Vec::new();let mut secondary=Vec::new();let mut metadata=Vec::new();let mut flushed=false;
        while self.physical.len()/72>=PACKET_FRAMES || end&&!self.physical.is_empty() {
            let frames=(self.physical.len()/72).min(PACKET_FRAMES);
            let final_packet=end&&self.physical.len()==frames*72;
            let seed_end=self.output_frames.checked_add(frames).ok_or_else(||VoiceError::new("AI2 seed cursor overflow"))?;
            let seeds=self.seeds.get(self.output_frames..seed_end).ok_or_else(||VoiceError::new("AI2 output cursor exceeds record seed source"))?;
            neural.append_frame_noise_seeds(seeds)?;
            let input=Sequence::new(frames,72,self.physical.drain(..frames*72).collect())?;
            let controls=if self.has_controls==Some(true) {Some(Sequence::new(frames,2,self.controls.drain(..frames*2).collect())?)}else{None};
            let output=neural.process(&input,controls.as_ref(),final_packet)?;
            self.output_frames+=output.primary.frames;
            primary.extend(output.primary.values);secondary.extend(output.secondary.values);metadata.extend(output.metadata);
            flushed=final_packet;
        }
        if end&&!flushed {
            let input=Sequence::zeros(0,72)?;
            let controls=if self.has_controls==Some(true) {Some(Sequence::zeros(0,2)?)}else{None};
            let output=neural.process(&input,controls.as_ref(),true)?;
            self.output_frames+=output.primary.frames;
            primary.extend(output.primary.values);secondary.extend(output.secondary.values);metadata.extend(output.metadata);
        }
        if end {
            if self.input_frames!=self.output_frames || !self.physical.is_empty() || !self.controls.is_empty() {
                return Err(VoiceError::new("AI2 vocoder packet tail does not align"));
            }
            self.finished=true;
        }
        Ok(VocoderOutput{primary:Sequence::from_values(240,primary)?,secondary:Sequence::from_values(240,secondary)?,metadata})
    }
}
