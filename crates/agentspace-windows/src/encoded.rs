//! The remote command form on Windows (docs/operator.md, The remote command on Windows; N4).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Every remote command the operator sends a Windows host over ssh is one line,
//! `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand <b64>`, where
//! `<b64>` is the base64 of the script's UTF-16LE bytes with no `=`: the script is given one or
//! two spaces at its end when needed, so its byte count is a multiple of 3. The whole line MUST be
//! at most 8,191 characters, cmd.exe's limit, and a script that would make it longer is never
//! sent.

use agentspace_operator_api::{CallerFailure, Failure};

/// The line's fixed start, before the base64.
pub const PREFIX: &str =
    "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ";

/// The longest line the form allows: cmd.exe's limit on a command line.
pub const MAX_LINE: usize = 8191;

/// The failure of a script too long for the form. For the dev lead: operator.md gives the row no
/// message of its own, and Linux has no such step, so this is the crate's own sentence, naming no
/// backend and never quoting the script, with the caller's "not available" line in caller mode.
pub const TOO_LONG: &str = "agentspace: A step's script is too long to send to this Windows host \
as one command, so it was not sent.";

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The one remote command line that runs `script` on a Windows host, or [`TOO_LONG`]'s failure.
pub fn remote_command(script: &str) -> Result<String, Failure> {
    let mut units: Vec<u16> = script.encode_utf16().collect();
    // An empty script goes as three spaces: an empty `-EncodedCommand` is refused, and three
    // spaces are six bytes, which encode without `=` and do nothing.
    let pad = if units.is_empty() {
        3
    } else {
        (3 - units.len() % 3) % 3
    };
    units.extend(std::iter::repeat_n(u16::from(b' '), pad));

    // Each 3 units are 6 bytes, which base64 writes as 8 characters, so the line's length is
    // known before anything is encoded.
    let length = PREFIX.len() + units.len() / 3 * 8;
    if length > MAX_LINE {
        return Err(too_long());
    }

    let bytes: Vec<u8> = units.iter().flat_map(|unit| unit.to_le_bytes()).collect();
    let mut line = String::with_capacity(length);
    line.push_str(PREFIX);
    // The padding made the byte count a multiple of 3, so no bytes are left over.
    let (groups, _) = bytes.as_chunks::<3>();
    for group in groups {
        let word = (u32::from(group[0]) << 16) | (u32::from(group[1]) << 8) | u32::from(group[2]);
        for shift in [18u32, 12, 6, 0] {
            line.push(char::from(ALPHABET[((word >> shift) & 63) as usize]));
        }
    }
    Ok(line)
}

fn too_long() -> Failure {
    Failure {
        message: TOO_LONG.to_string(),
        caller: CallerFailure::NotAvailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This module's own decoder, the test's, never the encoder's inverse by construction.
    fn decode(line: &str) -> String {
        let b64 = line.strip_prefix(PREFIX).expect("the encoded form");
        assert_eq!(b64.len() % 4, 0, "whole groups");
        let mut bytes = Vec::new();
        for group in b64.as_bytes().chunks(4) {
            let mut word = 0u32;
            for &c in group {
                let value = ALPHABET.iter().position(|&a| a == c).expect("base64");
                word = (word << 6) | value as u32;
            }
            bytes.extend([(word >> 16) as u8, (word >> 8) as u8, word as u8]);
        }
        assert_eq!(bytes.len() % 2, 0);
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&units).expect("UTF-16LE")
    }

    #[test]
    fn a_known_script_encodes_as_powershell_reads_it() {
        // "abc" is three units, six bytes: 61 00 62 00 63 00.
        assert_eq!(remote_command("abc").unwrap(), format!("{PREFIX}YQBiAGMA"));
    }

    #[test]
    fn padding_makes_whole_groups_with_no_equals_sign() {
        for len in 1..40 {
            let script = "x".repeat(len);
            let line = remote_command(&script).unwrap();
            assert!(!line.contains('='), "{len}");
            let decoded = decode(&line);
            assert_eq!(decoded.trim_end_matches(' '), script);
            assert!(
                decoded.len() - script.len() <= 2,
                "one or two spaces at most"
            );
        }
    }

    #[test]
    fn an_empty_script_is_three_spaces() {
        assert_eq!(decode(&remote_command("").unwrap()), "   ");
    }

    #[test]
    fn the_longest_line_is_sent_and_one_group_more_is_not() {
        let groups = (MAX_LINE - PREFIX.len()) / 8;
        let fits = "y".repeat(groups * 3);
        let line = remote_command(&fits).unwrap();
        assert!(line.len() <= MAX_LINE);
        assert_eq!(line.len(), PREFIX.len() + groups * 8);
        let over = "y".repeat(groups * 3 + 1);
        let failure = remote_command(&over).unwrap_err();
        assert_eq!(failure.message, TOO_LONG);
        assert_eq!(failure.caller, CallerFailure::NotAvailable);
    }
}
