//! The content, the stamp and the stamp file (operator.md, What the laptop reads, The stamp;
//! docs/state.md, The push stamp).

use crate::{Content, Token};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

fn hex(bytes: &[u8], upper: bool) -> String {
    let digits: &[u8; 16] = if upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from(digits[usize::from(b >> 4)]));
        s.push(char::from(digits[usize::from(b & 0x0F)]));
    }
    s
}

/// A value the content refuses: missing, empty, or holding a CR, an LF or a NUL.
fn refused(value: &str) -> bool {
    value.is_empty() || value.contains(['\r', '\n', '\0'])
}

/// The content, `LOCALSWIM_URL=<base URL>` LF `LOCALSWIM_TOKEN=<token>` LF, in UTF-8. `None` when
/// the base URL or the token is missing, empty, or holds a CR, an LF or a NUL, which gives the
/// "Could not read" warning.
pub fn content(base: Option<&str>, token: &Token) -> Option<Content> {
    let base = base?;
    let token = token.expose();
    if refused(base) || refused(token) {
        return None;
    }
    const URL: &[u8] = b"LOCALSWIM_URL=";
    const TOKEN: &[u8] = b"LOCALSWIM_TOKEN=";
    let len = URL.len() + base.len() + 1 + TOKEN.len() + token.len() + 1;
    // Sized once, so no reallocation leaves a copy of the token behind.
    let mut bytes = Zeroizing::new(Vec::with_capacity(len));
    bytes.extend_from_slice(URL);
    bytes.extend_from_slice(base.as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(TOKEN);
    bytes.extend_from_slice(token.as_bytes());
    bytes.push(b'\n');
    Some(Content(bytes))
}

/// The stamp: the SHA-256 of `<instance ID>` LF `<zone>` LF `<content>`, as 64 uppercase hex
/// digits. `instance_id` is taken with no whitespace around it, and `zone` is the checked zone
/// or empty.
pub fn stamp(instance_id: &str, zone: &str, content: &Content) -> String {
    let mut h = Sha256::new();
    h.update(instance_id.trim().as_bytes());
    h.update(b"\n");
    h.update(zone.as_bytes());
    h.update(b"\n");
    h.update(content.as_bytes());
    hex(&h.finalize(), true)
}

/// The hash the box prints for the content as it installs it, in lowercase hex. Content the
/// laptop accepted holds no CR, no BOM and no empty line, so it is the hash of its own bytes.
pub(crate) fn box_hash(content: &Content) -> String {
    hex(&Sha256::digest(content.as_bytes()), false)
}

/// The stored stamp, from the stamp file's bytes, when they match the schema
/// (`schemas/state/localswim-pushed.state.schema.json`). `None` is a stamp that "cannot be read",
/// so the push is due. JSON's whitespace is accepted.
///
/// The file is UTF-8, with or without the mark `EF BB BF`. This is not the receipt's rule: a
/// stamp file that is not UTF-8, such as one in UTF-16, cannot be parsed, so a push is due
/// (operator.md, What the laptop checks; architect, 2026-11-19, for E0.06-T01-S04).
pub fn parse_stamp_file(bytes: &[u8]) -> Option<String> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).ok()?;
    let value: Value = serde_json::from_str(text).ok()?;
    let object = value.as_object()?;
    if object.len() != 2 {
        return None;
    }
    // `const: 0` compares numbers by value, so 0.0 is 0 too.
    let version = object.get("version")?;
    if version.as_f64() != Some(0.0) {
        return None;
    }
    let stamp = object.get("stamp")?.as_str()?;
    if stamp.len() != 64
        || !stamp
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
    {
        return None;
    }
    Some(stamp.to_owned())
}

