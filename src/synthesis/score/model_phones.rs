use super::{LyricFrontend, ScorePhone};
use crate::{model::Language, synthesis::voice::{PreparedTree, Result}};

pub(super) fn materialize(
    syllables: Vec<Vec<String>>, language: Language, frontend: &LyricFrontend,
    tree: &PreparedTree, partition: bool,
) -> Result<Vec<Vec<ScorePhone>>> {
    let mut output = Vec::new();
    for syllable in syllables {
        let mut changed = false;
        let mut phones = Vec::with_capacity(syllable.len());
        for token in syllable {
            let missing = tree.resolve_phone(language.id(), &token)?.used_silence_fallback
                && frontend.unmapped_phone_class(language, &token)?.is_some();
            changed |= missing;
            phones.push(ScorePhone {
                language: language.id().to_owned(),
                token: if missing { "sil".into() } else { token.clone() },
                display_token: missing.then_some(token), onset_offset_seconds: None,
            });
        }
        if !changed || !partition {
            output.push(phones);
            continue;
        }

        let mut nuclei = Vec::new();
        for (i, phone) in phones.iter().enumerate() {
            let resolved = tree.resolve_phone(language.id(), &phone.token)?;
            if matches!(phone.token.as_str(), "sil" | "pau") || matches!(resolved.source_class,
                Some("vowel" | "diphthong" | "silence" | "breath")) {
                nuclei.push(i);
            }
        }
        let mut rest = phones;
        let mut consumed = 0;
        for nucleus in nuclei.iter().take(nuclei.len().saturating_sub(1)) {
            let tail = rest.split_off(nucleus + 1 - consumed);
            output.push(rest);
            rest = tail;
            consumed = nucleus + 1;
        }
        output.push(rest);
    }
    Ok(output)
}
