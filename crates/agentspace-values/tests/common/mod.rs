// Shared helpers for the integration tests of agentspace-values. Each test file uses some of them,
// so an unused helper in one file is not a defect.
#![allow(dead_code)]

use agentspace_values::{ValueCode, ValueError};

/// Every character with the Unicode `White_Space` property, as lease-api.md's Input tolerance lists
/// them: U+0009 to U+000D, U+0020, U+0085, U+00A0, U+1680, U+2000 to U+200A, U+2028, U+2029,
/// U+202F, U+205F and U+3000.
pub fn whitespace_set() -> Vec<char> {
    let mut v: Vec<char> = Vec::new();
    v.extend((0x09u32..=0x0d).filter_map(char::from_u32));
    v.extend(['\u{20}', '\u{85}', '\u{a0}', '\u{1680}']);
    v.extend((0x2000u32..=0x200a).filter_map(char::from_u32));
    v.extend(['\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}']);
    v
}

pub const UUID: &str = "3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f";

pub type StringFn = fn(&str) -> Result<String, ValueError>;

/// The five string values: the field, the function and one valid sample.
pub fn string_values() -> Vec<(&'static str, StringFn, &'static str)> {
    vec![
        ("id", agentspace_values::lease_id as StringFn, UUID),
        ("project", agentspace_values::project as StringFn, "my-proj"),
        (
            "platform",
            agentspace_values::platform as StringFn,
            "linux/arm64",
        ),
        (
            "profile",
            agentspace_values::profile as StringFn,
            "agent-linux",
        ),
        ("size", agentspace_values::size as StringFn, "mm"),
    ]
}

pub fn code_name(c: &ValueCode) -> &'static str {
    match c {
        ValueCode::InvalidValue => "invalid_value",
        ValueCode::OutOfRange => "out_of_range",
    }
}

/// Unwraps an accepted value, and says what was refused when it was not.
pub fn ok<T>(r: Result<T, ValueError>, input: &str) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!(
            "{input:?} must be accepted, but was refused as {} for {:?}: {:?}",
            code_name(&e.code),
            e.field,
            e.message
        ),
    }
}

/// Unwraps a refusal, and says what was accepted when it was not.
pub fn refused<T>(r: Result<T, ValueError>, input: &str) -> ValueError {
    match r {
        Ok(_) => panic!("{input:?} must be refused, but was accepted"),
        Err(e) => e,
    }
}

/// Asserts `invalid_value` for `field`, and returns the message.
pub fn invalid<T>(r: Result<T, ValueError>, input: &str, field: &str) -> String {
    let e = refused(r, input);
    assert!(
        matches!(e.code, ValueCode::InvalidValue),
        "{input:?} must be invalid_value, was {}: {:?}",
        code_name(&e.code),
        e.message
    );
    assert_eq!(e.field, field, "field of the error for {input:?}");
    assert_message_rules(&e.message, input);
    e.message
}

/// Asserts `out_of_range` for `field`, and returns the message.
pub fn out_of_range<T>(r: Result<T, ValueError>, input: &str, field: &str) -> String {
    let e = refused(r, input);
    assert!(
        matches!(e.code, ValueCode::OutOfRange),
        "{input:?} must be out_of_range, was {}: {:?}",
        code_name(&e.code),
        e.message
    );
    assert_eq!(e.field, field, "field of the error for {input:?}");
    assert_message_rules(&e.message, input);
    e.message
}

/// Messages: never empty, never start with a space, never hold a control character (lease-api.md,
/// Exit codes and errors).
pub fn assert_message_rules(message: &str, input: &str) {
    assert!(!message.is_empty(), "the message for {input:?} is empty");
    assert!(
        !message.starts_with(' '),
        "the message for {input:?} starts with a space: {message:?}"
    );
    assert!(
        !message.chars().any(|c| c.is_ascii_control()),
        "the message for {input:?} holds a control character: {message:?}"
    );
}

pub fn ttl_ok(v: &str) -> u32 {
    ok(agentspace_values::ttl(v), v)
}

pub fn ttl_invalid(v: &str) -> String {
    invalid(agentspace_values::ttl(v), v, "ttl")
}

pub fn ttl_range(v: &str) -> String {
    out_of_range(agentspace_values::ttl(v), v, "ttl")
}

pub fn msg_no_unit(quoted: &str) -> String {
    format!("TTL {quoted} has no unit. Give one, as in '8h'.")
}

pub fn msg_m(quoted: &str) -> String {
    format!(
        "TTL {quoted} uses 'M', which could mean months or minutes. Write minutes as 'm' or 'min'."
    )
}

pub fn msg_not_duration(quoted: &str) -> String {
    format!(
        "TTL {quoted} is not a duration. Give one as in '8h', '90m', '1h30m', '2 hours' or 'PT8H'."
    )
}

pub fn msg_range(quoted: &str) -> String {
    format!("TTL {quoted} is outside the allowed range. Give from 1m to 24h; it is never clamped.")
}

pub fn msg_for(field: &str, quoted: &str) -> String {
    match field {
        "id" => format!("Lease ID {quoted} is not a UUIDv4. Give the id that create printed."),
        "project" => format!(
            "Project {quoted} is not a project name. Use 1 to 32 letters, digits and '-', starting and ending with a letter or digit."
        ),
        "platform" => {
            format!("Platform {quoted} is not an os/arch pair. Give one as in 'linux/arm64'.")
        }
        "profile" => format!(
            "Profile {quoted} is not a profile name. Give name or name@version, as in 'agent-linux'; a version is 1 or more."
        ),
        "size" => format!("Size {quoted} is not a size. Use 1 to 8 letters and digits, as in 'm'."),
        other => panic!("no template for field {other}"),
    }
}

pub fn msg_state(quoted: &str) -> String {
    format!(
        "State {quoted} is not a state. Use pending, ready, expired, released or unsatisfiable."
    )
}

/// Case variants of an ASCII word: lower, upper, capitalized and two alternating forms.
pub fn case_variants(s: &str) -> Vec<String> {
    let lower = s.to_ascii_lowercase();
    let upper = s.to_ascii_uppercase();
    let mut chars = lower.chars();
    let cap: String = match chars.next() {
        Some(f) => f.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    };
    let alt: String = lower
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i % 2 == 0 {
                c
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect();
    let alt2: String = lower
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i % 2 == 1 {
                c
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect();
    let mut v = vec![lower, upper, cap, alt, alt2];
    v.sort();
    v.dedup();
    v
}
