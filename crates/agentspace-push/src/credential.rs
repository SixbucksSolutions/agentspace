//! The Bot credential, `bot.json` (operator.md, The Bot credential; localswim's
//! `docs/windows-operations.md`, "The credential files", and its
//! `docs/schemas/launch-credential.schema.json`): one JSON object down to `token_dpapi_hex`'s
//! bytes, then the decryption behind [`Unprotect`], then the token.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The file is checked in code, never through localswim's schema.

use crate::{Token, Unprotect};
use serde::de::{self, DeserializeSeed, MapAccess, Visitor};
use serde_json::Value;
use std::fmt;
use zeroize::Zeroizing;

/// `format`'s one known value, compared exactly.
const FORMAT: &str = "localswim-credential";
/// `format_version`'s one known value, compared as an integer.
const FORMAT_VERSION: u64 = 1;
/// `role`'s value in `bot.json`, which MUST match the file's name.
const ROLE: &str = "bot";
/// The four members, each of which MUST be present exactly once, and no others.
const MEMBERS: [&str; 4] = ["format", "format_version", "role", "token_dpapi_hex"];

/// The top-level object's members, every one in the order given, a repeated name included.
/// serde_json alone keeps a repeated name's last value, so the members are collected here and
/// the duplicates found by [`parse_credential_blob`] itself.
struct TopLevel;

impl<'de> DeserializeSeed<'de> for TopLevel {
    type Value = Vec<(String, Value)>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for TopLevel {
    type Value = Vec<(String, Value)>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut members = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            members.push((key, map.next_value::<Value>()?));
        }
        Ok(members)
    }
}

/// `token_dpapi_hex`'s value as bytes: an even number of hex digits, at least two, and nothing
/// else.
// The architect's ruling for v0.07-T02 (2026-11-20; operator.md, The Bot credential): the hex is
// read tolerantly. Digits in either case are accepted, although localswim writes lowercase, and
// the DPAPI header and flags its schema pins are not checked here. `CryptUnprotectData` is the
// real check, and it refuses a blob that is not one.
fn hex_bytes(text: &str) -> Option<Vec<u8>> {
    let digits = text.as_bytes();
    if digits.is_empty() || !digits.len().is_multiple_of(2) {
        return None;
    }
    let nibble = |b: u8| -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    };
    let (pairs, _) = digits.as_chunks::<2>();
    pairs
        .iter()
        .map(|&[high, low]| Some((nibble(high)? << 4) | nibble(low)?))
        .collect()
}

