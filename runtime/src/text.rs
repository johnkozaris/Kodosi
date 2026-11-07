//! Words that people read.
//!
//! A sentence marked with `text!` goes to each app as data and becomes words in the language of
//! that app; the `kodosi` command, the host and the logs use English. Other words, such as the
//! detail of an internal fault, stay English everywhere.

use std::{
    borrow::Cow,
    collections::HashMap,
    fmt,
    sync::{
        OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _, ser::SerializeMap};
use serde_json::{Map, Value};

/// Marks a sentence for people. Each value fills the `{name}` of the same name.
#[macro_export]
macro_rules! text {
    ($words:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::text::Text::translated(
            concat!($words),
            vec![$((stringify!($name), $crate::text::Arg::from($value))),*],
        )
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Language {
    #[default]
    English = 0,
    German = 1,
    Greek = 2,
    Spanish = 3,
    French = 4,
    Italian = 5,
    Japanese = 6,
    Korean = 7,
    Chinese = 8,
}

impl Language {
    pub const ALL: [Self; 9] = [
        Self::English,
        Self::German,
        Self::Greek,
        Self::Spanish,
        Self::French,
        Self::Italian,
        Self::Japanese,
        Self::Korean,
        Self::Chinese,
    ];

    pub const fn tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::German => "de",
            Self::Greek => "el",
            Self::Spanish => "es",
            Self::French => "fr",
            Self::Italian => "it",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::Chinese => "zh-Hans",
        }
    }

    /// The language of a tag such as `de`, `de-AT`, `zh-Hans` or `zh_CN`; English for others.
    pub fn of(tag: &str) -> Self {
        let primary = tag.split(['-', '_']).next().unwrap_or_default();
        Self::ALL
            .into_iter()
            .find(|language| {
                language
                    .tag()
                    .split('-')
                    .next()
                    .is_some_and(|own| own.eq_ignore_ascii_case(primary))
            })
            .unwrap_or_default()
    }

    fn table(self) -> Option<&'static HashMap<String, String>> {
        static TABLES: [OnceLock<HashMap<String, String>>; 9] = [const { OnceLock::new() }; 9];
        let source = self.source()?;
        Some(TABLES[self as usize].get_or_init(|| serde_json::from_str(source).unwrap_or_default()))
    }

    fn source(self) -> Option<&'static str> {
        Some(match self {
            Self::English => return None,
            Self::German => include_str!("../texts/de.json"),
            Self::Greek => include_str!("../texts/el.json"),
            Self::Spanish => include_str!("../texts/es.json"),
            Self::French => include_str!("../texts/fr.json"),
            Self::Italian => include_str!("../texts/it.json"),
            Self::Japanese => include_str!("../texts/ja.json"),
            Self::Korean => include_str!("../texts/ko.json"),
            Self::Chinese => include_str!("../texts/zh-Hans.json"),
        })
    }
}

static LANGUAGE: AtomicU8 = AtomicU8::new(Language::English as u8);

/// Sets the language of the words that this process gives to people. Only an app sets it.
pub fn set_language(language: Language) {
    LANGUAGE.store(language as u8, Ordering::Relaxed);
}

pub fn language() -> Language {
    Language::ALL[usize::from(LANGUAGE.load(Ordering::Relaxed))]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    words: Cow<'static, str>,
    args: Vec<(Cow<'static, str>, Arg)>,
    translated: bool,
}

/// A value in a sentence: words as they are, or a sentence in the same language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    Words(String),
    Text(Text),
}

