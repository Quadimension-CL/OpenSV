use super::{
    ExplicitScore, FRAME_SECONDS, LyricFrontend, PreparedPhrase, ScoreNote, ScorePhone, options,
};
use crate::{
    model::QUARTER,
    synthesis::{
        plan::{GlobalBlick, OwnerInput, TempoMapping},
        voice::{PreparedTree, Result, VoiceError, VoiceFamily},
    },
};

pub fn prepare_owner(
    input: &OwnerInput,
    lexicon: &LyricFrontend,
    duration_tree: &PreparedTree,
    family: VoiceFamily,
) -> Result<PreparedPhrase> {
    let mut phrase = ExplicitScore::from_owner(input, lexicon, duration_tree, family)?
        .prepare(duration_tree, family)?;
    phrase.automations = super::PreparedAutomations::new(input)?.map(std::sync::Arc::new);
    phrase.voice_controls =
        super::PreparedVoiceControls::new(input, family, phrase.automations.clone())?;
    let tuning = phrase
        .voice_controls
        .as_ref()
        .map_or(0.0, super::PreparedVoiceControls::tuning_semitones);
    if tuning != 0.0 {
        // Apply tuning to model conditions after validating the score's MIDI
        // pitches. Rest and boundary records share the same group tuning.
        for (record, note) in phrase.records.iter_mut().zip(&mut phrase.notes) {
            record.pitch_offset += tuning as f32;
            note.pitch_offset += tuning as f32;
        }
    }
    phrase.pitch_controls = super::PreparedPitchControls::new(input, tuning);
    phrase.performance_controls = super::PreparedPerformanceControls::new(input)?;
    Ok(phrase)
}

impl ExplicitScore {
    pub fn from_owner(
        input: &OwnerInput,
        lexicon: &LyricFrontend,
        duration_tree: &PreparedTree,
        family: VoiceFamily,
    ) -> Result<Self> {
        options::validate_owner(input, family)?;
        validate_tempo(&input.tempo)?;
        let language = lexicon.language(input)?;
        let mut sources = Vec::new();
        for note in &input.notes {
            if options::muted(&note.attributes)? {
                continue;
            }
            options::validate_note(note, family)?;
            if note.duration <= 0 {
                return Err(VoiceError::new(format!(
                    "Note {} has non-positive duration",
                    note.id
                )));
            }
            let begin = note
                .onset
                .0
                .checked_add(input.content_offset.0)
                .ok_or_else(|| VoiceError::new("Global note onset overflows"))?;
            let end = begin
                .checked_add(note.duration)
                .ok_or_else(|| VoiceError::new("Global note end overflows"))?;
            sources.push((begin, end, note));
        }
        sources.sort_by_key(|(begin, _, note)| (*begin, note.id));
        if sources.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(VoiceError::new(
                "Overlapping notes within one synthesis owner are not supported",
            ));
        }
        let visible_begin_seconds = seconds_at(&input.tempo, input.visible_begin)?;
        let visible_end_seconds = input
            .visible_end
            .map(|end| seconds_at(&input.tempo, end))
            .transpose()?;
        let start_seconds = sources
            .first()
            .map(|(begin, _, _)| seconds_at(&input.tempo, GlobalBlick(*begin)))
            .transpose()?
            .unwrap_or(visible_begin_seconds);
        let mut score = Self {
            start_seconds,
            visible_begin_seconds,
            visible_end_seconds,
            notes: Vec::new(),
        };
        let mut previous_time = 0.0;
        let mut previous_end_seconds = start_seconds;
        let mut previous_pitch = 60.0;
        for (begin, end, note) in sources {
            let begin_seconds = seconds_at(&input.tempo, GlobalBlick(begin))?;
            let end_seconds = seconds_at(&input.tempo, GlobalBlick(end))?;
            let begin_time = relative_time(begin_seconds - start_seconds, family)?;
            let end_time = relative_time(end_seconds - start_seconds, family)?;
            let (duration_seconds, gap_seconds) = if family == VoiceFamily::Sv1 {
                (
                    end_seconds - begin_seconds,
                    begin_seconds - previous_end_seconds,
                )
            } else {
                (
                    (end_time - begin_time) * FRAME_SECONDS,
                    (begin_time - previous_time) * FRAME_SECONDS,
                )
            };
            if end_time <= begin_time {
                return Err(VoiceError::new(format!(
                    "Note {} is shorter than one model frame",
                    note.id
                )));
            }
            if gap_seconds > 0.0 {
                // SV1 gaps are flagged rest records (kind 1); SV2 gaps
                // use the default kind and an unset flag.
                score.notes.push(ScoreNote {
                    source_note_id: None,
                    retake_seeds: Default::default(),
                    performance: Default::default(),
                    duration_seconds: gap_seconds,
                    pitch: previous_pitch,
                    phones: vec![ScorePhone {
                        language: language.id().to_owned(),
                        token: "sil".to_owned(),
                        source: None,
                        attributes: Default::default(),
                        source_note_onset_seconds: None,
                    }],
                    kind: u32::from(family == VoiceFamily::Sv1),
                    flag: family == VoiceFamily::Sv1,
                    tag: None,
                    parts: 1,
                    part_index: 0,
                });
            }
            let pitch = note
                .pitch
                .checked_add(input.pitch_offset)
                .filter(|pitch| (0..=127).contains(pitch))
                .ok_or_else(|| {
                    VoiceError::new(format!("Note {} pitch is outside MIDI range", note.id))
                })? as f32;
            let note_language = lexicon.note_language(note, language)?;
            let syllables = super::pronunciation::resolve_note(
                note,
                note_language,
                lexicon,
                input.voice.dictionary.as_ref(),
                duration_tree,
            )?;
            let parts = u32::try_from(syllables.len())
                .map_err(|_| VoiceError::new("Too many syllables in one note"))?;
            if parts == 0 || syllables.iter().any(Vec::is_empty) {
                return Err(VoiceError::new(format!("Note {} has no phonemes", note.id)));
            }
            let phoneme_attributes =
                crate::model::PhonemeAttributes::read(&note.attributes).map_err(VoiceError::new)?;
            let mut phone_index = 0;
            for (part_index, tokens) in syllables.into_iter().enumerate() {
                score.notes.push(ScoreNote {
                    source_note_id: Some(note.id),
                    retake_seeds: Some(super::retakes::note_seeds(note, family)?),
                    performance: crate::model::NotePerformance::from_parts(
                        &note.musical_type,
                        &note.attributes,
                    )
                    .map_err(VoiceError::new)?,
                    duration_seconds,
                    pitch,
                    phones: tokens
                        .into_iter()
                        .map(|token| {
                            let index = phone_index;
                            phone_index += 1;
                            ScorePhone {
                                language: note_language.id().to_owned(),
                                token,
                                source: Some(crate::model::SourcePhone {
                                    note_id: note.id,
                                    phone_index: index,
                                }),
                                attributes: phoneme_attributes
                                    .get(index)
                                    .copied()
                                    .unwrap_or_default(),
                                source_note_onset_seconds: Some(begin_seconds),
                            }
                        })
                        .collect(),
                    kind: 0,
                    // SV1 uses the note flag for even syllable splitting,
                    // which is enabled unless the note explicitly disables it.
                    flag: family == VoiceFamily::Sv1
                        && note
                            .attributes
                            .get("evenSyllableDuration")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(true),
                    tag: note
                        .extensions
                        .get("accent")
                        .and_then(serde_json::Value::as_str)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned),
                    parts,
                    part_index: part_index as u32,
                });
            }
            previous_time = end_time;
            previous_end_seconds = end_seconds;
            previous_pitch = pitch;
        }
        // ExplicitScore::prepare materializes the boundary context
        // and moves leading consonants into the preceding temporal record.
        Ok(score)
    }
}

