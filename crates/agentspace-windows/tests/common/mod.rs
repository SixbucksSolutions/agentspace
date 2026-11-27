//! Helpers shared by the seam's tests: the one platform under test, and a base64 and UTF-16LE
//! codec of the test's own, so a remote command is decoded without trusting the code that built it.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#![allow(dead_code)]

use agentspace_operator_api::{Failure, HostPlatform};

/// Operator.md, The remote command on Windows: the line's fixed start.
pub const PREFIX: &str =
    "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ";

/// Operator.md, The remote command on Windows: cmd.exe's limit.
pub const MAX_LINE: usize = 8191;

/// The platform under test, as every caller reaches it.
pub fn platform() -> &'static dyn HostPlatform {
    &agentspace_windows::WINDOWS
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The script's UTF-16LE bytes.
pub fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}

/// Standard base64, padded with `=` when the length needs it.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let word = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(word >> 18) as usize & 63] as char);
        out.push(ALPHABET[(word >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(word >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[word as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Standard base64 with no padding allowed: `None` for any other character or a length that is
/// not a multiple of 4.
pub fn base64_decode_unpadded(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::new();
    for chunk in text.as_bytes().chunks(4) {
        let mut word = 0u32;
        for &byte in chunk {
            let value = ALPHABET.iter().position(|&c| c == byte)? as u32;
            word = (word << 6) | value;
        }
        out.push((word >> 16) as u8);
        out.push((word >> 8) as u8);
        out.push(word as u8);
    }
    Some(out)
}

/// UTF-16LE bytes back to text, `None` when they are not whole, valid UTF-16.
pub fn from_utf16le(bytes: &[u8]) -> Option<String> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16(&units).ok()
}

/// A remote command line, taken apart: the script it carries, with the padding spaces still on.
/// Panics (so the test fails) when the line is not the one form.
pub fn decode_line(line: &str) -> String {
    let encoded = line
        .strip_prefix(PREFIX)
        .unwrap_or_else(|| panic!("not the encoded form: {} characters", line.len()));
    let bytes = base64_decode_unpadded(encoded).expect("the payload is unpadded base64");
    from_utf16le(&bytes).expect("the payload is UTF-16LE")
}

/// The encoded line `HostPlatform::remote_command` gives, or a panic naming the message.
pub fn encode(script: &str) -> String {
    platform()
        .remote_command(script)
        .unwrap_or_else(|failure| panic!("remote_command refused: {}", failure.message))
}

/// A remote command that a row hands over already encoded, decoded to its script with the padding
/// spaces removed.
pub fn script_of(line: &str) -> String {
    decode_line(line).trim_end_matches(' ').to_string()
}

/// A failure is the person's one line, in agentspace's own words, naming no backend.
pub fn assert_agentspace_words(failure: &Failure) {
    let message = &failure.message;
    assert!(
        message.starts_with("agentspace: "),
        "starts with the program's name"
    );
    let after = &message["agentspace: ".len()..];
    assert!(
        after.chars().next().is_some_and(|c| c.is_ascii_uppercase()),
        "the sentence starts with a capital"
    );
    assert!(message.ends_with('.'), "the sentence ends with a full stop");
    assert!(!message.contains('\n'), "one line");
    let folded = message.to_lowercase();
    for backend in ["aws", "ssm", "session manager", "cloudformation", "ec2"] {
        assert!(!folded.contains(backend), "names no backend: {backend}");
    }
}
