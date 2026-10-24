pub(super) fn formants(values:&[f32],ratio:f32,f0:f32,nyquist:f32)->Vec<f32> {
    let count=values.len();
    let frequency=|i:usize|i as f32*nyquist/count as f32;
    let last=frequency(count-1);
    let source=[0.0,f0,(4000.0*ratio).clamp(100.0,7000.0),8000.0,last];
    let target=[0.0,f0,4000.0,8000.0,last];
    let end=(last+last)-frequency(count-2);
    let inverse=1.0/end;
    let mut segment=0;
    (0..count).map(|i| {
        let q=frequency(i);
        while segment<4 && q>source[segment+1] {segment+=1;}
        let mapped=if segment==4 || q<=source[0] {target[segment]}
            else {(target[segment+1]-target[segment])*(q-source[segment])
                /(source[segment+1]-source[segment])+target[segment]};
        let position=(mapped*inverse)*count as f32;
        let index=position as usize;
        if position<0.0 {values[0]}
        else if f64::from(position)+1.01>=count as f64 {values[count-1]}
        else {(values[index+1]-values[index])*(position-index as f32)+values[index]}
    }).collect()
}

pub(super) fn uniform(values:&[f32],begin:f32,end:f32,queries:impl Iterator<Item=f32>)->Vec<f32> {
    let inverse=1.0/(end-begin);
    queries.map(|query| {
        let position=((query-begin)*inverse)*values.len() as f32;
        let i=position as usize;
        if query<begin {values[0]}
        else if f64::from(position)+1.01>=values.len() as f64 {values[values.len()-1]}
        else {(values[i+1]-values[i])*(position-i as f32)+values[i]}
    }).collect()
}