/// Reads `bot.json`'s bytes down to the DPAPI blob in its `token_dpapi_hex` member. `None` is a
/// refusal, which gives the "Could not read" warning; it carries no reason, since no warning may
/// quote one. The blob is the token encrypted, not the token; the token exists only after
/// [`Unprotect`].
///
/// - The bytes MUST be UTF-8, after an optional UTF-8 byte order mark (`EF BB BF`). Any other
///   encoding, and any byte that is not valid UTF-8, is refused.
/// - The text MUST be exactly one JSON value, an object, with only whitespace around it.
/// - The object MUST have exactly the members `format`, `format_version`, `role` and
///   `token_dpapi_hex`, each once, named exactly so, and no others. A name given twice, and a name
///   that differs from another only in ASCII case, are refused.
/// - `format` MUST be the string `localswim-credential`, compared exactly; `format_version` MUST
///   be the integer 1, so `1.0`, `1e0` and `"1"` are refused; `role` MUST be the string `bot`.
/// - `token_dpapi_hex` MUST be a string of hex digits, an even number of them and at least two.
pub fn parse_credential_blob(bytes: &[u8]) -> Option<Vec<u8>> {
    let text = crate::text::decode_utf8(bytes)?;
    let mut de = serde_json::Deserializer::from_str(text);
    let members = TopLevel.deserialize(&mut de).ok()?;
    de.end().ok()?;

    // Exactly four members, each named exactly as one of the four.
    if members.len() != MEMBERS.len() {
        return None;
    }
    if !members
        .iter()
        .all(|(name, _)| MEMBERS.contains(&name.as_str()))
    {
        return None;
    }
    // A repeated name, and a name that differs from another only in ASCII case, are refused.
    // With four members all among the four names, this also means each of the four is present.
    // The count and the exact names above already refuse both; this loop is kept as a second
    // guard, so that loosening either check alone never lets a duplicate through.
    for (i, (name, _)) in members.iter().enumerate() {
        if members[i + 1..]
            .iter()
            .any(|(other, _)| other.eq_ignore_ascii_case(name))
        {
            return None;
        }
    }

    let member = |wanted: &str| {
        members
            .iter()
            .find(|(name, _)| name == wanted)
            .map(|(_, value)| value)
    };
    match member("format")? {
        Value::String(s) if s == FORMAT => {}
        _ => return None,
    }
    // An integer alone: serde_json reads `1.0` and `1e0` as floats, which have no `u64`.
    match member("format_version")? {
        Value::Number(n) if n.is_u64() && n.as_u64() == Some(FORMAT_VERSION) => {}
        _ => return None,
    }
    match member("role")? {
        Value::String(s) if s == ROLE => {}
        _ => return None,
    }
    match member("token_dpapi_hex")? {
        Value::String(s) => hex_bytes(s),
        _ => None,
    }
}

/// The token's length in characters (localswim's `docs/windows-operations.md`, "The credential
/// files": 32 random bytes in unpadded base64url).
const TOKEN_CHARS: usize = 43;

