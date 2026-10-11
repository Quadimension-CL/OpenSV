use super::{PreparedModule, PreparedTree};
use crate::synthesis::voice::{ResolvedPhone, Result, VoiceError};

impl PreparedTree {
    /// Keep the language table child order, separate from the unified
    /// table. Configuration's supported-language display order is not this order.
    pub fn resolve_phone(&self, language: &str, token: &str) -> Result<ResolvedPhone<'_>> {
        let tables: Vec<_> = self
            .nodes
            .iter()
            .filter_map(|node| match &node.module {
                PreparedModule::PhoneSet(table) => Some(table),
                _ => None,
            })
            .collect();
        let mut unified = tables.iter().filter(|table| table.name == "multiphone");
        let unified_table = *unified
            .next()
            .ok_or_else(|| VoiceError::new("Model has no unified phone table"))?;
        if unified.next().is_some() {
            return Err(VoiceError::new(
                "Model has multiple unified phone tables; routing is unresolved",
            ));
        }
        let languages: Vec<_> = tables
            .into_iter()
            .filter(|table| table.name != "multiphone")
            .collect();
        crate::synthesis::voice::resolve_explicit(&languages, unified_table, language, token)
    }
}
