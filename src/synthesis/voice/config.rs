use std::collections::BTreeMap;

use serde::Serialize;

use super::{Result, VoiceError, VoiceFamily, archive::ResourceId};

pub(super) const CONFIGURATION_ID: ResourceId = ResourceId([
    0x21, 0x19, 0x4a, 0xe9, 0x56, 0x77, 0x3e, 0x78, 0x8d, 0x0e, 0x8c, 0xe2, 0xd3, 0x26, 0xbc, 0x44,
]);

#[derive(Serialize)]
pub struct VoiceConfiguration {
    pub family: VoiceFamily,
    pub model_version: u8,
    pub f0_enhancement: Option<f32>,
    pub name: String,
    pub version: String,
    pub vendor: String,
    pub default_language: String,
    pub supported_languages: Vec<String>,
    pub styles: Vec<String>,
    pub additional_fields: usize,
    pub parser_scope: &'static str,
    #[serde(skip)]
    _uninterpreted: BTreeMap<String, String>,
}

impl VoiceConfiguration {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| VoiceError::new("Voice configuration is not UTF-8"))?;
        let mut fields = BTreeMap::new();
        // Both model generations use `key value\n`. Values are kept literally;
        // reject duplicate keys so configuration values remain unambiguous.
        for line in text.lines() {
            let (key, value) = line
                .split_once(' ')
                .filter(|(key, value)| !key.is_empty() && !value.is_empty())
                .ok_or_else(|| VoiceError::new("Unsupported voice configuration line grammar"))?;
            if fields.insert(key.to_owned(), value.to_owned()).is_some() {
                return Err(VoiceError::new(
                    "Duplicate voice configuration field is unsupported",
                ));
            }
        }
        let model_version = match fields.remove("model-version").as_deref() {
            None | Some("1") => 1,
            Some("2") => 2,
            _ => return Err(VoiceError::new("Unsupported voice model-version")),
        };
        let name = take(&mut fields, "name")?;
        let version = take(&mut fields, "version")?;
        let vendor = take(&mut fields, "vendor")?;
        let default_language = take(&mut fields, "default_language")?;
        let supported_languages = take(&mut fields, "supported_languages")?
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let styles = take(&mut fields, "style_config")?
            .split(',')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if !supported_languages.contains(&default_language) || styles.iter().any(String::is_empty) {
            return Err(VoiceError::new("Invalid voice language or style metadata"));
        }
        if styles.iter().any(|style| style.contains('=')) {
            return Err(VoiceError::new(
                "Extended voice style configuration grammar is not implemented",
            ));
        }
        Ok(Self {
            family: if model_version == 1 {
                VoiceFamily::Sv2Compatible
            } else {
                VoiceFamily::Sv2
            },
            model_version,
            f0_enhancement: None,
            name,
            version,
            vendor,
            default_language,
            supported_languages,
            styles,
            additional_fields: fields.len(),
            parser_scope: "observed model-version 1/2 key-space-value grammar; extended style grammar and duplicate policy not recovered",
            _uninterpreted: fields,
        })
    }

    pub(super) fn from_nofs(mut fields: BTreeMap<String, String>) -> Result<Self> {
        if take(&mut fields, ".type")? != "mu" {
            return Err(VoiceError::new(
                "暂不支持该 SV1 声库类型，仅支持 DDS AI 声库",
            ));
        }
        if take(&mut fields, "f0model-configs")? != "_ _ _ _ _ _ _ _" {
            return Err(VoiceError::new("暂不支持该 SV1 声库的自定义 F0 模型配置"));
        }
        let name = take(&mut fields, ".name")?;
        let version = take(&mut fields, ".version")?;
        let vendor = take(&mut fields, ".vendor")?;
        let default_language = take(&mut fields, ".language")?;
        let supported_languages = match fields.remove(".multi") {
            Some(value) => value.split_whitespace().map(str::to_owned).collect(),
            None => vec![default_language.clone()],
        };
        let styles = fields
            .remove(".timbre_styles")
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let f0_enhancement = fields
            .remove(".feature_f0_enhancement")
            .map(|value| {
                if value.is_empty() {
                    Ok(1.0)
                } else {
                    value
                        .parse::<f32>()
                        .map_err(|_| VoiceError::new("SV1 F0 enhancement is invalid"))
                }
            })
            .transpose()?;
        if name.is_empty()
            || vendor.is_empty()
            || version.parse::<u32>().is_err()
            || !supported_languages.contains(&default_language)
            || f0_enhancement.is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(VoiceError::new("SV1 voice configuration is invalid"));
        }
        Ok(Self {
            family: VoiceFamily::Sv1,
            model_version: 1,
            f0_enhancement,
            name,
            version,
            vendor,
            default_language,
            supported_languages,
            styles,
            additional_fields: fields.len(),
            parser_scope: "SV1 AI NOFS public metadata; DDS model family",
            _uninterpreted: fields,
        })
    }
}

fn take(fields: &mut BTreeMap<String, String>, key: &str) -> Result<String> {
    fields
        .remove(key)
        .ok_or_else(|| VoiceError::new(format!("Missing voice configuration field: {key}")))
}
