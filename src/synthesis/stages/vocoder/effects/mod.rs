mod math;
mod warp;
mod lf;
mod timeline;
#[cfg(feature="acoustic-diagnostic")]
pub mod diagnostic;
pub use timeline::{EffectTimeline,FrameEffect};
use crate::synthesis::voice::{Result,VoiceError};

pub fn response(packed:&mut [f32],f0:f32,gender:f32,shape:Option<[f32;2]>,noise:Option<[f32;2]>)->Result<()> {
    if packed.len()<4 || !packed.len().is_multiple_of(2)
        || packed.iter().any(|v|!v.is_finite()) || !f0.is_finite() || f0<=0.0
        || !gender.is_finite() || gender<=0.0 {
        return Err(VoiceError::new("Invalid spectral response control input"));
    }
    let bins=packed.len()/2;
    let mut amplitudes=vec![0.0;bins];let mut phases=vec![0.0;bins];
    amplitudes[0]=math::log(packed[0].abs());
    for bin in 1..bins {
        let real=packed[bin*2];let imag=packed[bin*2+1];
        amplitudes[bin]=math::log(real*real+imag*imag)*0.5;
        phases[bin]=f64::from(imag).atan2(f64::from(real)) as f32;
    }
    if (f64::from(gender)-1.0).abs()>0.01 {
        amplitudes=warp::formants(&amplitudes,gender,f0,24000.0);
    }
    if let Some([old,new])=shape && f64::from((new-old).abs())>0.01 {
        let correction=lf::correction(old,new,f0,bins);
        for (a,c) in amplitudes.iter_mut().zip(correction) {*a+=c;}
    }
    if let Some([ratio,gain])=noise {
        let log_gain=(f64::from(gain)+1e-8).ln();
        let amount=ratio.log2().abs().min(1.0);
        for (bin,a) in amplitudes.iter_mut().enumerate() {
            let frequency=bin as f32*24000.0/bins as f32;
            let (begin,end)=if ratio<1.0 {(3000.0,7000.0)}else{(500.0,2000.0)};
            let weight=if frequency<begin {-0.30000000000000004}
                else {f64::from(((frequency-begin)/(end-begin)).min(1.0))*0.3+0.7-1.0};
            let scale=(f64::from(amount)*weight+1.0) as f32;
            *a=if gain>=1.0 {(f64::from(*a)+f64::from(scale)*log_gain) as f32}
                else {(f64::from(*a)+(f64::from((1.0-scale)*(1.0-gain))+f64::from(gain)+1e-8).ln()) as f32};
        }
    }
    packed[0]=math::exp(amplitudes[0]);
    for bin in 1..bins {
        let amplitude=math::exp(amplitudes[bin]);
        packed[bin*2]=math::sin(phases[bin]+1.570_796_4_f32)*amplitude;
        packed[bin*2+1]=math::sin(phases[bin])*amplitude;
    }
    Ok(())
}

pub fn changed_shape(shape:f32,control:f32)->f32 {
    let base=shape.clamp(0.1,3.0);
    let delta=control.clamp(-1.0,1.0);
    let changed=base+delta*if delta>0.0 {3.0-base}else{base-0.1};
    let map=|value:f32|(value+0.0749).ln()*-3.1276-6.7906;
    let reference=map(base);
    let difference=map(changed)-reference;
    let difference=if difference>0.0 {((f64::from(difference)/3.0).tanh()*3.0) as f32}else{difference};
    (((difference+reference+6.7906)/-3.1276).exp()-0.0749).clamp(0.1,3.0)
}
