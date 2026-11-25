//! Messages for the interface: a code (a key of `src/locales/*.json`) with named
//! parameters, never a finished sentence (specs/ui.md, "Languages"). The interface looks
//! the code up in the user's language.

use std::fmt;

use ks_utility::message::Message;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeMap, SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};

/// The code for a message saved by an older version as an English sentence: the text is
/// shown as it was saved.
pub const SAVED_TEXT: &str = "error.savedText";

/// A message code and its parameters; serialised as `{ "code": …, "params": { … } }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msg(pub Message);

/// `msg!("error.noSuchDownload")` or `msg!("error.createFile", detail = e)`.
#[macro_export]
macro_rules! msg {
    ($code:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::msg::Msg(ks_utility::message::Message::new($code)$(.with(stringify!($name), $value))*)
    };
}

impl Msg {
    #[cfg(test)]
    pub fn code(&self) -> &str {
        self.0.code()
    }
}

/// For logs only.
impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

struct Params<'a>(&'a [(String, String)]);

impl Serialize for Params<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in self.0 {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}

impl Serialize for Msg {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("Msg", 2)?;
        st.serialize_field("code", self.0.code())?;
        st.serialize_field("params", &Params(self.0.params()))?;
        st.end()
    }
}

/// Reads `{ code, params }`, or a plain string saved by an older version (kept as
/// [`SAVED_TEXT`] with the text as its parameter). An unknown or malformed code is kept
/// the same way, so a damaged list can never produce an invalid code.
impl<'de> Deserialize<'de> for Msg {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Msg;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a message code with parameters, or a text")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Msg, E> {
                Ok(saved_text(v))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Msg, A::Error> {
                let mut code: Option<String> = None;
                let mut params: Vec<(String, String)> = Vec::new();
                while let Some(k) = map.next_key::<String>()? {
                    match k.as_str() {
                        "code" => code = Some(map.next_value()?),
                        "params" => {
                            let p: std::collections::BTreeMap<String, String> = map.next_value()?;
                            params = p.into_iter().collect();
                        }
                        _ => {
                            let _: de::IgnoredAny = map.next_value()?;
                        }
                    }
                }
                let code = code.ok_or_else(|| de::Error::missing_field("code"))?;
                if !ks_utility::message::is_valid_code(&code)
                    || params.iter().any(|(n, _)| !ks_utility::message::is_valid_param(n))
                {
                    return Ok(saved_text(&code));
                }
                let mut m = Message::new(code);
                for (n, v) in params {
                    m = m.with(n, v);
                }
                Ok(Msg(m))
            }
        }
        d.deserialize_any(V)
    }
}

fn saved_text(text: &str) -> Msg {
    Msg(Message::new(SAVED_TEXT).with("text", text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_older_texts() {
        let m = msg!("error.createFile", detail = "disk full");
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"{"code":"error.createFile","params":{"detail":"disk full"}}"#);
        assert_eq!(serde_json::from_str::<Msg>(&json).unwrap(), m);
        let old: Msg = serde_json::from_str(r#""Stopped before it finished.""#).unwrap();
        assert_eq!((old.code(), old.0.param("text")), (SAVED_TEXT, Some("Stopped before it finished.")));
        let bad: Msg = serde_json::from_str(r#"{"code":"Not A Code","params":{}}"#).unwrap();
        assert_eq!(bad.code(), SAVED_TEXT);
        assert_eq!(msg!("error.noSuchDownload").to_string(), "error.noSuchDownload");
    }

    /// Every code Rust sends has text in both language files of the interface, with the
    /// parameters Rust fills in.
    #[test]
    fn every_code_has_text() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // The app's texts and ks-utility's shared catalogue (`common.*`; installed by `npm ci`).
        let read = |l: &str| -> serde_json::Map<String, serde_json::Value> {
            let mut all = serde_json::Map::new();
            for file in [format!("../src/locales/{l}.json"), format!("../node_modules/ks-utility/lang/common/{l}.json")] {
                let bytes = std::fs::read(dir.join(&file)).unwrap_or_else(|e| panic!("{file}: {e} (run npm install first)"));
                all.extend(serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&bytes).unwrap());
            }
            all
        };
        let (en, hi) = (read("en"), read("hi"));
        let mut codes = vec![(SAVED_TEXT.to_owned(), vec!["text".to_owned()])];
        for f in std::fs::read_dir(dir.join("src")).unwrap().flatten() {
            let text = std::fs::read_to_string(f.path()).unwrap();
            for part in text.split("msg!(\"").skip(1) {
                let code = &part[..part.find('"').unwrap()];
                let call = &part[..part.find(')').unwrap_or(part.len())];
                let params = call.split(", ").skip(1).filter_map(|p| p.split(" = ").next()).map(str::to_owned).collect();
                codes.push((code.to_owned(), params));
            }
        }
        assert!(codes.len() > 50, "found {} codes", codes.len());
        for (code, params) in codes {
            for (lang, file) in [("en", &en), ("hi", &hi)] {
                let text = file.get(&code).and_then(|v| v.as_str()).unwrap_or_else(|| panic!("{code} has no {lang} text"));
                for p in &params {
                    assert!(text.contains(&format!("{{{p}}}")), "{lang} {code} does not use {{{p}}}");
                }
            }
        }
    }
}