impl Text {
    pub fn translated(words: &'static str, args: Vec<(&'static str, Arg)>) -> Self {
        Self {
            words: Cow::Borrowed(words),
            args: args
                .into_iter()
                .map(|(name, value)| (Cow::Borrowed(name), value))
                .collect(),
            translated: true,
        }
    }

    /// Words that stay English, such as the detail of an internal fault.
    pub fn plain(words: impl Into<Cow<'static, str>>) -> Self {
        Self {
            words: words.into(),
            args: Vec::new(),
            translated: false,
        }
    }

    /// The sentence before its values, as the code writes it.
    pub fn words(&self) -> &str {
        &self.words
    }

    pub fn in_language(&self, language: Language) -> String {
        if !self.translated {
            return match language {
                Language::English => self.words.to_string(),
                _ => format!(
                    "{} ({})",
                    text!("Kodosi had an error.").in_language(language),
                    self.words
                ),
            };
        }
        let words = language
            .table()
            .and_then(|table| table.get(self.words.as_ref()))
            .map_or_else(|| self.words.as_ref(), String::as_str);
        fill(words, &self.args, language)
    }

    fn from_value(value: Value) -> Option<Self> {
        match value {
            Value::String(words) => Some(Self::plain(words)),
            Value::Object(mut object) => {
                if let Some(Value::String(words)) = object.remove("$fault") {
                    return Some(Self::plain(words));
                }
                let Some(Value::String(words)) = object.remove("$text") else {
                    return None;
                };
                let args = match object.remove("args") {
                    None => Vec::new(),
                    Some(Value::Object(args)) => args
                        .into_iter()
                        .map(|(name, value)| Some((Cow::Owned(name), Arg::from_value(value)?)))
                        .collect::<Option<_>>()?,
                    Some(_) => return None,
                };
                Some(Self {
                    words: Cow::Owned(words),
                    args,
                    translated: true,
                })
            }
            _ => None,
        }
    }
}

fn fill(words: &str, args: &[(Cow<'static, str>, Arg)], language: Language) -> String {
    let mut filled = String::with_capacity(words.len());
    let mut rest = words;
    while let Some((before, after)) = rest.split_once('{')
        && let Some((name, next)) = after.split_once('}')
    {
        filled.push_str(before);
        match args.iter().find(|(own, _)| own == name) {
            Some((_, Arg::Words(value))) => filled.push_str(value),
            Some((_, Arg::Text(value))) if !value.translated => filled.push_str(&value.words),
            Some((_, Arg::Text(value))) => filled.push_str(&value.in_language(language)),
            None => filled.push_str(&rest[before.len()..rest.len() - next.len()]),
        }
        rest = next;
    }
    filled.push_str(rest);
    filled
}

/// Whether `value` holds a text that still needs its words.
pub fn has_text(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object.contains_key("$text")
                || object.contains_key("$fault")
                || object.values().any(has_text)
        }
        Value::Array(items) => items.iter().any(has_text),
        _ => false,
    }
}

/// Puts each text in `value` into words of `language`.
pub fn render(value: &mut Value, language: Language) {
    match value {
        Value::Object(object) if object.contains_key("$text") || object.contains_key("$fault") => {
            if let Some(text) = Text::from_value(value.take()) {
                *value = Value::String(text.in_language(language));
            }
        }
        Value::Object(object) => object.values_mut().for_each(|item| render(item, language)),
        Value::Array(items) => items.iter_mut().for_each(|item| render(item, language)),
        _ => {}
    }
}

impl fmt::Display for Text {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.in_language(Language::English))
    }
}

impl From<String> for Text {
    fn from(words: String) -> Self {
        Self::plain(words)
    }
}

impl From<&'static str> for Text {
    fn from(words: &'static str) -> Self {
        Self::plain(words)
    }
}

impl PartialEq<&str> for Text {
    fn eq(&self, other: &&str) -> bool {
        self.in_language(Language::English) == *other
    }
}

impl Arg {
    fn from_value(value: Value) -> Option<Self> {
        match value {
            Value::String(words) => Some(Self::Words(words)),
            value => Text::from_value(value).map(Self::Text),
        }
    }
}

impl From<Text> for Arg {
    fn from(text: Text) -> Self {
        Self::Text(text)
    }
}

impl From<&Text> for Arg {
    fn from(text: &Text) -> Self {
        Self::Text(text.clone())
    }
}

impl From<String> for Arg {
    fn from(words: String) -> Self {
        Self::Words(words)
    }
}

impl From<&String> for Arg {
    fn from(words: &String) -> Self {
        Self::Words(words.clone())
    }
}

impl From<&str> for Arg {
    fn from(words: &str) -> Self {
        Self::Words(words.to_owned())
    }
}

impl From<&&str> for Arg {
    fn from(words: &&str) -> Self {
        Self::Words((*words).to_owned())
    }
}

macro_rules! numbers {
    ($($number:ty),*) => {$(
        impl From<$number> for Arg {
            fn from(number: $number) -> Self {
                Self::Words(number.to_string())
            }
        }
    )*};
}
numbers!(u8, u16, u32, u64, usize, i32, i64);

