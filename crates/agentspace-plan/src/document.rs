//! From bytes to a document of the plan schema's shape: checks 1 and 3 to 6 of lease-api.md's
//! table (Plan). Check 2, reading the file, is the lease CLI's, since this crate reads no file.

use jsonschema::Validator;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt;
use std::sync::OnceLock;

/// The most bytes a plan file holds, a byte order mark counted (lease-api.md, Plan; pin P1).
pub const MAX_FILE_BYTES: usize = 262_144;

/// Check 1's `<why>`.
pub const WHY_TOO_LARGE: &str = "the file is larger than 262144 bytes";
/// Check 2's `<why>`, which the lease CLI gives for a file it cannot read; never given here.
pub const WHY_UNREADABLE: &str = "the file could not be read";
pub(crate) const WHY_NOT_UTF8: &str = "the file is not valid UTF-8";
pub(crate) const WHY_NOT_JSON: &str = "the file is not valid JSON";
pub(crate) const WHY_NOT_SCHEMA: &str = "the document does not match the plan schema";

/// The plan schema, embedded at build time.
const SCHEMA: &str = include_str!("../../../schemas/plan.input.schema.json");

/// The plan schema's draft 2020-12 validator, built once, or `None` if it could not be built,
/// which would be a defect a unit test catches. It fetches nothing: `jsonschema` is built without
/// its `resolve-http` and `resolve-file` features, and the schema is self-contained (D38).
fn validator() -> Option<&'static Validator> {
    static BUILT: OnceLock<Option<Validator>> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let schema: Value = serde_json::from_str(SCHEMA).ok()?;
            jsonschema::draft202012::new(&schema).ok()
        })
        .as_ref()
}

/// Checks 1 and 3 to 6, in order, returning the document or the first failure's `<why>`.
pub(crate) fn read(bytes: &[u8]) -> Result<Value, String> {
    // 1. Size, a byte order mark counted.
    if bytes.len() > MAX_FILE_BYTES {
        return Err(WHY_TOO_LARGE.to_string());
    }
    // 3. UTF-8, over the whole file.
    let text = std::str::from_utf8(bytes).map_err(|_| WHY_NOT_UTF8.to_string())?;
    // 4. JSON, the leading byte order mark set aside. Any other U+FEFF outside a string is not JSON
    // whitespace, so the parser refuses it. serde_json refuses a string with an unpaired surrogate,
    // and nesting past its limit of 128 levels, which is above the 64 lease-api.md requires.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let document: Value = serde_json::from_str(text).map_err(|_| WHY_NOT_JSON.to_string())?;
    // 5. Members: the parser above keeps one of two values silently, so a second pass finds the
    // first member, in document order, whose name an earlier member of the same object has.
    if let Some(member) = first_repeated_member(text) {
        return Err(format!(
            "an object has two members named '{}'",
            quote(&member)
        ));
    }
    // 6. Schema.
    match validator() {
        Some(v) if v.is_valid(&document) => Ok(document),
        _ => Err(WHY_NOT_SCHEMA.to_string()),
    }
}

