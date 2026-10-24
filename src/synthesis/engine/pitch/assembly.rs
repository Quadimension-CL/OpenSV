use crate::synthesis::{
    score::PreparedPhrase,
    stages::f0::{PitchResult, frontend::frame_count, postprocess::NOTE_CONTEXT_FRAMES},
    voice::{Result, VoiceError},
};

pub(crate) fn assemble(phrase: &PreparedPhrase, prediction: &PitchResult) -> Result<Vec<f32>> {
    if phrase.notes.len() != phrase.source_note_ids.len()
        || prediction.pitch.is_empty()
        || prediction.pitch.iter().any(|p| !p.is_finite())
    {
        return Err(VoiceError::new("Invalid note-curve assembly input"));
    }
    let counts = phrase
        .notes
        .iter()
        .map(frame_count)
        .collect::<Result<Vec<_>>>()?;
    let rows = phrase
        .notes
        .iter()
        .zip(&counts)
        .zip(&phrase.source_note_ids)
        .map(|((note, &count), source)| (count, note.pitch + note.pitch_offset, source.is_some()))
        .collect::<Vec<_>>();
    assemble_rows(&rows, &prediction.pitch)
}

fn assemble_rows(rows:&[(usize,f32,bool)],pitch:&[f32])->Result<Vec<f32>> {
    let total=rows.iter().try_fold(0_usize,|a,(count,_,_)|a.checked_add(*count)).ok_or_else(||VoiceError::new("Pitch assembly clock overflow"))?;
    if total!=pitch.len() || rows.iter().any(|(_,base,_)|!base.is_finite()) || pitch.iter().any(|v|!v.is_finite()) {
        return Err(VoiceError::new("Pitch assembly differs from model clock or contains invalid values"));
    }
    let mut accumulated=vec![0.0_f32;total];let mut weights=vec![0.0_f32;total];let mut curve_origin=0_usize;
    let mut seconds=0.0_f64;
    let context=NOTE_CONTEXT_FRAMES;
    for &(count,baseline,present) in rows {

        let scale=if present {1.0_f32}else{0.01_f32};

        let cursor=(seconds/crate::synthesis::score::FRAME_SECONDS) as usize;
        let next_seconds=seconds+count as f64*crate::synthesis::score::FRAME_SECONDS;
        let body_end=(next_seconds/crate::synthesis::score::FRAME_SECONDS) as usize;
        let begin=cursor.saturating_sub(context);let end=body_end.saturating_add(context).min(total);
        for frame in begin..end {
            let fraction=if frame<cursor {(frame+context-cursor) as f32/context as f32}
                else if frame>=body_end {(body_end+context-frame) as f32/context as f32}
                else {1.0};
            let squared=fraction*fraction;let weight=squared*squared;

            let source_frame=(curve_origin as i64+frame as i64-cursor as i64)
                .min((curve_origin+count+context-1) as i64).clamp(0,total as i64-1) as usize;
            let value=if present && count>0 {(pitch[source_frame]-baseline)+baseline}else{baseline};
            accumulated[frame]+=(value*weight)*scale;
            weights[frame]+=weight*scale;
        }
        curve_origin+=count;seconds=next_seconds;
    }
    for (value,weight) in accumulated.iter_mut().zip(weights) {
        if !weight.is_finite() || weight<=0.0 {return Err(VoiceError::new("Uncovered pitch assembly frame"));}
        *value/=weight.max(0.000_01);
    }
    Ok(accumulated)
}
