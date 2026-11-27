//! The remote command form (docs/operator.md, The remote command on Windows; The platform seam,
//! the remote command row; N4): `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass
//! -EncodedCommand <b64>`, the base64 of the script's UTF-16LE bytes with no `=`, at most 8,191
//! characters.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;

use common::{
    MAX_LINE, PREFIX, assert_agentspace_words, base64_decode_unpadded, decode_line, encode,
    from_utf16le, platform, utf16le,
};

/// The largest count of three-unit groups a line of the limit holds: each group is eight base64
/// characters.
fn most_groups() -> usize {
    (MAX_LINE - PREFIX.len()) / 8
}

/// A script of `units` UTF-16 units, all `a`.
fn script_of_units(units: usize) -> String {
    "a".repeat(units)
}

fn padded_units(units: usize) -> usize {
    units.div_ceil(3) * 3
}

#[test]
fn the_line_starts_with_the_fixed_program_and_flags() {
    assert!(encode("Write-Output 'hello'").starts_with(PREFIX));
}

#[test]
fn the_payload_decodes_as_utf16le_to_the_script() {
    let script = "Write-Output 'one two three'"; // 28 units, so one padding space
    let decoded = decode_line(&encode(script));
    assert_eq!(decoded.trim_end_matches(' '), script);
}

#[test]
fn a_script_of_a_multiple_of_three_units_gets_no_padding() {
    let script = script_of_units(30);
    assert_eq!(decode_line(&encode(&script)), script);
}

#[test]
fn a_script_one_unit_over_a_multiple_of_three_gets_two_spaces() {
    let script = script_of_units(31);
    assert_eq!(decode_line(&encode(&script)), format!("{script}  "));
}

#[test]
fn a_script_two_units_over_a_multiple_of_three_gets_one_space() {
    let script = script_of_units(32);
    assert_eq!(decode_line(&encode(&script)), format!("{script} "));
}

#[test]
fn a_script_that_ends_in_spaces_keeps_them_and_is_padded_after_them() {
    let script = "exit 0   "; // 9 units
    assert_eq!(decode_line(&encode(script)), script);
}

#[test]
fn a_script_with_characters_outside_the_basic_plane_survives_as_surrogate_pairs() {
    let script = "Write-Output 'é 😀 日本'";
    let units = script.encode_utf16().count();
    let expected = format!("{script}{}", " ".repeat(padded_units(units) - units));
    assert_eq!(decode_line(&encode(script)), expected);
}

#[test]
fn the_payload_is_the_utf16le_bytes_not_utf8() {
    let script = "Get-Date";
    let line = encode(script);
    let bytes = base64_decode_unpadded(line.strip_prefix(PREFIX).unwrap()).unwrap();
    let mut expected = utf16le(script);
    expected.resize(padded_units(script.len()) * 2, 0);
    // The padding is spaces, which are 0x20 0x00 in UTF-16LE.
    for pair in expected[script.len() * 2..].chunks_mut(2) {
        pair[0] = 0x20;
    }
    assert_eq!(bytes, expected);
    assert_eq!(from_utf16le(&bytes).unwrap().trim_end(), script);
}

#[test]
fn there_is_no_equals_sign_for_any_script_length() {
    for units in 0..=40 {
        let line = encode(&script_of_units(units));
        assert!(!line.contains('='), "{units} units");
    }
}

#[test]
fn a_planted_padded_line_would_hold_an_equals_sign() {
    // Proves the "no =" check can fail: standard padded base64 of a 28-unit script has one.
    let script = script_of_units(28);
    let padded = common::base64_encode(&utf16le(&script));
    assert!(padded.contains('='));
}

#[test]
fn the_payload_holds_only_base64_characters_of_the_plus_slash_alphabet() {
    let line = encode("Write-Output \"100%\"`n\\");
    let payload = line.strip_prefix(PREFIX).unwrap();
    assert!(
        payload
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/'),
        "{payload}"
    );
}

#[test]
fn a_script_with_percent_quote_control_and_backslash_leaves_none_on_the_line() {
    let script = "$a = \"100%\"\r\n\tWrite-Output $a\\\u{7}\\";
    let line = encode(script);
    assert!(!line.contains('%'));
    assert!(!line.contains('"'));
    assert!(!line.chars().any(char::is_control));
    assert!(!line.ends_with('\\'));
}

#[test]
fn the_line_at_the_limit_is_sent() {
    let units = most_groups() * 3;
    let line = encode(&script_of_units(units));
    assert_eq!(line.len(), PREFIX.len() + most_groups() * 8);
    assert!(line.len() <= MAX_LINE);
}

#[test]
fn a_script_that_pads_up_to_the_limit_is_sent() {
    for under in [1, 2] {
        let units = most_groups() * 3 - under;
        let line = encode(&script_of_units(units));
        assert!(line.len() <= MAX_LINE, "{units} units");
    }
}

#[test]
fn a_script_one_unit_past_the_limit_is_never_sent() {
    let units = most_groups() * 3 + 1;
    assert!(platform().remote_command(&script_of_units(units)).is_err());
}

#[test]
fn a_very_long_script_is_never_sent() {
    assert!(platform().remote_command(&script_of_units(20_000)).is_err());
}

#[test]
fn a_script_whose_utf16_units_pass_the_limit_with_few_characters_is_never_sent() {
    // Each emoji is two units, so this holds fewer characters than the limit's units.
    let script = "😀".repeat(most_groups() * 3 / 2 + 1);
    assert!(script.chars().count() < most_groups() * 3);
    assert!(platform().remote_command(&script).is_err());
}

#[test]
fn the_too_long_message_is_agentspace_words_naming_no_backend() {
    let failure = platform()
        .remote_command(&script_of_units(20_000))
        .unwrap_err();
    assert_agentspace_words(&failure);
}

#[test]
fn the_too_long_message_does_not_carry_the_script() {
    let script = format!("# SECRET-MARKER {}", script_of_units(20_000));
    let failure = platform().remote_command(&script).unwrap_err();
    assert!(!failure.message.contains("SECRET-MARKER"));
}

#[test]
fn a_script_that_just_fits_does_not_fail() {
    assert!(
        platform()
            .remote_command(&script_of_units(most_groups() * 3))
            .is_ok()
    );
}

#[test]
fn an_empty_script_does_not_panic() {
    let _ = platform().remote_command("");
}

#[test]
fn the_line_never_carries_the_scripts_text() {
    let line = encode("Write-Output 'Mountain Standard Time'");
    assert!(!line.contains("Mountain"));
}
