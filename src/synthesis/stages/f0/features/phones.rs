use super::FeatureNetwork;
use crate::{
    model::Language,
    synthesis::{
        inference::ops::embedding::Embedding,
        stages::f0::TimedPhone,
        voice::{PhoneticMap, PreparedModule, Result, VoiceError},
    },
};

pub(super) enum PhoneEncoder {
    Categorical(Embedding),
    Phonetic(PhoneticMap),
}
pub(super) enum PhoneValues {
    Categorical(Vec<usize>),
    Phonetic(Vec<f32>),
}
pub struct FeaturePhones {
    pub language_ids: Vec<usize>,
    pub vowels: Vec<bool>,
    pub(super) values: PhoneValues,
}

impl FeatureNetwork<'_> {
    pub fn prepare_phones(&self, phones: &[TimedPhone]) -> Result<FeaturePhones> {
        let mut language_ids = Vec::with_capacity(phones.len());
        let mut vowels = Vec::with_capacity(phones.len());
        let values = match &self.phone_encoder {
            PhoneEncoder::Categorical(_) => {
                let mut ids = Vec::with_capacity(phones.len());
                for phone in phones {
                    let resolved = self.tree.resolve_phone(&phone.language, &phone.token)?;
                    let table = self
                        .tree
                        .nodes()
                        .iter()
                        .find_map(|node| match &node.module {
                            PreparedModule::PhoneSet(table)
                                if table.name == resolved.selected_phone_set =>
                            {
                                Some(table)
                            }
                            _ => None,
                        })
                        .ok_or_else(|| {
                            VoiceError::new("Shared phone classification table is missing")
                        })?;
                    vowels.push(
                        table
                            .lookup(&phone.token)
                            .is_some_and(|p| matches!(p.class, "vowel" | "diphthong")),
                    );
                    ids.push(resolved.unified_phone_index);
                    language_ids.push(resolved.language_index);
                }
                PhoneValues::Categorical(ids)
            }
            PhoneEncoder::Phonetic(map) => {
                let mut values = Vec::with_capacity(phones.len() * 73);
                for phone in phones {
                    let language = Language::parse(&phone.language).ok_or_else(|| {
                        VoiceError::new(format!("Unknown phonetic language {}", phone.language))
                    })?;
                    let language_id = map
                        .languages
                        .iter()
                        .position(|name| name == language.id())
                        .ok_or_else(|| {
                            VoiceError::new("Phonetic language absent from original map")
                        })?;
                    let row = map.encode(language.id(), &phone.token)?;

                    vowels.push(row[24] == 1.0 || row[25] == 1.0);
                    values.extend(row);
                    language_ids.push(language_id);
                }
                PhoneValues::Phonetic(values)
            }
        };
        Ok(FeaturePhones {
            language_ids,
            vowels,
            values,
        })
    }
    pub(super) fn phonetic(&self) -> bool {
        matches!(self.phone_encoder, PhoneEncoder::Phonetic(_))
    }
}
