//! The encoded remote form a Windows host's every remote command takes (docs/operator.md, The
//! remote command on Windows; N4, pin P16 of v0.12-T02), and its reading by a test.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. A test never runs the script it decodes:
//! it reads what the script names.
#![cfg(windows)]
#![allow(dead_code)]

/// The line every remote command to a Windows host starts with.
pub const ENCODED_PREFIX: &str =
    "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ";

fn b64_value(c: u8) -> u8 {
    match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        other => panic!("{:?} is no base64 digit", other as char),
    }
}

/// Decodes a remote command in the encoded form and returns the script it carries, failing, with
/// the rule it breaks, when the line is not operator.md's form: the program and its flags, then
/// base64 with no `=`, of UTF-16LE bytes whose count is a multiple of 3, the whole line at most
/// 8,191 characters and with no `%`, double quote, control character or trailing backslash.
pub fn decode_encoded(cmd: &str) -> String {
    let b64 = cmd.strip_prefix(ENCODED_PREFIX).unwrap_or_else(|| {
        panic!(
            "A Windows host's remote command MUST be the encoded form, and is {:?}",
            &cmd[..cmd.len().min(60)]
        )
    });
    assert!(
        cmd.len() <= 8191,
        "The line MUST be at most 8,191 characters: {}",
        cmd.len()
    );
    assert!(
        !cmd.contains('%')
            && !cmd.contains('"')
            && !cmd.chars().any(|c| c.is_control())
            && !cmd.ends_with('\\'),
        "The line MUST hold no %, double quote or control character, and not end in a backslash"
    );
    assert!(!b64.contains('='), "The base64 MUST have no =");
    assert!(
        !b64.is_empty() && b64.len().is_multiple_of(4),
        "The script's byte count MUST be a multiple of 3, so its base64 is a multiple of 4 digits: {}",
        b64.len()
    );
    let mut bytes = Vec::new();
    for quad in b64.as_bytes().chunks(4) {
        let v: Vec<u8> = quad.iter().map(|c| b64_value(*c)).collect();
        bytes.push(v[0] << 2 | v[1] >> 4);
        bytes.push(v[1] << 4 | v[2] >> 2);
        bytes.push(v[2] << 6 | v[3]);
    }
    assert!(
        bytes.len().is_multiple_of(2),
        "UTF-16LE has an even byte count"
    );
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    String::from_utf16(&units).expect("The script is valid UTF-16LE")
}

/// The last argument of a call's arguments, which is a remote command's place.
pub fn last_arg(args: &[String]) -> &str {
    args.last().map(String::as_str).unwrap_or("")
}

/// Encodes `script` as the form does: UTF-16LE, padded with spaces to a multiple of 3 bytes,
/// base64 with no `=`. For the proof of the decoder.
pub fn encode(script: &str) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = script.to_string();
    loop {
        let bytes = text.encode_utf16().count() * 2;
        if bytes.is_multiple_of(3) {
            break;
        }
        text.push(' ');
    }
    let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut b64 = String::new();
    for triple in bytes.chunks(3) {
        let n = (u32::from(triple[0]) << 16) | (u32::from(triple[1]) << 8) | u32::from(triple[2]);
        for shift in [18, 12, 6, 0] {
            b64.push(DIGITS[((n >> shift) & 63) as usize] as char);
        }
    }
    format!("{ENCODED_PREFIX}{b64}")
}

#[test]
fn the_decoder_reads_what_the_encoder_wrote_and_refuses_what_is_not_the_form() {
    // Proof of the decoder, which every Windows test leans on: a round trip, and each rule.
    let script = "Remove-Item -Recurse -Force $HOME\\.agentspace-system; exit 0";
    let line = encode(script);
    assert_eq!(decode_encoded(&line).trim_end(), script);
    for bad in [
        "bash -c true".to_string(),
        format!("{ENCODED_PREFIX}QQ=="),
        format!("{ENCODED_PREFIX}QUJD"),
        ENCODED_PREFIX.to_string(),
        format!("{ENCODED_PREFIX}AB%D"),
    ] {
        assert!(
            std::panic::catch_unwind(|| decode_encoded(&bad)).is_err(),
            "{bad:?} is not the form"
        );
    }
}
