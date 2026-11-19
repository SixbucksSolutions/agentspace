//! The zone check of operator.md, The time zone, step 3, and the legacy names of step 2.
//!
//! Every test here would fail without the rule it names.

use agentspace_zone::{LEGACY_NAMES, check_zone, is_valid_zone, map_legacy, zone};

/// Names that pass the check, a shape at a time.
#[test]
fn names_that_pass() {
    for name in [
        "America/Los_Angeles",
        "UTC",
        "Etc/GMT+12",
        "Etc/GMT-12",
        "America/Argentina/Buenos_Aires",
        "America/Port-au-Prince",
        "_underscore",
        "+plus",
        "9lives",
        "a",
        "A/b",
        // A later segment MAY start with a hyphen: only the first character of the whole name
        // is barred from it.
        "Etc/-x",
        "a-b/c-d/e-f",
    ] {
        assert!(is_valid_zone(name), "{name:?} should pass");
        assert_eq!(check_zone(name), Some(name));
    }
}

/// The empty name has no first character, so it fails.
#[test]
fn refuses_the_empty_name() {
    assert!(!is_valid_zone(""));
    assert_eq!(check_zone(""), None);
}

/// A leading `-` would be read by the box's zone tool as an option.
#[test]
fn refuses_a_leading_hyphen() {
    for name in ["-", "-h", "--help", "-America/New_York", "-1"] {
        assert!(!is_valid_zone(name), "{name:?} should fail");
    }
}

/// `\z` never matches before a trailing newline, so a name ending in one fails, and so does one
/// with a newline anywhere.
#[test]
fn refuses_a_trailing_newline_and_any_other() {
    for name in [
        "America/New_York\n",
        "UTC\n",
        "America/New_York\r\n",
        "America/New_York\r",
        "America\n/New_York",
        "\nUTC",
    ] {
        assert!(!is_valid_zone(name), "{name:?} should fail");
    }
}

/// A quote, a space and every character a shell treats as syntax fail.
#[test]
fn refuses_quotes_spaces_and_shell_syntax() {
    for name in [
        "not a zone",
        " UTC",
        "UTC ",
        "America/New York",
        "UTC\t",
        "a'b",
        "a\"b",
        "a`b",
        "$(id)",
        "$HOME",
        "a;b",
        "a|b",
        "a&b",
        "a<b",
        "a>b",
        "a(b)",
        "a{b}",
        "a[b]",
        "a*b",
        "a?b",
        "a!b",
        "a#b",
        "a~b",
        "a\\b",
        "a=b",
        "a,b",
        "a:b",
        "a@b",
        "a%b",
        "a^b",
        "a.b",
        "America/New_York; reboot",
        "UTC'; reboot; '",
    ] {
        assert!(!is_valid_zone(name), "{name:?} should fail");
    }
}

/// The dot is not in the set, so `.` and `..` cannot climb out of the zone folder.
#[test]
fn refuses_dots_and_paths_that_climb() {
    for name in ["..", ".", "../etc/passwd", "America/../x", "./UTC", "UTC/."] {
        assert!(!is_valid_zone(name), "{name:?} should fail");
    }
}

/// A slash separates segments, and a segment is never empty.
#[test]
fn refuses_empty_segments() {
    for name in ["/", "/UTC", "UTC/", "America//New_York", "a/b/", "//"] {
        assert!(!is_valid_zone(name), "{name:?} should fail");
    }
}

/// `[A-Za-z]` means the ASCII letters alone, so a letter outside ASCII fails, in any segment.
#[test]
fn refuses_non_ascii() {
    for name in [
        "Zürich",
        "Europe/Zürich",
        "é",
        "UTC\u{a0}",
        "UTC\u{200b}",
        "\u{ff21}\u{ff22}",     // Full-width A and B.
        "Europe/\u{0662}",      // An Arabic-Indic digit.
        "Europe/\u{212a}elvin", // The Kelvin sign, which lowercases to an ASCII k.
        "日本",
        "UTC\u{0}",
        "UTC\u{7f}",
    ] {
        assert!(!is_valid_zone(name), "{name:?} should fail");
    }
}

/// The check is case-sensitive about the legacy map, and does not itself change case.
#[test]
fn check_gives_the_name_back_unchanged() {
    assert_eq!(check_zone("america/new_york"), Some("america/new_york"));
    assert_eq!(check_zone("UTC"), Some("UTC"));
}

/// Each of the seven legacy names maps to its current one, exactly as the spec's table gives.
#[test]
fn maps_the_seven_legacy_names() {
    let expected = [
        ("Asia/Calcutta", "Asia/Kolkata"),
        ("Asia/Katmandu", "Asia/Kathmandu"),
        ("Asia/Rangoon", "Asia/Yangon"),
        ("Europe/Kiev", "Europe/Kyiv"),
        ("America/Buenos_Aires", "America/Argentina/Buenos_Aires"),
        ("America/Godthab", "America/Nuuk"),
        ("America/Indianapolis", "America/Indiana/Indianapolis"),
    ];
    assert_eq!(LEGACY_NAMES.len(), 7);
    for (legacy, current) in expected {
        assert!(LEGACY_NAMES.contains(&(legacy, current)));
        assert_eq!(map_legacy(legacy), current);
        assert_eq!(zone(None, Some(legacy)).as_deref(), Some(current));
    }
}

/// Every other name is kept as it is, the current names included.
#[test]
fn keeps_every_other_name() {
    for name in [
        "Asia/Kolkata",
        "Europe/Kyiv",
        "America/Nuuk",
        "America/Los_Angeles",
        "UTC",
        "Asia/Calcutta/x",
        "Asia/Calcutta2",
        "xAsia/Calcutta",
    ] {
        assert_eq!(map_legacy(name), name);
    }
}

/// The legacy match is case-sensitive: a differently cased legacy name is not mapped.
#[test]
fn legacy_names_are_matched_case_sensitively() {
    for name in [
        "asia/calcutta",
        "ASIA/CALCUTTA",
        "Asia/calcutta",
        "europe/kiev",
    ] {
        assert_eq!(map_legacy(name), name);
        assert_eq!(zone(None, Some(name)).as_deref(), Some(name));
    }
}

/// A legacy name with anything around it is not a legacy name, and a name that fails the check
/// stays failed whether or not it looks like one.
#[test]
fn legacy_names_are_matched_whole() {
    for name in ["Asia/Calcutta ", " Asia/Calcutta", "Asia/Calcutta\n"] {
        assert_eq!(map_legacy(name), name);
        assert_eq!(zone(None, Some(name)), None);
    }
}

/// The map comes before the check, and the check applies to what the map gives. Every current
/// name the map gives passes the check.
#[test]
fn mapped_names_pass_the_check() {
    for (legacy, current) in LEGACY_NAMES {
        assert!(is_valid_zone(legacy));
        assert!(is_valid_zone(current));
    }
}
