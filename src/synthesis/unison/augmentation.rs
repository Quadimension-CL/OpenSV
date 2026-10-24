use super::{SeatingPlan,UnisonSettings};
use crate::synthesis::{inference::{ModelRuntime,Sequence},score::{PreparedPhrase,FRAME_SECONDS},
    stages::f0::frontend::FrontendNote,voice::{PreparedVoice,Result,VoiceError}};

const TYPE:[u8;16]=[0x77,0xde,0xc2,0xf8,0x44,0xfd,0x03,0x39,0x5d,0x28,0xaa,0x83,0x3a,0xc8,0xf2,0xa1];
const M:u64=0x5851_f42d_4c95_7f2d;
const ADD:u64=0xda3e_39cb_94b9_5bdb;

pub(crate) fn prepare_members(voice:&PreparedVoice,phrase:&PreparedPhrase,seating:&SeatingPlan)->Result<Vec<PreparedPhrase>> {
    let settings:&UnisonSettings=&phrase.unison;
    if seating.count==1 || voice.speakers.is_empty() {

        return (0..seating.count).map(|member|settings.member_phrase(phrase,member)).collect();
    }
    let tree=voice.model("unconfirmed")?;
    let root=tree.node(tree.root_index())?;
    if root.type_id!=TYPE || root.payload_bytes!=0 || root.children.is_empty() {
        return Err(VoiceError::new("Selected voice has no supported ensemble timing/pitch model"));
    }
    let notes=&phrase.unison_notes;
    let n=notes.len();
    if n!=phrase.notes.len() || n<2 || n>1_000_000 || seating.count>16 {
        return Err(VoiceError::new("Invalid central ensemble note timeline"));
    }
    let input=features(notes)?;
    let hashes=notes.iter().map(note_hash).collect::<Vec<_>>();
    let mut variations=Vec::with_capacity(seating.count);
    for member in 0..seating.count {
        crate::synthesis::execution::checkpoint()?;
        let speaker=seating.speakers[seating.first+member].unwrap_or(0);
        let index=*root.children.get(speaker).ok_or_else(||VoiceError::new("Ensemble model lacks selected member"))?;
        let affine=tree.affine(index)?;
        if affine.input_channels()!=36 || affine.output_channels()!=4 {return Err(VoiceError::new("Unsupported ensemble affine shape"));}
        let mut runtime=ModelRuntime::new(tree);
        let prediction=runtime.execute(index,&input,true)?;
        let mut coefficients=prediction.values.chunks_exact(4).map(|r|[r[0],r[1],r[2],r[3]]).collect::<Vec<_>>();
        if coefficients.len()!=n {return Err(VoiceError::new("Ensemble prediction length differs"));}
        for i in 0..n {
            if is_silence(&notes[i]) && is_silence(&notes[i.saturating_sub(1)]) {
                coefficients[i]=[0.05,0.5,0.07071068,0.70710677];
            }
        }
        apply_spread(&mut coefficients,notes);
        let mut random=vec![[0.;2];n];
        for i in 0..n {
            if i%128==0 {crate::synthesis::execution::checkpoint()?;}
            let seeds=phrase.retake_seeds[i];
            random[i]=[normal(hashes[i],member,central_duration_seed(&phrase.retake_seeds,i),0x3415_8ee8_41da_03f4),
                normal(hashes[i],member,seeds.map(|s|s.pitch),0xb578_a886_de24_5e20)];
        }
        variations.push(temporal(&coefficients,&random)?);
    }
    let normalization=(1.-1./seating.count as f64).sqrt().recip();
    let mut means=vec![[0.;2];n];
    for v in &variations {for (mean,value) in means.iter_mut().zip(v) {mean[0]+=value[0];mean[1]+=value[1];}}
    for mean in &mut means {mean[0]/=seating.count as f64;mean[1]/=seating.count as f64;}
    let mut base=vec![0.];
    for note in notes {base.push(base.last().unwrap()+note.duration_seconds);}
    let mut output=Vec::with_capacity(seating.count);
    for (member,variation) in variations.iter_mut().enumerate() {
        for (v,mean) in variation.iter_mut().zip(&means) {v[0]=(v[0]-mean[0])*normalization;v[1]=(v[1]-mean[1])*normalization;}
        let mut central=base.clone();
        for i in 1..n {
            let shift=variation[i][0];
            let bounded=if shift<0. {shift.max(-((central[i]-central[i-1])*0.5))}
                else{shift.min((central[i+1]-central[i])*0.5)};
            central[i]+=bounded;
        }
        for i in 0..n {
            central[i]=(central[i]/FRAME_SECONDS).round()*FRAME_SECONDS;
            if i>0 {central[i]=central[i].max(central[i-1]);}
        }

        let mut summed=vec![0.];
        for i in 0..n {summed.push(summed.last().unwrap()+((central[i+1]-central[i])/FRAME_SECONDS).round()*FRAME_SECONDS);}
        let mut boundaries=base.clone();
        for i in 1..n {
            if phrase.source_note_ids[i].is_some() || i==n-1 {
                boundaries[i]+=summed[i]-base[i];
            }
            boundaries[i]=boundaries[i].max(boundaries[i-1]);
        }
        let mut selected=settings.member_phrase(phrase,member)?;
        for i in 0..n {
            let duration=boundaries[i+1]-boundaries[i];
            let pitch=if phrase.source_note_ids[i].is_some() && notes[i].kind!=1 {
                let central_pitch=(f64::from(notes[i].pitch)+variation[i][1]) as f32;
                let delta=central_pitch-notes[i].pitch;
                (f64::from(selected.notes[i].pitch)+f64::from(delta)) as f32
            }else{selected.notes[i].pitch};
            selected.records[i].note.duration_seconds=duration;
            selected.records[i].note.pitch=pitch;
            selected.notes[i].duration_seconds=duration;
            selected.notes[i].pitch=pitch;
        }

        for record in &mut selected.records {record.linked_durations_seconds.clear();}
        let mut previous:Option<usize>=None;
        for i in 0..n {
            if selected.notes[i].kind==2 {
                if let Some(previous)=previous {selected.records[previous].linked_durations_seconds.push(duration_frame_seconds(&selected.notes[i])?);}
            }else if !selected.records[i].phones.is_empty() {previous=Some(i);}
        }
        output.push(selected);
    }
    Ok(output)
}

