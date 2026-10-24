use crate::synthesis::{rendered::PitchTrack,voice::{Result,VoiceError}};

#[derive(Default)]
pub(super) struct EnsemblePitch {
    clock:Option<(f64,f64)>,
    sums:Vec<f32>,
    counts:Vec<u8>,
    minimum:Vec<f32>,
    maximum:Vec<f32>,
}

impl EnsemblePitch {
    pub fn add(&mut self,pitch:&PitchTrack)->Result<()> {
        let clock=(pitch.start_seconds,pitch.frame_seconds);
        if let Some(previous)=self.clock {
            if previous!=clock||self.sums.len()!=pitch.values.len() {
                return Err(VoiceError::new("齐唱成员的音高显示时钟不一致"));
            }
        }else{
            self.clock=Some(clock);
            self.sums=vec![0.0;pitch.values.len()];
            self.counts=vec![0;pitch.values.len()];
            self.minimum=vec![f32::INFINITY;pitch.values.len()];
            self.maximum=vec![f32::NEG_INFINITY;pitch.values.len()];
        }
        for segment in &pitch.segments {
            if segment.end>pitch.values.len() {return Err(VoiceError::new("齐唱音高显示范围无效"))}
            for index in segment.clone() {
                let value=pitch.values[index];
                if !value.is_finite(){continue}
                self.sums[index]+=value;
                self.counts[index]=self.counts[index].checked_add(1)
                    .ok_or_else(||VoiceError::new("齐唱音高成员数量超出范围"))?;
                self.minimum[index]=self.minimum[index].min(value);
                self.maximum[index]=self.maximum[index].max(value);
            }
        }
        Ok(())
    }

    pub fn finish(mut self,pitch:&mut PitchTrack) {
        let mut segments:Vec<std::ops::Range<usize>>=Vec::new();
        for (index,&count) in self.counts.iter().enumerate() {
            if count==0 {
                self.sums[index]=0.0;self.minimum[index]=0.0;self.maximum[index]=0.0;
            }else{
                self.sums[index]/=f32::from(count);
                if let Some(segment)=segments.last_mut() && segment.end==index {segment.end+=1}
                else{segments.push(index..index+1)}
            }
        }
        pitch.values=self.sums.into();pitch.segments=segments;
        pitch.unison_range=Some([self.minimum.into(),self.maximum.into()]);
    }
}