/// The stamp file's bytes: `{"version":0,"stamp":"<stamp>"}`, UTF-8 with no byte order mark,
/// compact, with no line ending.
pub fn stamp_file_bytes(stamp: &str) -> Vec<u8> {
    format!(r#"{{"version":0,"stamp":"{stamp}"}}"#).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(s: &str) -> Token {
        Token(Zeroizing::new(s.to_owned()))
    }

    const URL: &str = "http://127.0.0.1:8811";
    const DUMMY: &str = "dummy-token-not-real";

    #[test]
    fn content_bytes() {
        let c = content(Some(URL), &token(DUMMY)).expect("Content");
        assert_eq!(
            c.as_bytes(),
            b"LOCALSWIM_URL=http://127.0.0.1:8811\nLOCALSWIM_TOKEN=dummy-token-not-real\n"
        );
    }

    #[test]
    fn content_refusals() {
        assert!(content(None, &token(DUMMY)).is_none());
        assert!(content(Some(""), &token(DUMMY)).is_none());
        assert!(content(Some(URL), &token("")).is_none());
        for bad in ["a\rb", "a\nb", "a\0b"] {
            assert!(content(Some(bad), &token(DUMMY)).is_none());
            assert!(content(Some(URL), &token(bad)).is_none());
        }
    }

    #[test]
    fn stamp_known_vectors() {
        let c = content(Some(URL), &token(DUMMY)).expect("Content");
        // printf 'i-0123456789abcdef0\nAmerica/New_York\n<content>' | sha256sum
        assert_eq!(
            stamp("i-0123456789abcdef0", "America/New_York", &c),
            "4E4B58BFB013E01F8A82A233E4478A03F3C897173809800964533F1E1CD6E115"
        );
        assert_eq!(
            stamp(" i-0123456789abcdef0\n", "", &c),
            "E11A8107807B04022D29B8ECCA6F360C31962A9D4DF95DACD5E749CDE62E27AE"
        );
        assert_eq!(
            box_hash(&c),
            "1d171cf9f0a8d9965c97eb40f0ae2d7ee8b8afdd5486b25857521a4958f3666c"
        );
    }

    #[test]
    fn stamp_file_round_trip() {
        let s = "4E4B58BFB013E01F8A82A233E4478A03F3C897173809800964533F1E1CD6E115";
        let bytes = stamp_file_bytes(s);
        assert_eq!(
            bytes,
            format!("{{\"version\":0,\"stamp\":\"{s}\"}}").into_bytes()
        );
        assert_eq!(parse_stamp_file(&bytes).as_deref(), Some(s));
        let mut marked = vec![0xEF, 0xBB, 0xBF];
        marked.extend(format!(" {{ \"stamp\" : \"{s}\" ,\n\"version\": 0 }}\r\n").as_bytes());
        assert_eq!(parse_stamp_file(&marked).as_deref(), Some(s));
    }

    #[test]
    fn stamp_file_refusals() {
        let s = "4E4B58BFB013E01F8A82A233E4478A03F3C897173809800964533F1E1CD6E115";
        for text in [
            String::new(),
            "  ".to_owned(),
            s.to_owned(),
            format!(r#"{{"version":1,"stamp":"{s}"}}"#),
            format!(r#"{{"version":"0","stamp":"{s}"}}"#),
            format!(r#"{{"stamp":"{s}"}}"#),
            r#"{"version":0}"#.to_owned(),
            format!(r#"{{"version":0,"stamp":"{}"}}"#, s.to_lowercase()),
            format!(r#"{{"version":0,"stamp":"{s}0"}}"#),
            format!(r#"{{"version":0,"stamp":"{s}\n"}}"#),
            format!(r#"{{"version":0,"stamp":"{s}","x":1}}"#),
            format!(r#"[{{"version":0,"stamp":"{s}"}}]"#),
        ] {
            assert_eq!(parse_stamp_file(text.as_bytes()), None, "{text}");
        }
    }

    #[test]
    fn a_stamp_file_that_is_not_utf8_cannot_be_parsed() {
        // The stamp reader reads UTF-8 with an optional `EF BB BF` alone, not the receipt's rule
        // (architect, 2026-11-19, for E0.06-T01-S04).
        let s = "4E4B58BFB013E01F8A82A233E4478A03F3C897173809800964533F1E1CD6E115";
        let good = String::from_utf8(stamp_file_bytes(s)).expect("UTF-8");
        let mut le = vec![0xFF, 0xFE];
        le.extend(good.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(parse_stamp_file(&le), None, "UTF-16LE with its mark");
        let mut be = vec![0xFE, 0xFF];
        be.extend(good.encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(parse_stamp_file(&be), None, "UTF-16BE with its mark");
        let bare: Vec<u8> = good.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(parse_stamp_file(&bare), None, "UTF-16LE with no mark");
        let mut twice = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
        twice.extend(good.as_bytes());
        assert_eq!(parse_stamp_file(&twice), None, "Only one mark is not text");
        let mut bad = good.clone().into_bytes();
        bad.push(0xFF);
        assert_eq!(parse_stamp_file(&bad), None, "A byte that is not UTF-8");
    }
}
