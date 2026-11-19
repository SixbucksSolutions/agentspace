//! What the keeper's JSON files share: the embedded schemas and their validator, the check for a
//! member named twice, and the bytes a writer produces (state.md, The keeper's files).

use crate::NotValid;
use jsonschema::Validator;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fmt;
use std::sync::OnceLock;

/// The three schemas, embedded at build time. The validator is draft 2020-12 and fetches nothing:
/// `jsonschema` is built without `resolve-http` and `resolve-file`, and each schema is
/// self-contained (D38).
const HOLDER: &str = include_str!("../../../schemas/state/board-holder.state.schema.json");
const LAUNCH: &str = include_str!("../../../schemas/state/board-holder-launch.state.schema.json");
const RESULT: &str =
    include_str!("../../../schemas/state/board-holder-launch-result.state.schema.json");

/// Which schema a document is checked against.
#[derive(Clone, Copy)]
pub(crate) enum Schema {
    Holder = 0,
    Launch = 1,
    Result = 2,
}

fn build(text: &str) -> Option<Validator> {
    let schema: Value = serde_json::from_str(text).ok()?;
    jsonschema::draft202012::new(&schema).ok()
}

fn validator(which: Schema) -> Option<&'static Validator> {
    static VALIDATORS: OnceLock<[Option<Validator>; 3]> = OnceLock::new();
    let all = VALIDATORS.get_or_init(|| [build(HOLDER), build(LAUNCH), build(RESULT)]);
    all[which as usize].as_ref()
}

/// Checks a document against a schema. A schema that could not be built refuses everything,
/// which would be a defect that a unit test catches.
pub(crate) fn is_valid(which: Schema, doc: &Value) -> bool {
    validator(which).is_some_and(|v| v.is_valid(doc))
}

/// A reason a document is not valid, in words that name no value, only where in it.
pub(crate) fn why_not_valid(which: Schema, doc: &Value, what: &str) -> NotValid {
    let at = validator(which)
        .and_then(|v| v.iter_errors(doc).next())
        .map(|e| e.instance_path().to_string())
        .filter(|p| !p.is_empty());
    NotValid(match at {
        Some(p) => format!("{what} is not valid at {p}"),
        None => format!("{what} is not valid"),
    })
}

/// A pass over a JSON text that fails on an object naming one member twice, at any depth. A JSON
/// parser keeps one of the two values silently, so this check MUST run beside it.
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

/// Reads a file's bytes as one JSON value in which no object names a member twice. A leading
/// UTF-8 byte order mark and JSON whitespace are accepted on read, though a writer never writes
/// them. `None` for anything else.
pub(crate) fn read_value(bytes: &[u8]) -> Option<Value> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).ok()?;
    let mut de = serde_json::Deserializer::from_str(text);
    NoRepeatedMember.deserialize(&mut de).ok()?;
    de.end().ok()?;
    serde_json::from_str(text).ok()
}

/// A file's bytes as a writer MUST write them: UTF-8 without a byte order mark, compact JSON with
/// no whitespace outside strings, then one LF.
pub(crate) fn file_bytes(doc: &Value) -> Result<Vec<u8>, NotValid> {
    let mut bytes = serde_json::to_vec(doc)
        .map_err(|_| NotValid("the file could not be serialized".to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// A JSON object from members in the order given.
pub(crate) fn object<const N: usize>(members: [(&str, Value); N]) -> Value {
    let mut map = Map::new();
    for (key, value) in members {
        map.insert(key.to_string(), value);
    }
    Value::Object(map)
}

/// Whether `s` is a time of the form `YYYY-MM-DDTHH:MM:SS.` then `fraction` digits then `Z`, with
/// the ranges the schemas' patterns give (a day of 01 to 31, not checked against the month).
pub(crate) fn is_time(s: &str, fraction: usize) -> bool {
    let b = s.as_bytes();
    if b.len() != 20 + fraction + 1 {
        return false;
    }
    let digits = |from: usize, to: usize| b[from..to].iter().all(u8::is_ascii_digit);
    let num = |from: usize| u32::from(b[from] - b'0') * 10 + u32::from(b[from + 1] - b'0');
    digits(0, 4)
        && b[4] == b'-'
        && digits(5, 7)
        && (1..=12).contains(&num(5))
        && b[7] == b'-'
        && digits(8, 10)
        && (1..=31).contains(&num(8))
        && b[10] == b'T'
        && digits(11, 13)
        && num(11) <= 23
        && b[13] == b':'
        && digits(14, 16)
        && num(14) <= 59
        && b[16] == b':'
        && digits(17, 19)
        && num(17) <= 59
        && b[19] == b'.'
        && digits(20, 20 + fraction)
        && b[20 + fraction] == b'Z'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_schemas_build_with_the_validator() {
        for (name, text) in [("holder", HOLDER), ("launch", LAUNCH), ("result", RESULT)] {
            assert!(build(text).is_some(), "Schema {name} must build");
        }
    }

    #[test]
    fn a_member_named_twice_is_not_read() {
        assert!(read_value(br#"{"a":1,"a":2}"#).is_none());
        assert!(read_value(br#"{"a":1,"b":{"c":1,"c":2}}"#).is_none());
        assert!(read_value(br#"{"a":1,"b":2}"#).is_some());
        assert!(read_value(br#"{"a":1} {"b":2}"#).is_none());
        assert!(read_value(b"\xEF\xBB\xBF{}").is_some());
        assert!(read_value(b"\xFF").is_none());
    }

    #[test]
    fn times_follow_the_patterns() {
        assert!(is_time("2026-11-19T04:05:06.123Z", 3));
        assert!(is_time("2026-11-19T04:05:06.1234567Z", 7));
        assert!(!is_time("2026-11-19T04:05:06.123Z", 7));
        assert!(!is_time("2026-13-19T04:05:06.123Z", 3));
        assert!(!is_time("2026-00-19T04:05:06.123Z", 3));
        assert!(!is_time("2026-11-32T04:05:06.123Z", 3));
        assert!(!is_time("2026-11-19T24:05:06.123Z", 3));
        assert!(!is_time("2026-11-19T04:60:06.123Z", 3));
        assert!(!is_time("2026-11-19T04:05:60.123Z", 3));
        assert!(!is_time("2026-11-19T04:05:06.123", 3));
        assert!(!is_time("2026-11-19T04:05:06,123Z", 3));
        assert!(!is_time("2026-11-19T04:05:06.123Z\n", 3));
        assert!(!is_time("", 3));
    }
}
