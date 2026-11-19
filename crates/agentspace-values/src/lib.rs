//! agentspace-values (docs/decisions.md, D38).
//!
//! The accepted forms, normal forms and refusal messages of a lease's values, as
//! docs/lease-api.md's Input tolerance, Values and Messages say them. RFC 2119 keywords, and the
//! capitals are load-bearing.
//!
//! This crate is pure: it MUST NOT touch a file, the clock or the environment, and it MUST NOT
//! write to stdout or stderr or panic on any input. Every check matches the whole value. Letters
//! and digits are ASCII only, and "any case" means ASCII case.

mod ttl;

pub use ttl::ttl;

/// The two codes a value check refuses with (lease-api.md, Exit codes and errors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueCode {
    /// A value outside its accepted forms.
    InvalidValue,
    /// A well-formed value outside its allowed range.
    OutOfRange,
}

/// A refused value: its code, the field at fault without dashes, and the message for a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueError {
    pub code: ValueCode,
    pub field: &'static str,
    pub message: String,
}

/// The state names, in the order of lease-api.md's States table.
const STATE_NAMES: [&str; 5] = ["pending", "ready", "expired", "released", "unsatisfiable"];

/// Whether a character has the Unicode `White_Space` property, as Input tolerance lists it
/// character by character. U+FEFF is not whitespace.
pub(crate) fn is_trim_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'..='\u{000D}'
            | '\u{0020}'
            | '\u{0085}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
    )
}

/// The value without the whitespace before its first and after its last other character.
pub(crate) fn trim(v: &str) -> &str {
    v.trim_matches(is_trim_space)
}

/// A value as a message quotes it: as given, untrimmed, in single quotes, with each control
/// character (U+0000 to U+001F and U+007F) written as `\u` and four lowercase hex digits.
pub fn quote(v: &str) -> String {
    let mut out = String::with_capacity(v.len() + 2);
    out.push('\'');
    for c in v.chars() {
        if c.is_ascii_control() {
            out.push_str(&format!("\\u{:04x}", c as u32));
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

fn invalid(field: &'static str, message: String) -> ValueError {
    ValueError {
        code: ValueCode::InvalidValue,
        field,
        message,
    }
}

/// Whether the whole string is one or more ASCII letters and digits.
fn is_alnum(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Whether the whole string is 1 to `max` characters of ASCII letters, digits and `-`, starting
/// and ending with a letter or digit.
fn is_dashed_name(s: &str, max: usize) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= max
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'-')
        && b[0].is_ascii_alphanumeric()
        && b[b.len() - 1].is_ascii_alphanumeric()
}

/// A lease ID: a UUIDv4 in any case, with surrounding whitespace. Normal form: lowercase,
/// trimmed. The field is `id`.
pub fn lease_id(v: &str) -> Result<String, ValueError> {
    let t = trim(v);
    let b = t.as_bytes();
    let ok = b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            14 => *c == b'4',
            19 => matches!(*c, b'8' | b'9' | b'a' | b'b' | b'A' | b'B'),
            _ => c.is_ascii_hexdigit(),
        });
    if ok {
        Ok(t.to_ascii_lowercase())
    } else {
        Err(invalid(
            "id",
            format!(
                "Lease ID {} is not a UUIDv4. Give the id that create printed.",
                quote(v)
            ),
        ))
    }
}

/// A project: 1 to 32 letters, digits and `-`, starting and ending with a letter or digit.
/// Normal form: lowercase, trimmed. The field is `project`.
pub fn project(v: &str) -> Result<String, ValueError> {
    let t = trim(v);
    if is_dashed_name(t, 32) {
        Ok(t.to_ascii_lowercase())
    } else {
        Err(invalid(
            "project",
            format!(
                "Project {} is not a project name. Use 1 to 32 letters, digits and '-', \
                 starting and ending with a letter or digit.",
                quote(v)
            ),
        ))
    }
}

