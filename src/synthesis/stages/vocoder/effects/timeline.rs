use crate::synthesis::{parameters::{self,VoiceParameters,FrameParameters},score::{PreparedPhrase,PreparedAutomation,FRAME_SECONDS},voice::Result};
use super::super::excitation::ExcitationFrame;

#[derive(Clone,Debug)]
pub struct EffectTimeline {
    voice:VoiceParameters,
    automation:Option<PreparedAutomation>,
    origin:f64,
    modes:Option<crate::synthesis::vocal_modes::VocalModeCoefficients>,
    first_frame:usize,
}
#[derive(Clone,Copy,Debug)]
pub struct FrameEffect {
    pub frame:usize,
    pub parameters:FrameParameters,
    pub f0:f32,
    pub shape:f32,
}
impl EffectTimeline {
    pub fn new(phrase:&PreparedPhrase,origin:f64)->Self {
        Self{voice:phrase.voice_parameters,automation:phrase.automation.clone(),origin,
            modes:phrase.mode_coefficients.clone(),first_frame:((origin-phrase.start_seconds)/FRAME_SECONDS).round().max(0.0) as usize}
    }
    pub fn frame(&self,index:usize,excitation:ExcitationFrame,reference_shape:f32)->FrameEffect {
        let mut parameters=parameters::sample(self.voice,self.automation.as_ref(),self.origin+index as f64*FRAME_SECONDS);
        if let Some(modes)=&self.modes {
            let normalizer=modes.normalization_at(self.first_frame+index);
            parameters.primary_gain/=normalizer;parameters.secondary_gain/=normalizer;
        }
        FrameEffect {frame:index,parameters,f0:excitation.f0_hz,shape:reference_shape}
    }
}
impl FrameEffect {
    pub(crate) fn apply(self,response:&mut [[f32;2]],noise:bool)->Result<()> {
        let count=response.len();let bins=count/2;
        let mut packed=vec![0.0;count];packed[0]=response[0][0];packed[1]=response[bins][0];
        for bin in 1..bins {packed[bin*2..bin*2+2].copy_from_slice(&response[bin]);}
        let p=self.parameters;
        #[cfg(feature="acoustic-diagnostic")]
        let observed=super::diagnostic::selected(self.frame,noise).then(||packed.clone());
        super::response(&mut packed,self.f0,p.gender_ratio,
            (!noise).then(||[self.shape,super::changed_shape(self.shape,p.shape_control)]),
            noise.then(||[((f64::from(p.secondary_gain)+0.1)/(f64::from(p.primary_gain)+0.1)) as f32,p.secondary_gain]))?;
        #[cfg(feature="acoustic-diagnostic")]
        if let Some(input)=observed {super::diagnostic::capture(self,noise,input,&packed);}
        if !noise {for value in &mut packed {*value*=p.primary_gain;}}
        response[0]=[packed[0],0.0];response[bins]=[packed[1],0.0];
        for bin in 1..bins {response[bin]=[packed[bin*2],packed[bin*2+1]];}
        Ok(())
    }
}
