//! Reading a file's bytes as one JSON value in which no object names a member twice (docs/state.md,
//! The host registry and The project list: a JSON parser keeps one of two values silently, so this
//! check runs beside it).

use crate::error::Invalid;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;

/// A pass over a JSON text that fails on an object naming one member twice, at any depth. Member
/// names are compared after their escapes are decoded.
struct NoRepeatedMember;

impl<'de> DeserializeSeed<'de> for NoRepeatedMember {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for NoRepeatedMember {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element_seed(NoRepeatedMember)?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key) {
                return Err(de::Error::custom("a member is named twice"));
            }
            map.next_value_seed(NoRepeatedMember)?;
        }
        Ok(())
    }
}

/// Whether the text is one JSON value in which no object names a member twice.
fn has_no_repeated_member(text: &str) -> bool {
    let mut de = serde_json::Deserializer::from_str(text);
    NoRepeatedMember.deserialize(&mut de).is_ok() && de.end().is_ok()
}

/// Reads a file's bytes as one JSON value. A leading UTF-8 byte order mark and any JSON whitespace
/// are accepted (docs/state.md: read tolerantly, D20).
pub fn parse(bytes: &[u8]) -> Result<Value, Invalid> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| Invalid::NotUtf8)?;
    let doc: Value = serde_json::from_str(text).map_err(|_| Invalid::NotJson)?;
    if !has_no_repeated_member(text) {
        return Err(Invalid::RepeatedMember);
    }
    Ok(doc)
}

/// One string as a JSON string literal.
pub fn quote(s: &str) -> String {
    // Serializing a string cannot fail; an empty literal is the safe fallback.
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeated_member_is_refused_at_any_depth_even_with_equal_values() {
        for text in [
            r#"{"a":1,"a":1}"#,
            r#"{"a":[{"b":1,"b":2}]}"#,
            r#"{"a":1,"a":2}"#,
        ] {
            assert_eq!(
                parse(text.as_bytes()).unwrap_err(),
                Invalid::RepeatedMember,
                "{text}"
            );
        }
    }

    #[test]
    fn a_byte_order_mark_and_whitespace_are_accepted_and_bad_bytes_are_not() {
        assert!(parse(b"\xEF\xBB\xBF \r\n{ \"a\" : 1 }\n\t").is_ok());
        assert_eq!(parse(b"{\"a\":\"\xFF\"}").unwrap_err(), Invalid::NotUtf8);
        assert_eq!(parse(b"{").unwrap_err(), Invalid::NotJson);
        assert_eq!(parse(b"{}{}").unwrap_err(), Invalid::NotJson);
        // Only one byte order mark is allowed.
        assert_eq!(
            parse(b"\xEF\xBB\xBF\xEF\xBB\xBF{}").unwrap_err(),
            Invalid::NotJson
        );
    }
}
