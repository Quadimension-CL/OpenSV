use crate::synthesis::voice::{Result,VoiceError};

pub(super) fn flags(categories:&[bool],frames:&[usize])->Result<Vec<bool>> {
    if categories.len()!=frames.len(){return Err(VoiceError::new("Silence phone flags are not aligned"));}
    let count=frames.iter().try_fold(0usize,|a,&b|a.checked_add(b))
        .filter(|&n|n<=i32::MAX as usize).ok_or_else(||VoiceError::new("Silence timeline overflows"))?;
    let mut result=Vec::with_capacity(count);
    let mut previous=true;let mut previous_frames=0usize;
    for (&current,&length) in categories.iter().zip(frames) {
        let shared=previous&&current;
        result.extend(std::iter::repeat_n(shared,previous_frames.div_ceil(2)));
        result.extend(std::iter::repeat_n(shared,length/2));
        previous=current;previous_frames=length;
    }
    result.extend(std::iter::repeat_n(previous,previous_frames.div_ceil(2)));
    debug_assert_eq!(result.len(),count);
    Ok(result)
}
