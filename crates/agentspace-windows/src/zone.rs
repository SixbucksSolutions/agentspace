//! The push's zone on a Windows host (docs/operator.md, The token push to a Windows host; D96,
//! N17).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The zone is the laptop's Windows zone ID itself, never converted to an IANA name, and it MUST
//! match `^[A-Za-z][A-Za-z0-9 .+()-]{0,62}[A-Za-z0-9)]\z`, case-sensitively, `[A-Za-z]` being the
//! ASCII letters alone. A zone that does not is the empty zone, which is not sent.

use agentspace_operator_api::{CallerFailure, Failure};

/// The failure for a zone outside the pattern. For the dev lead: operator.md names no message
/// for this row, and on Linux a zone that fails its check becomes the empty zone, whose warning
/// is this one (operator.md, What the laptop sends, step 1); so the push that
/// gets this failure sends no zone, and MAY print the message as that warning.
pub const NO_ZONE: &str =
    "agentspace: Could not work out the laptop's time zone; times on the box stay in its own zone.";

/// Whether `zone` matches the Windows zone ID's pattern.
pub fn is_windows_zone_id(zone: &str) -> bool {
    let bytes = zone.as_bytes();
    // `{0,62}` between a first and a last character: 2 to 64 characters, each ASCII.
    if bytes.len() < 2 || bytes.len() > 64 {
        return false;
    }
    let (first, rest) = (bytes[0], &bytes[1..]);
    let (last, middle) = (rest[rest.len() - 1], &rest[..rest.len() - 1]);
    first.is_ascii_alphabetic()
        && middle
            .iter()
            .all(|&c| c.is_ascii_alphanumeric() || b" .+()-".contains(&c))
        && (last.is_ascii_alphanumeric() || last == b')')
}

/// The zone the host is given: `windows_zone_id` when it matches the pattern, or [`NO_ZONE`]'s
/// failure.
pub fn push_zone(windows_zone_id: &str) -> Result<String, Failure> {
    if is_windows_zone_id(windows_zone_id) {
        Ok(windows_zone_id.to_string())
    } else {
        Err(no_zone())
    }
}

pub(crate) fn no_zone() -> Failure {
    Failure {
        message: NO_ZONE.to_string(),
        caller: CallerFailure::NotAvailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_ids_match() {
        for zone in [
            "Mountain Standard Time",
            "UTC-02",
            "UTC",
            "Pacific Standard Time (Mexico)",
            "W. Europe Standard Time",
            "UTC+12",
            "Ab",
        ] {
            assert!(is_windows_zone_id(zone), "{zone}");
        }
        assert!(is_windows_zone_id(&format!("A{}b", "x".repeat(62))));
    }

    #[test]
    fn other_values_do_not() {
        for zone in [
            "",
            "A",
            "America/Denver",
            "1UTC",
            " UTC",
            "UTC ",
            "UTC-",
            "UTC\n",
            "Mountain Standard Time\r",
            "It's Time",
            "Zone \"quoted\"",
            "Zoné Time",
            "Zone;Time",
            "Zone$Time",
        ] {
            assert!(!is_windows_zone_id(zone), "{zone:?}");
        }
        assert!(!is_windows_zone_id(&format!("A{}b", "x".repeat(63))));
    }

    #[test]
    fn a_zone_outside_the_pattern_fails_and_one_inside_is_returned_whole() {
        assert_eq!(
            push_zone("Mountain Standard Time").unwrap(),
            "Mountain Standard Time"
        );
        assert_eq!(push_zone("America/Denver").unwrap_err().message, NO_ZONE);
    }
}
