//! localswim's service receipt (operator.md, The receipt).

use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, Visitor};
use serde_json::Value;
use std::fmt;

/// What the push takes from the receipt. Neither member is secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// `base`, after JSON unescaping, or `None` when the receipt has no `base`. A missing or
    /// empty one is refused later, with the token's own checks.
    pub base: Option<String>,
    /// `credential_directory`: a string, not empty.
    pub credential_directory: String,
}

/// The two members, each as many times as it was given, in any ASCII case.
#[derive(Default)]
struct Members {
    base: Vec<Value>,
    credential_directory: Vec<Value>,
}

struct TopLevel;

impl<'de> DeserializeSeed<'de> for TopLevel {
    type Value = Members;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Members, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for TopLevel {
    type Value = Members;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Members, A::Error> {
        let mut m = Members::default();
        while let Some(key) = map.next_key::<String>()? {
            if key.eq_ignore_ascii_case("base") {
                m.base.push(map.next_value::<Value>()?);
            } else if key.eq_ignore_ascii_case("credential_directory") {
                m.credential_directory.push(map.next_value::<Value>()?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(m)
    }
}

/// Reads the receipt's bytes under The receipt's rules. `None` is a refusal, which gives the
/// "Could not read" warning; it carries no reason, since no warning may quote one.
pub fn parse_receipt(bytes: &[u8]) -> Option<Receipt> {
    let text = crate::text::decode_marked(bytes)?;
    let mut de = serde_json::Deserializer::from_str(&text);
    let members = TopLevel.deserialize(&mut de).ok()?;
    de.end().ok()?;

    if members.base.len() > 1 || members.credential_directory.len() != 1 {
        return None;
    }
    let credential_directory = match members.credential_directory.into_iter().next() {
        Some(Value::String(s)) if !s.is_empty() => s,
        _ => return None,
    };
    let base = match members.base.into_iter().next() {
        None => None,
        Some(Value::String(s)) => Some(s),
        Some(_) => return None,
    };
    Some(Receipt {
        base,
        credential_directory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> Receipt {
        parse_receipt(text.as_bytes()).expect("Accepted")
    }

    #[test]
    fn reads_both_members_in_any_ascii_case() {
        let r = ok(
            r#" { "BASE" : "http://127.0.0.1:8811", "Credential_Directory": "C:\\x", "exe": 1 } "#,
        );
        assert_eq!(r.base.as_deref(), Some("http://127.0.0.1:8811"));
        assert_eq!(r.credential_directory, "C:\\x");
    }

    #[test]
    fn base_may_be_missing_here() {
        assert_eq!(ok(r#"{"credential_directory":"d"}"#).base, None);
    }

    #[test]
    fn refusals() {
        for text in [
            "",
            "   ",
            "[]",
            "\"x\"",
            "null",
            r#"{"credential_directory":"d",}"#,
            r#"{"credential_directory":"d"} // c"#,
            r#"{'credential_directory':"d"}"#,
            r#"{credential_directory:"d"}"#,
            r#"{"base":"u"}"#,
            r#"{"credential_directory":""}"#,
            r#"{"credential_directory":1}"#,
            r#"{"credential_directory":"d","base":null}"#,
            r#"{"credential_directory":"d","base":1}"#,
            r#"{"credential_directory":"d","base":"u","BASE":"u"}"#,
            r#"{"credential_directory":"d","CREDENTIAL_DIRECTORY":"d"}"#,
            r#"{"credential_directory":"d"} {}"#,
            r#"{"credential_directory":"\ud800"}"#,
        ] {
            assert_eq!(parse_receipt(text.as_bytes()), None, "{text}");
        }
    }

    #[test]
    fn other_members_are_ignored_whatever_they_hold() {
        let r = ok(r#"{"x":1,"x":[{"a":1,"a":2}],"credential_directory":"d","base":"u"}"#);
        assert_eq!(r.credential_directory, "d");
    }
}
