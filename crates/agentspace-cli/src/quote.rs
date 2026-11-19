//! Quoting a value in a message (lease-api.md, Messages and Input tolerance, gap FA-09).

use std::ffi::OsStr;
use std::fmt::Write;

use crate::osx::{Piece, pieces};

/// Escapes a string for a message: each control character (U+0000 to U+001F, and U+007F) is written
/// as `\u` and four lowercase hex digits. Other characters stay as they are.
pub fn escape_controls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if u32::from(c) <= 0x1f || u32::from(c) == 0x7f {
            push_escape(&mut out, u32::from(c));
        } else {
            out.push(c);
        }
    }
    out
}

fn push_escape(out: &mut String, code: u32) {
    // Writing to a String never fails.
    let _ = write!(out, "\\u{code:04x}");
}

/// Quotes a value for a message: `'<value>'`, as given, in single quotes. Each control character
/// and each unpaired surrogate is written as `\u` and four lowercase hex digits, so the message is
/// valid UTF-8 and holds no control character.
pub fn quote_os(value: &OsStr) -> String {
    let mut out = String::from("'");
    for piece in pieces(value) {
        match piece {
            Piece::Char(c) if u32::from(c) <= 0x1f || u32::from(c) == 0x7f => {
                push_escape(&mut out, u32::from(c));
            }
            Piece::Char(c) => out.push(c),
            Piece::Bad(code) => push_escape(&mut out, u32::from(code)),
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_are_written_as_lowercase_hex() {
        assert_eq!(
            quote_os(OsStr::new("a\nb\u{7f}\u{1f}")),
            "'a\\u000ab\\u007f\\u001f'"
        );
    }

    #[test]
    fn other_characters_stay() {
        assert_eq!(
            quote_os(OsStr::new("é 日本 \u{85}\\u")),
            "'é 日本 \u{85}\\u'"
        );
        assert_eq!(quote_os(OsStr::new("")), "''");
    }

    #[cfg(unix)]
    #[test]
    fn a_unit_that_is_not_unicode_is_escaped() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let os = OsString::from_vec(vec![b'x', 0xff]);
        assert_eq!(quote_os(&os), "'x\\udcff'");
    }

    #[test]
    fn escape_controls_leaves_text_alone_otherwise() {
        assert_eq!(escape_controls("a\u{0}b\tc"), "a\\u0000b\\u0009c");
    }
}
