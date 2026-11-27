//! The time zone for a Windows host: the laptop's Windows zone ID itself, never converted, and a
//! Linux host's IANA name unchanged (docs/operator.md, "The token push to a Windows host" and
//! "The time zone"; D96; E0.12-T03-S04, #0257), written from those pages alone.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The spec's rule for a Windows host's zone: **the ID of the zone the laptop is set to at that
//! moment, or `AGENTSPACE_TIMEZONE`'s value when it is set and not empty, as for Linux; never
//! converted to an IANA name, the legacy map not applying; it MUST match
//! `^[A-Za-z][A-Za-z0-9 .+()-]{0,62}[A-Za-z0-9)]\z`, case-sensitively**, and a zone that does not
//! is the empty zone. The crate reads no environment variable, so every test passes the override
//! in as a value.

use agentspace_zone::{
    is_valid_windows_zone_id, laptop_windows_zone, laptop_zone, windows_zone, windows_zone_id, zone,
};

// ---------------------------------------------------------------------------------------------
// The check, by the spec's pattern.
// ---------------------------------------------------------------------------------------------

/// The examples the spec names pass.
#[test]
fn the_specs_examples_of_windows_zone_ids_pass_the_check() {
    for id in [
        "Mountain Standard Time",
        "UTC-02",
        "Pacific Standard Time (Mexico)",
        "UTC",
        "W. Europe Standard Time",
    ] {
        assert!(is_valid_windows_zone_id(id), "{id:?}");
    }
}

/// An IANA name is not a Windows zone ID, since the pattern has no `/`, so a Windows host is
/// never given one, a legacy name included.
#[test]
fn an_iana_name_fails_the_windows_check() {
    for name in [
        "America/Denver",
        "America/New_York",
        "Asia/Calcutta",
        "Asia/Kolkata",
        "UTC/",
    ] {
        assert!(!is_valid_windows_zone_id(name), "{name:?}");
    }
}

/// The pattern's first character is an ASCII letter, its last a letter, a digit or `)`, and its
/// length 2 to 64 (one opening letter, up to 62 in the middle, one closing character).
#[test]
fn the_check_holds_the_patterns_first_last_and_length() {
    let of_length = |len: usize| format!("A{}B", "x".repeat(len - 2));
    assert!(is_valid_windows_zone_id(&of_length(2)));
    assert!(is_valid_windows_zone_id(&of_length(64)));
    assert!(!is_valid_windows_zone_id(&of_length(65)));
    for bad in [
        "", "A", "9A", " A", "-A", "(A)", "A-", "A+", "A.", "A ", "AB-",
    ] {
        assert!(!is_valid_windows_zone_id(bad), "{bad:?}");
    }
    for ok in ["A)", "A9", "Ab", "A (B)", "A.B", "A+B", "A-B"] {
        assert!(is_valid_windows_zone_id(ok), "{ok:?}");
    }
}