/// Whether a character is in unpadded base64url's alphabet: `A-Z`, `a-z`, `0-9`, `-` and `_`.
fn is_base64url(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// The token from the plaintext [`Unprotect`] gave.
///
/// - The plaintext MUST be UTF-16LE: an even byte count, and valid UTF-16, with no unpaired
///   surrogate.
/// - The token MUST then be exactly 43 characters, each from `A-Z`, `a-z`, `0-9`, `-` and `_`
///   (the architect's ruling for v0.07-T02, 2026-11-20; operator.md, The Bot credential; localswim's
///   `docs/windows-operations.md`, "The credential files", which requires a reader to check the
///   plaintext). A byte order mark, a terminator, padding, `+` and `/` are therefore refused.
///
/// Anything else is `None`, the push's "Could not read" path, and no value is printed.
pub(crate) fn token_from_plaintext(plain: &[u8]) -> Option<Token> {
    if !plain.len().is_multiple_of(2) {
        return None;
    }
    // Room for the longest UTF-8 the plaintext can give (3 bytes for each 2), so no reallocation
    // leaves a copy of the token behind.
    let mut text = Zeroizing::new(String::with_capacity(plain.len() / 2 * 3));
    let (pairs, _) = plain.as_chunks::<2>();
    let units = pairs.iter().map(|&pair| u16::from_le_bytes(pair));
    for c in char::decode_utf16(units) {
        text.push(c.ok()?);
    }
    // Every base64url character is one byte of UTF-8, so the byte count is the character count.
    if text.len() != TOKEN_CHARS || !text.chars().all(is_base64url) {
        return None;
    }
    Some(Token(text))
}

/// Reads `bot.json`'s bytes and decrypts its `token_dpapi_hex` to the token. `None` is a refusal,
/// a failed decryption included.
pub fn read_token(bytes: &[u8], unprotect: &dyn Unprotect) -> Option<Token> {
    let blob = parse_credential_blob(bytes)?;
    let plain = unprotect.unprotect(&blob)?;
    token_from_plaintext(&plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"0a0B"}"#;

    fn parse(text: &str) -> Option<Vec<u8>> {
        parse_credential_blob(text.as_bytes())
    }

    #[test]
    fn reads_the_hex_as_written() {
        assert_eq!(parse(GOOD), Some(vec![0x0a, 0x0b]));
        // PowerShell 5.1's and 7's indentation, and any member order.
        let spaced = "{\r\n    \"token_dpapi_hex\":  \"ff\",\r\n  \"role\": \"bot\",\n\t\"format_version\" : 1 ,\"format\":\"localswim-credential\"\r\n}\r\n";
        assert_eq!(parse(spaced), Some(vec![0xff]));
    }

    #[test]
    fn a_utf8_mark_is_accepted_and_no_other() {
        let mut marked = vec![0xEF, 0xBB, 0xBF];
        marked.extend(GOOD.as_bytes());
        assert_eq!(parse_credential_blob(&marked), Some(vec![0x0a, 0x0b]));
        let mut le = vec![0xFF, 0xFE];
        le.extend(GOOD.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(parse_credential_blob(&le), None, "UTF-16LE");
        let bare_le: Vec<u8> = GOOD.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(
            parse_credential_blob(&bare_le),
            None,
            "UTF-16LE with no mark"
        );
        let mut twice = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
        twice.extend(GOOD.as_bytes());
        assert_eq!(parse_credential_blob(&twice), None, "Two marks");
        let mut bad = GOOD.as_bytes().to_vec();
        bad.insert(1, 0xFF);
        assert_eq!(parse_credential_blob(&bad), None, "Invalid UTF-8");
    }

    #[test]
    fn an_escaped_name_is_the_name() {
        // `format`, its first letter written as the JSON escape backslash, `u0066`.
        let escaped_name = format!("\"{}u0066ormat\":", '\\');
        let escaped = GOOD.replace(r#""format":"#, &escaped_name);
        assert_ne!(escaped, GOOD);
        assert_eq!(parse(&escaped), Some(vec![0x0a, 0x0b]));
    }

    #[test]
    fn members_missing_extra_repeated_or_case_variant_are_refused() {
        let cases = [
            // Each member missing.
            r#"{"format_version":1,"role":"bot","token_dpapi_hex":"0a"}"#,
            r#"{"format":"localswim-credential","role":"bot","token_dpapi_hex":"0a"}"#,
            r#"{"format":"localswim-credential","format_version":1,"token_dpapi_hex":"0a"}"#,
            r#"{"format":"localswim-credential","format_version":1,"role":"bot"}"#,
            // An extra member.
            r#"{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"0a","x":1}"#,
            // A member given twice, with the same value and with another.
            r#"{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"0a","role":"bot"}"#,
            r#"{"format":"localswim-credential","format_version":1,"role":"human","token_dpapi_hex":"0a","role":"bot"}"#,
            r#"{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"0a","token_dpapi_hex":"0b"}"#,
            // A repeated name in place of a missing one keeps the count at four.
            r#"{"format":"localswim-credential","format_version":1,"role":"bot","role":"bot"}"#,
            // A case variant, beside its member and in its place.
            r#"{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"0a","ROLE":"bot"}"#,
            r#"{"Format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"0a"}"#,
            r#"{"format":"localswim-credential","format_version":1,"role":"bot","Token_DPAPI_Hex":"0a"}"#,
        ];
        for c in cases {
            assert_eq!(parse(c), None, "{c}");
        }
    }

    #[test]
    fn values_are_refused_unless_known() {
        let with = |member: &str, value: &str| {
            let mut fields = vec![
                ("format", r#""localswim-credential""#.to_owned()),
                ("format_version", "1".to_owned()),
                ("role", r#""bot""#.to_owned()),
                ("token_dpapi_hex", r#""0a""#.to_owned()),
            ];
            for f in &mut fields {
                if f.0 == member {
                    f.1 = value.to_owned();
                }
            }
            let body: Vec<String> = fields
                .iter()
                .map(|(n, v)| format!(r#""{n}":{v}"#))
                .collect();
            format!("{{{}}}", body.join(","))
        };
        assert_eq!(
            parse(&with("format", r#""localswim-credential""#)),
            Some(vec![0x0a])
        );
        let cases = [
            ("format", r#""Localswim-Credential""#),
            ("format", r#""localswim-credential ""#),
            ("format", r#""localswim-credentials""#),
            ("format", "null"),
            ("format_version", "1.0"),
            ("format_version", "1e0"),
            ("format_version", r#""1""#),
            ("format_version", "2"),
            ("format_version", "0"),
            ("format_version", "-1"),
            ("format_version", "true"),
            ("format_version", "null"),
            ("role", r#""human""#),
            ("role", r#""Bot""#),
            ("role", r#""bot ""#),
            ("role", "null"),
            ("token_dpapi_hex", r#""""#),
            ("token_dpapi_hex", r#""0a0""#),
            ("token_dpapi_hex", r#"" 0a""#),
            ("token_dpapi_hex", r#""0g""#),
            ("token_dpapi_hex", "10"),
            ("token_dpapi_hex", r#"["0a"]"#),
            ("token_dpapi_hex", "null"),
        ];
        for (member, value) in cases {
            let text = with(member, value);
            assert_eq!(parse(&text), None, "{member} = {value}");
        }
    }

    #[test]
    fn anything_but_one_object_is_refused() {
        for text in [
            "",
            "   ",
            "[]",
            "\"x\"",
            "null",
            "1",
            "not json",
            &format!("{GOOD} {{}}"),
            &format!("[{GOOD}]"),
            &GOOD.replace('}', ",}"),
            &format!("{GOOD} // c"),
            &GOOD.replace('"', "'"),
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }

    /// A made-up token, 43 characters, every class of the alphabet among them.
    const TOKEN43: &str = "Made-up_token_0123456789abcdefghijABCDEFGHI";

    fn utf16le(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    #[test]
    fn plaintext_rules() {
        assert_eq!(TOKEN43.len(), 43);
        let mut odd = utf16le(TOKEN43);
        odd.push(0x61);
        assert!(token_from_plaintext(&odd).is_none(), "Odd byte count");
        let mut surrogate = utf16le(&TOKEN43[..42]);
        surrogate.extend([0x00, 0xD8]);
        assert!(
            token_from_plaintext(&surrogate).is_none(),
            "Unpaired surrogate"
        );
        let t = token_from_plaintext(&utf16le(TOKEN43)).expect("Token");
        assert_eq!(t.expose(), TOKEN43);
    }

    #[test]
    fn the_token_is_43_base64url_characters() {
        let all_classes = format!("{}-_", &"aZ09".repeat(11)[..41]);
        assert_eq!(all_classes.len(), 43);
        assert!(token_from_plaintext(&utf16le(&all_classes)).is_some());
        let cases = [
            ("42 characters", TOKEN43[..42].to_owned()),
            ("44 characters", format!("{TOKEN43}A")),
            ("Empty", String::new()),
            ("A plus", format!("+{}", &TOKEN43[1..])),
            ("A slash", format!("{}/", &TOKEN43[..42])),
            ("Padding", format!("{}=", &TOKEN43[..42])),
            ("A space", format!("{} ", &TOKEN43[..42])),
            ("A line feed", format!("{}\n", &TOKEN43[..42])),
            ("A terminator", format!("{}\0", &TOKEN43[..42])),
            ("A byte order mark", format!("\u{FEFF}{}", &TOKEN43[..42])),
            ("A non-ASCII letter", format!("{}\u{e9}", &TOKEN43[..42])),
            ("A terminator after 43", format!("{TOKEN43}\0")),
            ("A byte order mark before 43", format!("\u{FEFF}{TOKEN43}")),
        ];
        for (what, text) in cases {
            assert!(token_from_plaintext(&utf16le(&text)).is_none(), "{what}");
        }
    }
}