impl Serialize for Text {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(if self.translated { 2 } else { 1 }))?;
        if !self.translated {
            map.serialize_entry("$fault", &self.words)?;
            return map.end();
        }
        map.serialize_entry("$text", &self.words)?;
        let args: Map<String, Value> = self
            .args
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    Arg::Words(words) => Value::String(words.clone()),
                    Arg::Text(text) if !text.translated => Value::String(text.words.to_string()),
                    Arg::Text(text) => serde_json::to_value(text).unwrap_or_default(),
                };
                (name.to_string(), value)
            })
            .collect();
        map.serialize_entry("args", &args)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_value(Value::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("words must be text"))
    }
}

impl specta::Type for Text {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <String as specta::Type>::definition(types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_language_table_reads_and_keeps_the_values_of_each_sentence() {
        let names = regex_free_names;
        for language in Language::ALL.into_iter().skip(1) {
            let source = language.source().unwrap();
            let table: HashMap<String, String> = serde_json::from_str(source).unwrap();
            for (english, words) in &table {
                assert_eq!(
                    names(english),
                    names(words),
                    "{}: {english}",
                    language.tag()
                );
            }
        }
    }

    fn regex_free_names(words: &str) -> Vec<&str> {
        let mut names: Vec<&str> = words
            .split('{')
            .skip(1)
            .filter_map(|part| part.split_once('}').map(|(name, _)| name))
            .collect();
        names.sort_unstable();
        names
    }

    #[test]
    fn a_text_travels_as_data_and_becomes_words_in_each_language() {
        let text = text!(
            "{name} is offline. The terminal closes when it is back.",
            name = "studio"
        );
        let mut event = serde_json::json!({"type":"session.error","message":text});
        assert_eq!(
            event["message"],
            serde_json::json!({"$text":"{name} is offline. The terminal closes when it is back.","args":{"name":"studio"}})
        );
        render(&mut event, Language::English);
        assert_eq!(
            event["message"],
            "studio is offline. The terminal closes when it is back."
        );
    }

    #[test]
    fn a_fault_keeps_its_english_detail_after_a_general_sentence() {
        let fault = Text::plain("Terminal frame has an invalid body.");
        assert_eq!(fault.to_string(), "Terminal frame has an invalid body.");
        let mut event = serde_json::json!({"message":fault});
        assert_eq!(
            event["message"],
            serde_json::json!({"$fault":"Terminal frame has an invalid body."})
        );
        render(&mut event, Language::German);
        let words = event["message"].as_str().unwrap();
        assert!(
            words.ends_with(" (Terminal frame has an invalid body.)"),
            "{words}"
        );
    }

    #[test]
    fn a_sentence_inside_a_sentence_takes_the_same_language() {
        let inner = text!("Approve this device from one of your existing devices.");
        let outer = text!(
            "{reason} Approve this device from another device, or start fresh here.",
            reason = inner
        );
        let travelled: Text =
            serde_json::from_value(serde_json::to_value(&outer).unwrap()).unwrap();
        assert_eq!(travelled, outer);
        assert_eq!(
            travelled.to_string(),
            "Approve this device from one of your existing devices. Approve this device from another device, or start fresh here."
        );
    }

    #[test]
    fn a_brace_without_its_end_stays_as_it_is() {
        let text = Text::translated("Use {name} or a { alone", vec![("name", Arg::from("x"))]);
        assert_eq!(text.to_string(), "Use x or a { alone");
    }

    #[test]
    fn a_fault_inside_a_sentence_keeps_its_words_before_and_after_it_travels() {
        let text = text!(
            "{reason} Go back to Kodosi and try again.",
            reason = Text::plain("The code expired.")
        );
        let travelled: Text = serde_json::from_value(serde_json::to_value(&text).unwrap()).unwrap();
        for text in [text, travelled] {
            assert!(
                text.in_language(Language::German)
                    .starts_with("The code expired. "),
                "{}",
                text.in_language(Language::German)
            );
        }
    }

    #[test]
    fn tags_name_their_language() {
        assert_eq!(Language::of("de-AT"), Language::German);
        assert_eq!(Language::of("zh-Hans"), Language::Chinese);
        assert_eq!(Language::of("zh_CN"), Language::Chinese);
        assert_eq!(Language::of("el"), Language::Greek);
        assert_eq!(Language::of("pt-BR"), Language::English);
    }
}
