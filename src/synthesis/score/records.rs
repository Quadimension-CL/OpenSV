//! Family-specific record-level onset placement.
//!
//! The original note retains its phonemes. Only the derived frontend records
//! move leading consonants into the preceding record before duration inference.

use crate::synthesis::{
    stages::{duration::DurationRecord, f0::frontend::FrontendNote},
    voice::{PreparedTree, Result, VoiceError, VoiceFamily},
};

pub(super) fn redistribute_onsets(
    tree: &PreparedTree,
    records: &mut [DurationRecord],
    notes: &mut [FrontendNote],
    family: VoiceFamily,
) -> Result<()> {
    if records.len() != notes.len() {
        return Err(VoiceError::new(
            "Onset redistribution requires matching frontend records",
        ));
    }

    // Resolve every source class before mutating either array. The selected
    // language table matters: classifying the unified symbol is not equivalent.
    let mut prefixes = Vec::with_capacity(records.len());
    for record in records.iter() {
        let mut first_core = None;
        for (index, phone) in record.phones.iter().enumerate() {
            let resolved = tree.resolve_phone(&phone.language, &phone.token)?;
            let core = match family {
                // SV1 transfers every leading non-vowel, including
                // semivowels; silence and breath are not nuclei.
                VoiceFamily::Sv1 => {
                    matches!(resolved.source_class, Some("vowel" | "diphthong"))
                }
                // SV2 treats sil/pau and language-local silence/breath
                // classes as nuclei in addition to vowels and diphthongs.
                VoiceFamily::Sv2Compatible | VoiceFamily::Sv2 => {
                    matches!(phone.token.as_str(), "sil" | "pau")
                        || matches!(
                            resolved.source_class,
                            Some("vowel" | "diphthong" | "silence" | "breath")
                        )
                }
            };
            if core && first_core.is_none() {
                first_core = Some(index);
            }
        }
        prefixes.push(first_core.unwrap_or(0));
    }

    let mut previous: Option<usize> = None;
    for (index, &prefix) in prefixes.iter().enumerate() {
        // Tied records do not become the preceding record; linked-note
        // merging is separate from onset redistribution.
        if notes[index].kind == 2 {
            continue;
        }
        if let Some(previous) = previous
            && prefix > 0
        {
            let (earlier, current) = records.split_at_mut(index);
            earlier[previous]
                .phones
                .extend(current[0].phones.drain(..prefix));
        }
        previous = Some(index);
    }

    for (record, note) in records.iter().zip(notes) {
        note.phones = record
            .phones
            .iter()
            .map(|phone| phone.token.clone())
            .collect();
    }
    Ok(())
}
