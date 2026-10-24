mod templates;
use std::{collections::BTreeMap,sync::{OnceLock,atomic::{AtomicUsize,Ordering}}};
use serde::{Deserialize,Serialize};

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,Serialize,Deserialize)]
pub enum Language {
    #[serde(rename="en-us")] English,
    #[default] #[serde(rename="zh-cn")] SimplifiedChinese,
    #[serde(rename="zh-tw")] TraditionalChinese,
    #[serde(rename="ja-jp")] Japanese,
    #[serde(rename="ko-kr")] Korean,
    #[serde(rename="de-de")] German,
    #[serde(rename="es-la")] Spanish,
    #[serde(rename="fr-fr")] French,
    #[serde(rename="pt-br")] Portuguese,
    #[serde(rename="ru-ru")] Russian,
    #[serde(rename="vi-vn")] Vietnamese,
}
impl Language {
    pub const ALL:[Self;11]=[Self::English,Self::SimplifiedChinese,Self::TraditionalChinese,Self::Japanese,Self::Korean,Self::German,Self::Spanish,Self::French,Self::Portuguese,Self::Russian,Self::Vietnamese];
    pub fn key(self)->&'static str {match self {
        Self::English=>"en-us",Self::SimplifiedChinese=>"zh-cn",Self::TraditionalChinese=>"zh-tw",Self::Japanese=>"ja-jp",Self::Korean=>"ko-kr",Self::German=>"de-de",Self::Spanish=>"es-la",Self::French=>"fr-fr",Self::Portuguese=>"pt-br",Self::Russian=>"ru-ru",Self::Vietnamese=>"vi-vn",
    }}
    pub fn name(self)->&'static str {match self {
        Self::English=>"English",Self::SimplifiedChinese=>"简体中文",Self::TraditionalChinese=>"繁體中文",Self::Japanese=>"日本語",Self::Korean=>"한국어",Self::German=>"Deutsch",Self::Spanish=>"Español",Self::French=>"Français",Self::Portuguese=>"Português",Self::Russian=>"Русский",Self::Vietnamese=>"Tiếng Việt",
    }}
}
#[derive(Deserialize)]
struct Catalogue {keys:BTreeMap<String,String>,locales:BTreeMap<String,BTreeMap<String,String>>}
static CATALOGUE:OnceLock<Catalogue>=OnceLock::new();
static LANGUAGE:AtomicUsize=AtomicUsize::new(1);
fn catalogue()->&'static Catalogue {CATALOGUE.get_or_init(||serde_json::from_str(include_str!("../../assets/locales/catalogue.json")).expect("validated embedded editor translations"))}
pub fn set_language(value:Language) {LANGUAGE.store(value as usize,Ordering::Relaxed);}
pub fn language()->Language {Language::ALL[LANGUAGE.load(Ordering::Relaxed)]}
pub fn t(text:&str)->&str {
    let data=catalogue();let key=data.keys.get(text).map_or(text,String::as_str);
    if let Some(value)=data.locales.get(language().key()).and_then(|m|m.get(key)) {return value;}
    if language()==Language::SimplifiedChinese && data.keys.contains_key(text) {return text;}
    key
}
pub fn format(source:&str,rendered:String)->String {
    let exact=t(&rendered);
    if exact!=rendered {return exact.into();}
    let translated=t(source);
    if translated==source {return rendered;}
    templates::translate(source,translated,&rendered).unwrap_or(rendered)
}

pub fn message(text:&str)->String {
    if text.is_empty() {return String::new();}
    let exact=t(text);if exact!=text {return exact.into();}
    for source in catalogue().keys.keys().filter(|s|s.contains('{')) {
        let translated=t(source);
        if translated!=source && let Some(value)=templates::translate(source,translated,text) {return value;}
    }
    text.into()
}
#[macro_export]
macro_rules! lformat {
    ($template:literal $($arguments:tt)*)=>{
        $crate::i18n::format($template,format!($template $($arguments)*))
    };
}