/// Native SV1 retains seconds; SV2 uses integer frame positions. The latter
/// remain exact f64 integers so subtraction happens before scaling to seconds.
fn relative_time(seconds: f64, family: VoiceFamily) -> Result<f64> {
    let frames = seconds / FRAME_SECONDS;
    if !frames.is_finite() || frames < 0.0 || frames >= u32::MAX as f64 {
        return Err(VoiceError::new(
            "Score time exceeds the supported model frame range",
        ));
    }
    match family {
        // SV1 retains the mapped note duration in seconds. Both
        // syllable partitioning and phone timing corrections precede its
        // acoustic consumer's cumulative frame quantization.
        VoiceFamily::Sv1 => Ok(seconds),
        // Quantize SV2 absolute boundaries relative to the phrase origin,
        // rather than each duration independently, to avoid accumulated drift.
        VoiceFamily::Sv2Compatible | VoiceFamily::Sv2 => {
            Ok((frames + f64::from_bits(0x3fdf_ffff_ffff_ffff)).trunc())
        }
    }
}

fn validate_tempo(mapping: &TempoMapping) -> Result<()> {
    if !mapping.origin_seconds.is_finite()
        || mapping
            .points
            .first()
            .is_none_or(|point| point.position.0 != 0)
        || mapping
            .points
            .iter()
            .any(|point| !point.bpm.is_finite() || point.bpm <= 0.0)
        || mapping
            .points
            .windows(2)
            .any(|pair| pair[0].position.0 >= pair[1].position.0)
    {
        return Err(VoiceError::new("Invalid project tempo mapping"));
    }
    Ok(())
}

fn seconds_at(mapping: &TempoMapping, position: GlobalBlick) -> Result<f64> {
    let mut seconds = mapping.origin_seconds;
    if position.0 < 0 {
        seconds += position.0 as f64 / QUARTER as f64 * 60.0 / mapping.points[0].bpm;
    } else {
        for (index, point) in mapping.points.iter().enumerate() {
            if position.0 <= point.position.0 {
                break;
            }
            let end = mapping
                .points
                .get(index + 1)
                .map_or(position.0, |next| next.position.0.min(position.0));
            let distance = i128::from(end) - i128::from(point.position.0);
            seconds += distance as f64 / QUARTER as f64 * 60.0 / point.bpm;
        }
    }
    if !seconds.is_finite() {
        return Err(VoiceError::new("Project time mapping is non-finite"));
    }
    Ok(seconds)
}
