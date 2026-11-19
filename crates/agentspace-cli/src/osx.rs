//! An argument as the operating system gave it, read without losing a unit.
//!
//! An argument is a run of units: bytes on Unix, UTF-16 code units on Windows. Neither need be valid
//! Unicode, and none is ever replaced. Everything above this module works on units, so the same code
//! serves both systems, and the only code per system is here.

use std::ffi::{OsStr, OsString};

/// One piece of an argument: a character, or a unit that is not part of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    /// A character.
    Char(char),
    /// A unit that is not valid Unicode, as the code of the unpaired surrogate that stands for it.
    Bad(u16),
}

/// The units of an argument.
#[cfg(unix)]
pub fn units(value: &OsStr) -> Vec<u32> {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().iter().map(|&b| u32::from(b)).collect()
}

/// The units of an argument.
#[cfg(windows)]
pub fn units(value: &OsStr) -> Vec<u32> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().map(u32::from).collect()
}

/// The argument that has exactly these units.
#[cfg(unix)]
pub fn from_units(units: &[u32]) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    // Every unit came from a byte, so the narrowing never loses one.
    OsString::from_vec(units.iter().map(|&u| u as u8).collect())
}

/// The argument that has exactly these units.
#[cfg(windows)]
pub fn from_units(units: &[u32]) -> OsString {
    use std::os::windows::ffi::OsStringExt;
    let wide: Vec<u16> = units.iter().map(|&u| u as u16).collect();
    OsString::from_wide(&wide)
}

/// The argument read as characters, with each unit that is not valid Unicode kept apart.
#[cfg(unix)]
pub fn pieces(value: &OsStr) -> Vec<Piece> {
    use std::os::unix::ffi::OsStrExt;
    let mut out = Vec::new();
    let mut rest = value.as_bytes();
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                out.extend(text.chars().map(Piece::Char));
                break;
            }
            Err(e) => {
                let (good, bad) = rest.split_at(e.valid_up_to());
                if let Ok(text) = std::str::from_utf8(good) {
                    out.extend(text.chars().map(Piece::Char));
                }
                let len = e.error_len().unwrap_or(bad.len()).max(1);
                // GAP: lease-api.md names only unpaired surrogates. A byte that is not UTF-8 is
                // written as the surrogate that Python's "surrogateescape" gives it, U+DC00 + byte.
                out.extend(
                    bad[..len]
                        .iter()
                        .map(|&b| Piece::Bad(0xDC00 + u16::from(b))),
                );
                rest = &bad[len..];
            }
        }
    }
    out
}

/// The argument read as characters, with each unit that is not valid Unicode kept apart.
#[cfg(windows)]
pub fn pieces(value: &OsStr) -> Vec<Piece> {
    use std::os::windows::ffi::OsStrExt;
    char::decode_utf16(value.encode_wide())
        .map(|r| match r {
            Ok(c) => Piece::Char(c),
            Err(e) => Piece::Bad(e.unpaired_surrogate()),
        })
        .collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn units_round_trip_bytes_that_are_not_utf8() {
        let os = OsString::from_vec(vec![b'a', 0xff, 0xc3, b'-']);
        assert_eq!(from_units(&units(&os)), os);
    }

    #[test]
    fn pieces_keep_each_bad_byte_apart() {
        let os = OsString::from_vec(vec![b'a', 0xff, 0xe2, 0x82, b'b']);
        assert_eq!(
            pieces(&os),
            vec![
                Piece::Char('a'),
                Piece::Bad(0xDCFF),
                Piece::Bad(0xDCE2),
                Piece::Bad(0xDC82),
                Piece::Char('b')
            ]
        );
    }
}