fn central_duration_seed(seeds:&[Option<crate::model::RetakeSeeds>],index:usize)->Option<u32> {
    let source=if index+1==seeds.len() {index.checked_sub(1)?}else{index};
    seeds.get(source).copied().flatten().map(|s|s.duration)
}

fn apply_spread(coefficients:&mut [[f32;4]],notes:&[FrontendNote]) {
    for (c,note) in coefficients.iter_mut().zip(notes) {
        let duration=note.performance.choir_onset.max(0.01);
        let pitch=note.performance.choir_pitch.max(0.01);
        c[0]*=duration;c[2]*=duration;c[1]*=pitch;c[3]*=pitch;
    }
}

fn duration_frame_seconds(note:&FrontendNote)->Result<f64> {
    Ok(crate::synthesis::stages::f0::frontend::frame_count(note)? as f64*note.frame_seconds)
}
fn is_silence(note:&FrontendNote)->bool {note.kind==1 || note.phones.iter().any(|p|matches!(p.as_str(),"sil"|"pau"|"br"))}
fn features(notes:&[FrontendNote])->Result<Sequence> {
    let mut values=Vec::with_capacity(notes.len()*36);
    for (i,note) in notes.iter().enumerate() {
        if i%128==0 {crate::synthesis::execution::checkpoint()?;}
        let prev=&notes[i.saturating_sub(1)];let next=&notes[(i+1).min(notes.len()-1)];
        let base=[(note.pitch-prev.pitch).clamp(-12.,12.)/12.,(next.pitch-note.pitch).clamp(-12.,12.)/12.,
            (prev.duration_seconds as f32).clamp(0.,3.),(note.duration_seconds as f32).clamp(0.,3.),(next.duration_seconds as f32).clamp(0.,3.),
            u8::from(is_silence(prev)) as f32,u8::from(is_silence(note)) as f32,u8::from(is_silence(next)) as f32,u8::from(next.kind==2) as f32];
        let mut power=base;values.extend_from_slice(&power);
        for _ in 0..3 {for (v,b) in power.iter_mut().zip(&base) {*v*=b;}values.extend_from_slice(&power);}
    }
    Sequence::from_values(36,values)
}
fn temporal(coefficients:&[[f32;4]],random:&[[f64;2]])->Result<Vec<[f64;2]>> {
    let n=coefficients.len();
    if n!=random.len() {return Err(VoiceError::new("Ensemble random and variance shapes differ"));}
    let mut variation=vec![[0.;2];n];
    for channel in 0..2 {
        let sigma=coefficients.iter().map(|c|f64::from(c[channel])).collect::<Vec<_>>();
        let noise=random.iter().zip(&sigma).map(|(r,s)|r[channel]*s).collect::<Vec<_>>();
        for i in 0..n {
            if i%128==0 {crate::synthesis::execution::checkpoint()?;}
            let kernel=weights(bandwidth(f64::from(coefficients[i][channel+2]),sigma[i])?);
            let mut value=0.;let mut variance=0.;
            for (k,weight) in kernel.into_iter().enumerate() {
                let j=i as isize+k as isize-4;
                if j>=0 && (j as usize)<n {
                    let j=j as usize;value+=weight*noise[j];variance+=(sigma[j]*sigma[j]*weight)*weight;
                }
            }
            variation[i][channel]=if variance>0. {value*sigma[i]/variance.sqrt()}else{0.};
        }
    }
    Ok(variation)
}
fn note_hash(note:&FrontendNote)->u64 {
    let mut text=String::new();for phone in &note.phones {text.push_str(phone);text.push(',');}text.push('_');
    text.push_str(&((note.duration_seconds/note.frame_seconds).round().max(0.) as u32).to_string());
    xxhash_rust::xxh3::xxh3_128(text.as_bytes()) as u64
}
fn output(state:u64)->u32 {(((state>>45)^(state>>27)) as u32).rotate_right((state>>59) as u32)}
fn next(state:&mut u64)->u32 {let result=output(*state);*state=state.wrapping_mul(M).wrapping_add(ADD);result}
fn uniform(state:&mut u64)->f64 {let low=next(state)>>11;let high=next(state);((u64::from(high)<<21)|u64::from(low)) as f64*(1./9007199254740992.)}
fn normal(hash:u64,member:usize,seed:Option<u32>,tag:u64)->f64 {
    let operand=2*member as u64;let inc=seed.map_or(1,|s|2*u64::from(s)+1);
    let mut state=hash.wrapping_add(operand).wrapping_mul(M).wrapping_add(operand).wrapping_mul(M)
        .wrapping_add(inc).wrapping_mul(M).wrapping_add(inc).wrapping_add(tag);
    loop {
        let x=uniform(&mut state)*2.-1.;let y=uniform(&mut state)*2.-1.;let radius=x*x+y*y;
        if x==0. || y==0. || radius>=1. {continue;}
        let (x,radius,extra)=if radius<=0.0001 {
            let exponent=x.abs().max(y.abs()).log2().floor() as i32;
            let scale=2f64.powi(-exponent);let a=x*scale;let b=y*scale;
            (a,a*a+b*b,f64::from(exponent)*std::f64::consts::LN_2*2.)
        }else{(x,radius,0.)};
        return (x*((radius.ln()+extra)*-2./radius).sqrt()).clamp(-2.,2.);
    }
}
fn weights(width:f64)->[f64;9] {
    let mut result=[0.;9];let mut total=0.;
    for (k,value) in result.iter_mut().enumerate() {*value=(((k as f64-4.)*width).clamp(-0.5,0.5)*std::f64::consts::TAU+std::f64::consts::PI).cos()*-0.5+0.5;total+=*value;}
    for value in &mut result {*value/=total;}result
}
fn energy_ratio(width:f64)->f64 {
    let weights=weights(width);let mut difference=0.;let mut energy=0.;let mut prev=0.;
    for value in weights {let delta=value-prev;difference+=delta*delta;energy+=value*value;prev=value;}
    (difference+prev*prev)/energy
}
fn bandwidth(delta:f64,sigma:f64)->Result<f64> {
    if !delta.is_finite() || !sigma.is_finite() {return Err(VoiceError::new("Invalid ensemble variance prediction"));}
    let target=(delta/sigma)*(delta/sigma);
    if target>=2. || sigma==0. {return Ok(0.5);}
    let mut low=0.;let mut high=1.;let mut sign=energy_ratio(0.)-target;
    for _ in 0..100 {let mid=(high-low)*0.5+low;if mid-low<1e-9{return Ok(mid);}let value=energy_ratio(mid)-target;
        if value==0. {return Ok(mid);}if sign*value<0. {high=mid;}else{low=mid;sign=value;}}
    Ok((high-low)*0.5+low)
}