/// A platform: `os/arch` or `os/arch/variant`. Normal form: lowercase, trimmed, `aarch64`
/// written `arm64`, `x86_64` and `x86-64` written `amd64`, and the variant `v8` of `arm64`
/// dropped. The field is `platform`.
pub fn platform(v: &str) -> Result<String, ValueError> {
    let lower = trim(v).to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('/').collect();
    // Only the architecture MAY also be `x86_64` or `x86-64`.
    let well_formed = match parts.as_slice() {
        [os, arch] => is_alnum(os) && (is_alnum(arch) || is_x86_spelling(arch)),
        [os, arch, variant] => {
            is_alnum(os) && (is_alnum(arch) || is_x86_spelling(arch)) && is_alnum(variant)
        }
        _ => false,
    };
    if !well_formed {
        return Err(invalid(
            "platform",
            format!(
                "Platform {} is not an os/arch pair. Give one as in 'linux/arm64'.",
                quote(v)
            ),
        ));
    }
    let arch = match parts[1] {
        "aarch64" => "arm64",
        "x86_64" | "x86-64" => "amd64",
        other => other,
    };
    Ok(match parts.get(2) {
        Some(&"v8") if arch == "arm64" => format!("{}/{arch}", parts[0]),
        Some(variant) => format!("{}/{arch}/{variant}", parts[0]),
        None => format!("{}/{arch}", parts[0]),
    })
}

fn is_x86_spelling(s: &str) -> bool {
    s == "x86_64" || s == "x86-64"
}

/// A profile: `name` or `name@version`. Normal form: lowercase, trimmed, leading zeros dropped
/// from the version. The field is `profile`.
pub fn profile(v: &str) -> Result<String, ValueError> {
    let refused = || {
        invalid(
            "profile",
            format!(
                "Profile {} is not a profile name. Give name or name@version, as in \
                 'agent-linux'; a version is 1 or more.",
                quote(v)
            ),
        )
    };
    let lower = trim(v).to_ascii_lowercase();
    let (name, version) = match lower.split_once('@') {
        Some((n, ver)) => (n, Some(ver)),
        None => (lower.as_str(), None),
    };
    if !is_dashed_name(name, 64) {
        return Err(refused());
    }
    match version {
        None => Ok(name.to_string()),
        Some(ver) => {
            if ver.is_empty() || !ver.bytes().all(|b| b.is_ascii_digit()) {
                return Err(refused());
            }
            let digits = ver.trim_start_matches('0');
            if digits.is_empty() {
                return Err(refused());
            }
            Ok(format!("{name}@{digits}"))
        }
    }
}

/// A size: 1 to 8 letters and digits. Normal form: lowercase, trimmed. The field is `size`.
pub fn size(v: &str) -> Result<String, ValueError> {
    let t = trim(v);
    if is_alnum(t) && t.len() <= 8 {
        Ok(t.to_ascii_lowercase())
    } else {
        Err(invalid(
            "size",
            format!(
                "Size {} is not a size. Use 1 to 8 letters and digits, as in 'm'.",
                quote(v)
            ),
        ))
    }
}