/// A value quoted by lease-api.md's Messages rules: each control character, U+0000 to U+001F and
/// U+007F, written as `\u` and four lowercase hex digits.
pub(crate) fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if c.is_ascii_control() {
            out.push_str(&format!("\\u{:04x}", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

/// The first member name, in document order, that repeats an earlier member's of the same object,
/// compared once escapes are read. The text is known to be one JSON document.
fn first_repeated_member(text: &str) -> Option<String> {
    let found = RefCell::new(None);
    let mut de = serde_json::Deserializer::from_str(text);
    // The pass stops at the first repeat, with an error; any other error cannot happen on a text
    // the parser accepted, and finds nothing.
    let _ = FirstRepeat(&found).deserialize(&mut de);
    found.into_inner()
}

/// A pass over a JSON text that stops at the first member whose name repeats an earlier member's
/// of the same object, and keeps that name. Nesting past serde_json's recursion limit is an error,
/// not a stack overflow.
struct FirstRepeat<'a>(&'a RefCell<Option<String>>);

impl<'de> DeserializeSeed<'de> for FirstRepeat<'_> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for FirstRepeat<'_> {
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
        while seq.next_element_seed(FirstRepeat(self.0))?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if seen.contains(&key) {
                *self.0.borrow_mut() = Some(key);
                return Err(de::Error::custom("a member is named twice"));
            }
            seen.insert(key);
            map.next_value_seed(FirstRepeat(self.0))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_schema_builds() {
        assert!(validator().is_some());
    }

    #[test]
    fn the_first_repeat_in_document_order_is_found() {
        assert_eq!(first_repeated_member(r#"{"a":1,"b":2}"#), None);
        assert_eq!(
            first_repeated_member(r#"{"a":1,"a":2}"#).as_deref(),
            Some("a")
        );
        // The inner repeat comes first in the text, before the outer one.
        assert_eq!(
            first_repeated_member(r#"{"o":{"x":1,"x":2},"o":3}"#).as_deref(),
            Some("x")
        );
        // The outer repeat comes first when it precedes the inner object.
        assert_eq!(
            first_repeated_member(r#"{"o":1,"o":{"x":1,"x":2}}"#).as_deref(),
            Some("o")
        );
        // Inside an array, and in a later sibling only.
        assert_eq!(
            first_repeated_member(r#"[{"a":1},{"b":1,"c":2,"b":3}]"#).as_deref(),
            Some("b")
        );
        // Two names the same once escapes are read.
        assert_eq!(
            first_repeated_member(r#"{"turns":1,"turns":2}"#).as_deref(),
            Some("turns")
        );
        // Siblings in different objects never repeat each other.
        assert_eq!(first_repeated_member(r#"[{"a":1},{"a":1}]"#), None);
    }

    #[test]
    fn a_control_character_is_quoted_as_an_escape() {
        assert_eq!(quote("a\nb\u{7f}c\u{0}"), "a\\u000ab\\u007fc\\u0000");
        assert_eq!(quote("plain name"), "plain name");
    }

    #[test]
    fn a_repeated_member_with_a_control_character_is_quoted() {
        let why = read(b"{\"a\\u0009\":1,\"a\\t\":2}").unwrap_err();
        assert_eq!(why, "an object has two members named 'a\\u0009'");
    }

    #[test]
    fn the_file_checks_come_in_lease_apis_order() {
        let big = vec![0xffu8; MAX_FILE_BYTES + 1];
        assert_eq!(read(&big).unwrap_err(), WHY_TOO_LARGE);
        assert_eq!(read(b"{\"turns\":\xff}").unwrap_err(), WHY_NOT_UTF8);
        // A repeated member in a text that is not JSON is reported as not JSON.
        assert_eq!(read(br#"{"a":1,"a":2"#).unwrap_err(), WHY_NOT_JSON);
        // A repeated member in a document the schema refuses is reported as the repeat.
        assert_eq!(
            read(br#"{"a":1,"a":2}"#).unwrap_err(),
            "an object has two members named 'a'"
        );
        assert_eq!(read(br#"{"a":1}"#).unwrap_err(), WHY_NOT_SCHEMA);
    }

    #[test]
    fn json_failures_are_not_json() {
        for text in [
            &b""[..],
            b"   \r\n\t ",
            b"{} {}",
            b"{}x",
            // A second byte order mark, and one outside a string.
            b"\xef\xbb\xbf\xef\xbb\xbf{}",
            b"{\xef\xbb\xbf}",
            // An unpaired surrogate, leading and trailing.
            br#"{"name":"\ud800","turns":[]}"#,
            br#"{"name":"\udc00","turns":[]}"#,
        ] {
            assert_eq!(read(text).unwrap_err(), WHY_NOT_JSON, "{text:?}");
        }
        // UTF-16, with its byte order mark and without, is not UTF-8.
        assert_eq!(read(b"\xff\xfe{\x00}\x00").unwrap_err(), WHY_NOT_UTF8);
        assert_eq!(read(b"{\x00}\x00").unwrap_err(), WHY_NOT_JSON);
        let nested = format!("{}{}", "[".repeat(200), "]".repeat(200));
        assert_eq!(read(nested.as_bytes()).unwrap_err(), WHY_NOT_JSON);
    }

    #[test]
    fn nesting_of_64_levels_is_parsed() {
        // 64 levels with the document as level 1: the schema then refuses it, not the parser.
        let nested = format!("{}{}", "[".repeat(64), "]".repeat(64));
        assert_eq!(read(nested.as_bytes()).unwrap_err(), WHY_NOT_SCHEMA);
    }

    #[test]
    fn a_leading_byte_order_mark_is_set_aside() {
        let plan = br#"{"turns":[{"name":"T","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]}]}"#;
        let mut with_bom = b"\xef\xbb\xbf".to_vec();
        with_bom.extend_from_slice(plan);
        assert!(read(plan).is_ok());
        assert!(read(&with_bom).is_ok());
    }
}
