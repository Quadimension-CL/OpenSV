use super::Contract;
use crate::synthesis::{
    stages::timbre::TimbreRequest,
    voice::{Result, VoiceError},
};

pub(super) struct Records {
    pub log_hz: Vec<f32>,
    pub local_parameters: Vec<i32>,
    pub shape: Vec<f32>,
    pub noise_scale: Vec<f32>,
    pub offset: Vec<f32>,
}

pub(super) fn prepare(
    input: &TimbreRequest<'_>,
    contract: Contract,
    log_hz: Vec<f32>,
) -> Result<Records> {
    if input.notes.len() != input.note_modulation.len()
        || input.phone_impulses.is_some()
        || input.frame_controls.is_some()
    {
        return Err(VoiceError::new(
            "Compatible acoustics requires unmodified frame conditions",
        ));
    }
    let mut note_frames = vec![0_usize; input.notes.len()];
    let mut last_note = 0;
    let mut total = 0_usize;
    let mut local_parameters = Vec::with_capacity(input.phones.len());
    let mut shape = Vec::new();
    let mut noise_scale = Vec::new();
    let mut offset = Vec::new();
    for phone in input.phones {
        if phone.note_index >= note_frames.len() || phone.note_index < last_note {
            return Err(VoiceError::new(
                "Compatible acoustic phones are outside their note timeline",
            ));
        }
        last_note = phone.note_index;
        total = total
            .checked_add(phone.frames)
            .filter(|&count| count <= i32::MAX as usize)
            .ok_or_else(|| {
                VoiceError::new("Compatible acoustic timeline exceeds signed frame range")
            })?;
        note_frames[phone.note_index] += phone.frames;
        let note = &input.note_modulation[phone.note_index];
        if note
            .factor
            .is_some_and(|value| !value.is_nan() && value != 1.0)
        {
            return Err(VoiceError::new(
                "Compatible acoustic note factors do not use the version-2 control path",
            ));
        }
        let (seed, controls) = match contract {
            Contract::Compatible => {
                // Forward the note seed unchanged and use fixed frame controls
                // for the SV2 compatibility input contract.
                (note.timbre_seed.unwrap_or(0) as i32, [1.0, 1.0, 0.0])
            }
            Contract::Native => {
                if note.timbre_seed.is_some_and(|seed| seed != 0) {
                    return Err(VoiceError::new(
                        "Original SV1 acoustic retake seeds are not supported",
                    ));
                }
                // The seed comes from the phone option, not its frame count.
                // With no explicit option or retake, it remains zero and uses
                // the seed-zero control branch at factor 1.
                (0, [0.995, 0.5, 1.0])
            }
        };
        local_parameters.push(seed);
        shape.extend(std::iter::repeat_n(controls[0], phone.frames));
        noise_scale.extend(std::iter::repeat_n(controls[1], phone.frames));
        offset.extend(std::iter::repeat_n(controls[2], phone.frames));
    }
    if total != log_hz.len()
        || input.requested_frame > total
        || total == 0
        || note_frames
            .iter()
            .zip(input.note_modulation)
            .any(|(&frames, note)| frames != note.frames)
    {
        return Err(VoiceError::new(
            "Compatible acoustic note, phone and pitch timelines differ",
        ));
    }
    Ok(Records {
        log_hz,
        local_parameters,
        shape,
        noise_scale,
        offset,
    })
}