/// The state filter of `list --state`. Each item is one `--state` value, split at commas here,
/// and each name is trimmed and matched in any ASCII case. The result is the state names in the
/// order of the States table, without duplicates. No occurrence at all gives an empty result,
/// meaning the filter was not given. The field is `state`.
pub fn states(occurrences: &[&str]) -> Result<Vec<&'static str>, ValueError> {
    let mut seen = [false; 5];
    for occurrence in occurrences {
        for raw in occurrence.split(',') {
            let name = trim(raw);
            match STATE_NAMES
                .iter()
                .position(|s| s.eq_ignore_ascii_case(name))
            {
                Some(i) => seen[i] = true,
                None => {
                    return Err(invalid(
                        "state",
                        format!(
                            "State {} is not a state. Use pending, ready, expired, released or \
                             unsatisfiable.",
                            quote(name)
                        ),
                    ));
                }
            }
        }
    }
    Ok(STATE_NAMES
        .iter()
        .zip(seen)
        .filter(|(_, s)| *s)
        .map(|(n, _)| *n)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ttl_code(v: &str) -> ValueCode {
        ttl(v).unwrap_err().code
    }

    #[test]
    fn ttl_forms_and_limits() {
        for (v, s) in [
            ("8h", 28800),
            ("90m", 5400),
            ("1h30m", 5400),
            ("1h 30m", 5400),
            ("2 hours", 7200),
            ("45 min", 2700),
            ("1d", 86400),
            ("23h59m60s", 86400),
            ("PT8H", 28800),
            ("pt90m", 5400),
            ("P1D", 86400),
            ("1440m", 86400),
            ("60s", 60),
            ("\u{3000}8h\n", 28800),
        ] {
            assert_eq!(ttl(v), Ok(s), "{v:?}");
        }
        for v in [
            "59s",
            "0m",
            "PT0S",
            "24h1s",
            "86401s",
            "P1DT2H",
            "99999999999999999999h",
        ] {
            assert_eq!(ttl_code(v), ValueCode::OutOfRange, "{v:?}");
        }
        for v in [
            "8",
            "0",
            "8M",
            "8 M",
            "1w",
            "P1M",
            "P8M",
            "1.5h",
            "-5m",
            "+5m",
            "1h1h",
            "30m1h",
            "PT",
            "P",
            "",
            " ",
            "1:30",
            "8\u{feff}h",
            "1m\u{feff}",
            "PT1H\u{feff}",
            "1 5h",
            "h",
        ] {
            assert_eq!(ttl_code(v), ValueCode::InvalidValue, "{v:?}");
        }
    }

    #[test]
    fn ttl_messages() {
        assert_eq!(
            ttl("8\n").unwrap_err().message,
            "TTL '8\\u000a' has no unit. Give one, as in '8h'."
        );
        assert!(ttl("8 M").unwrap_err().message.contains("uses 'M'"));
        assert!(ttl("P8M").unwrap_err().message.contains("uses 'M'"));
        assert!(ttl(" ").unwrap_err().message.contains("is not a duration"));
        assert!(
            ttl("8Mx")
                .unwrap_err()
                .message
                .contains("is not a duration")
        );
    }

    #[test]
    fn values_normalize() {
        assert_eq!(
            lease_id(" 123E4567-E89B-42D3-A456-426614174000\n").as_deref(),
            Ok("123e4567-e89b-42d3-a456-426614174000")
        );
        assert!(lease_id("123e4567-e89b-12d3-a456-426614174000").is_err());
        assert_eq!(project(" My-App ").as_deref(), Ok("my-app"));
        assert!(project("-a").is_err() && project("a-").is_err());
        assert_eq!(platform("Linux/AArch64/v8").as_deref(), Ok("linux/arm64"));
        assert_eq!(platform("linux/x86_64").as_deref(), Ok("linux/amd64"));
        assert_eq!(platform("linux/arm64/v9").as_deref(), Ok("linux/arm64/v9"));
        assert_eq!(platform("linux/amd64/v8").as_deref(), Ok("linux/amd64/v8"));
        assert!(platform("linux").is_err() && platform("linux/arm64/v8/x").is_err());
        assert!(platform("linux/arm_64").is_err() && platform("x86_64/arm64").is_err());
        assert_eq!(profile("Agent-Linux@03").as_deref(), Ok("agent-linux@3"));
        assert!(profile("agent-linux@0").is_err() && profile("agent-linux@").is_err());
        assert_eq!(size(" M ").as_deref(), Ok("m"));
        assert!(size("toolarge9").is_err());
    }

    #[test]
    fn states_filter() {
        assert_eq!(
            states(&["ready, PENDING", "ready"]),
            Ok(vec!["pending", "ready"])
        );
        let e = states(&["ready,"]).unwrap_err();
        assert_eq!(
            e.message,
            "State '' is not a state. Use pending, ready, expired, released or unsatisfiable."
        );
    }

    #[test]
    fn quoting() {
        assert_eq!(quote(" a\u{7f}\u{0}'"), "' a\\u007f\\u0000''");
    }
}