/// `\z` is the end of the string and never the end before a trailing newline, and nothing but
/// the pattern's ASCII characters passes: a tab, a quote, a semicolon, a non-ASCII letter and a
/// control character each fail.
#[test]
fn the_check_refuses_a_trailing_newline_and_any_character_outside_the_pattern() {
    for bad in [
        "Mountain Standard Time\n",
        "Mountain Standard Time\r",
        "Mountain\tStandard Time",
        "Mountain Standard Time;",
        "Mountain \"Standard\" Time",
        "Mountain Standard Time'",
        "Mountain Standard $Time",
        "Mountain Standard Tim\u{e9}",
        "\u{c9}astern Standard Time",
        "Mountain Standard Time\0",
    ] {
        assert!(!is_valid_windows_zone_id(bad), "{bad:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// The zone a Windows host is given, against the zone a Linux host is given.
// ---------------------------------------------------------------------------------------------

/// The same laptop zone is an IANA name for a Linux host and the Windows ID itself for a Windows
/// host: both ends are Windows, so nothing is converted (D96).
#[test]
fn a_windows_host_keeps_the_windows_id_where_a_linux_host_gets_its_iana_name() {
    let id = "Mountain Standard Time";
    assert_eq!(zone(Some(id), None).as_deref(), Some("America/Denver"));
    assert_eq!(windows_zone(Some(id), None).as_deref(), Some(id));
    // Another zone with a legacy name: the Linux host's is mapped, the Windows host's is the ID.
    let india = "India Standard Time";
    assert_eq!(zone(Some(india), None).as_deref(), Some("Asia/Kolkata"));
    assert_eq!(windows_zone(Some(india), None).as_deref(), Some(india));
}

/// The ID is not looked up: a well-formed ID that CLDR's table does not know is still sent as it
/// is, since the host's own `Set-TimeZone` is what knows the IDs, and it is checked by its
/// pattern alone.
#[test]
fn a_well_formed_id_the_cldr_table_does_not_know_is_not_refused() {
    let id = "Invented Standard Time";
    assert_eq!(
        zone(Some(id), None),
        None,
        "The Linux conversion has no such ID"
    );
    assert_eq!(windows_zone(Some(id), None).as_deref(), Some(id));
}

/// The override takes the place of the laptop's ID when it is set and not empty, and is used as
/// it is, with no conversion; an empty one counts as unset.
#[test]
fn the_override_takes_the_place_of_the_laptops_id_as_it_is() {
    assert_eq!(
        windows_zone(Some("Mountain Standard Time"), Some("UTC-02")).as_deref(),
        Some("UTC-02")
    );
    assert_eq!(
        windows_zone(None, Some("Pacific Standard Time (Mexico)")).as_deref(),
        Some("Pacific Standard Time (Mexico)")
    );
    assert_eq!(
        windows_zone(Some("Mountain Standard Time"), Some("")).as_deref(),
        Some("Mountain Standard Time")
    );
}

/// An override that fails the check is the empty zone, and the laptop's own ID is not used in its
/// place, as for Linux. An IANA name given as the override fails the Windows check, and the
/// legacy map does not rescue it.
#[test]
fn an_override_that_fails_the_check_is_the_empty_zone() {
    for bad in [
        "America/Denver",
        "Asia/Calcutta",
        "not a zone;",
        " ",
        "UTC-",
        "Mountain Standard Time\n",
    ] {
        assert_eq!(
            windows_zone(Some("Mountain Standard Time"), Some(bad)),
            None,
            "{bad:?}"
        );
    }
}

/// No zone at all, and a laptop ID that fails the check, are the empty zone.
#[test]
fn no_id_or_a_bad_id_is_the_empty_zone() {
    assert_eq!(windows_zone(None, None), None);
    assert_eq!(windows_zone(Some(""), None), None);
    assert_eq!(windows_zone(Some("Mountain Standard Time\n"), None), None);
}

/// The Linux host's zone is unchanged: an IANA name, the legacy map applying to the override and
/// the check refusing what it refused.
#[test]
fn a_linux_hosts_zone_is_unchanged() {
    assert_eq!(
        laptop_zone(Some("Asia/Calcutta")).as_deref(),
        Some("Asia/Kolkata")
    );
    assert_eq!(
        zone(None, Some("America/New_York")).as_deref(),
        Some("America/New_York")
    );
    assert_eq!(zone(None, Some("Mountain Standard Time")), None);
}

/// The laptop's own read: with an override the system is not read, and with none the answer is
/// the system's Windows ID, checked.
#[test]
fn the_laptops_windows_zone_is_its_own_id_or_the_override() {
    assert_eq!(
        laptop_windows_zone(Some("UTC-02")).as_deref(),
        Some("UTC-02")
    );
    assert_eq!(laptop_windows_zone(Some("America/Denver")), None);
    let own = windows_zone(windows_zone_id().as_deref(), None);
    assert_eq!(laptop_windows_zone(None), own);
    assert_eq!(laptop_windows_zone(Some("")), own);
}

/// On the laptop this runs on, the Windows host's zone is the system's own ID, not the IANA name
/// the Linux host is given for it.
#[cfg(windows)]
#[test]
fn on_windows_the_hosts_zone_is_the_systems_id_and_not_its_iana_conversion() {
    let id = windows_zone_id().expect("The system gives a zone ID");
    let for_windows = laptop_windows_zone(None).expect("A system ID passes the check");
    assert_eq!(for_windows, id);
    let for_linux = laptop_zone(None).expect("A system ID converts");
    assert!(for_linux.contains('/') || for_linux == "UTC" || for_linux.starts_with("Etc"));
    assert_ne!(for_windows, for_linux);
}
