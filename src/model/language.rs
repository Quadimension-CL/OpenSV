use serde_json::Value;

/// SVP language IDs and the corresponding voice configuration phone sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    English,
    Mandarin,
    Japanese,
    Spanish,
    Cantonese,
    Korean,
}

impl Language {
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Mandarin,
        Self::Japanese,
        Self::Spanish,
        Self::Cantonese,
        Self::Korean,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::English => "english",
            Self::Mandarin => "mandarin",
            Self::Japanese => "japanese",
            Self::Spanish => "spanish",
            Self::Cantonese => "cantonese",
            Self::Korean => "korean",
        }
    }

    pub fn phone_set(self) -> &'static str {
        match self {
            Self::English => "english-arpabet",
            Self::Mandarin => "mandarin-xsampa",
            Self::Japanese => "japanese-romaji",
            Self::Spanish => "spanish-xsampa",
            Self::Cantonese => "cantonese-xsampa",
            Self::Korean => "korean-xsampa",
        }
    }

    /// SVP stores the phonetic alphabet separately from the language ID.
    pub fn phoneme_alphabet(self) -> &'static str {
        match self {
            Self::English => "arpabet",
            Self::Japanese => "romaji",
            Self::Mandarin | Self::Spanish | Self::Cantonese | Self::Korean => "xsampa",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::English => "英语",
            Self::Mandarin => "汉语普通话",
            Self::Japanese => "日语",
            Self::Spanish => "西班牙语",
            Self::Cantonese => "汉语广东话",
            Self::Korean => "韩语",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| value == language.id() || value == language.phone_set())
    }
}

/// Apply one database scope over its inherited language. Both fields are SVP
/// fields: `language` describes the bank, `languageOverride` selects cross-language singing.
pub fn database_language(database: &Value, inherited: Language) -> Result<Language, String> {
    if !database.is_object() {
        return Err("歌声的 database 字段必须是对象".into());
    }
    let mut selected = inherited;
    for field in ["language", "languageOverride"] {
        if let Some(value) = database.get(field) {
            let value = value
                .as_str()
                .ok_or_else(|| format!("database.{field} 必须是字符串"))?;
            if !value.is_empty() {
                selected =
                    Language::parse(value).ok_or_else(|| format!("不支持的歌声语言：{value}"))?;
            }
        }
    }
    Ok(selected)
}
