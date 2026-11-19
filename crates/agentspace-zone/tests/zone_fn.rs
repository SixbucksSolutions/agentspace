//! `zone`: the laptop's Windows zone, and the override that replaces it (operator.md, The time
//! zone, and The test seam).

use agentspace_zone::{windows_to_iana, zone};

/// A few table rows as smoke tests. The whole table is checked against the spec by tests/table.rs.
#[test]
fn smoke_rows_of_the_table() {
    for (windows, iana) in [
        ("Pacific Standard Time", "America/Los_Angeles"),
        ("Eastern Standard Time", "America/New_York"),
        ("UTC", "Etc/UTC"),
        ("GMT Standard Time", "Europe/London"),
        ("Tokyo Standard Time", "Asia/Tokyo"),
        ("Dateline Standard Time", "Etc/GMT+12"),
    ] {
        assert_eq!(windows_to_iana(windows), Some(iana));
        assert_eq!(zone(Some(windows), None).as_deref(), Some(iana));
    }
}

/// The Windows zones whose CLDR name is a legacy one reach the current name through the map.
#[test]
fn windows_zones_with_legacy_names_are_mapped() {
    for (windows, cldr, current) in [
        ("India Standard Time", "Asia/Calcutta", "Asia/Kolkata"),
        ("Nepal Standard Time", "Asia/Katmandu", "Asia/Kathmandu"),
        ("Myanmar Standard Time", "Asia/Rangoon", "Asia/Yangon"),
        ("FLE Standard Time", "Europe/Kiev", "Europe/Kyiv"),
        (
            "Argentina Standard Time",
            "America/Buenos_Aires",
            "America/Argentina/Buenos_Aires",
        ),
        ("Greenland Standard Time", "America/Godthab", "America/Nuuk"),
        (
            "US Eastern Standard Time",
            "America/Indianapolis",
            "America/Indiana/Indianapolis",
        ),
    ] {
        assert_eq!(windows_to_iana(windows), Some(cldr));
        assert_eq!(zone(Some(windows), None).as_deref(), Some(current));
    }
}

/// A conversion that fails gives no zone: an unknown ID, an empty one, or none read at all.
#[test]
fn a_failed_conversion_gives_no_zone() {
    assert_eq!(zone(None, None), None);
    assert_eq!(zone(Some(""), None), None);
    assert_eq!(zone(Some("Not A Zone"), None), None);
    assert_eq!(zone(Some("Mars Standard Time"), None), None);
    assert_eq!(windows_to_iana("Not A Zone"), None);
}

/// A Windows ID is matched whole and as given: it is not an IANA name, and it is not trimmed.
#[test]
fn a_windows_id_is_matched_whole() {
    assert_eq!(zone(Some("Pacific Standard Time "), None), None);
    assert_eq!(zone(Some(" Pacific Standard Time"), None), None);
    assert_eq!(zone(Some("Pacific Standard Time\n"), None), None);
    assert_eq!(zone(Some("Pacific"), None), None);
    // An IANA name given as the Windows ID is not a Windows ID.
    assert_eq!(zone(Some("America/Los_Angeles"), None), None);
}

/// A Windows ID is matched case-sensitively: an ID in another case is not known.
#[test]
fn a_windows_id_is_matched_case_sensitively() {
    for id in [
        "pacific standard time",
        "PACIFIC STANDARD TIME",
        "Pacific standard Time",
        "utc",
        "gmt Standard Time",
    ] {
        assert_eq!(windows_to_iana(id), None, "{id:?}");
        assert_eq!(zone(Some(id), None), None, "{id:?}");
    }
}

/// A set, non-empty override takes the place of the laptop's zone: its result is the override's,
/// whatever the laptop's zone is, and a laptop zone that could not be read does not matter.
#[test]
fn the_override_replaces_the_laptops_zone() {
    assert_eq!(
        zone(Some("Pacific Standard Time"), Some("America/New_York")).as_deref(),
        Some("America/New_York")
    );
    assert_eq!(
        zone(None, Some("Asia/Tokyo")).as_deref(),
        Some("Asia/Tokyo")
    );
    assert_eq!(
        zone(Some("Not A Zone"), Some("Europe/Paris")).as_deref(),
        Some("Europe/Paris")
    );
}

/// The override is not a Windows ID: a Windows ID given as the override fails the check, since
/// it holds spaces, and does not fall back to the laptop's zone.
#[test]
fn the_override_is_not_converted_as_a_windows_id() {
    assert_eq!(
        zone(Some("Eastern Standard Time"), Some("Pacific Standard Time")),
        None
    );
    assert_eq!(zone(None, Some("UTC")).as_deref(), Some("UTC"));
}

/// An override that fails the check gives the empty zone, and does not fall back to the laptop's
/// zone, so a test reaches the push's no-zone path with `not a zone`.
#[test]
fn an_override_that_fails_the_check_gives_the_empty_zone() {
    for bad in [
        "not a zone",
        "-h",
        "America/New_York\n",
        "a'b",
        "$(id)",
        "a/../b",
        "Zürich",
        "/",
        "America//New_York",
    ] {
        assert_eq!(
            zone(Some("Pacific Standard Time"), Some(bad)),
            None,
            "{bad:?}"
        );
        assert_eq!(zone(None, Some(bad)), None, "{bad:?}");
    }
}

/// The legacy names are mapped in the override, as in any converted name.
#[test]
fn the_override_has_its_legacy_names_mapped() {
    assert_eq!(
        zone(Some("Pacific Standard Time"), Some("Asia/Calcutta")).as_deref(),
        Some("Asia/Kolkata")
    );
    assert_eq!(
        zone(None, Some("America/Indianapolis")).as_deref(),
        Some("America/Indiana/Indianapolis")
    );
}

/// An unset override means the laptop's zone.
#[test]
fn an_unset_override_means_the_laptops_zone() {
    assert_eq!(
        zone(Some("Pacific Standard Time"), None).as_deref(),
        Some("America/Los_Angeles")
    );
}

/// "Set and not empty": an empty override means the laptop's own zone (conformance.md, the zone
/// tables row), and it gives no zone when the laptop's could not be read either.
#[test]
fn an_empty_override_means_the_laptops_zone() {
    assert_eq!(
        zone(Some("Pacific Standard Time"), Some("")).as_deref(),
        Some("America/Los_Angeles")
    );
    assert_eq!(zone(None, Some("")), None);
    assert_eq!(zone(Some("Not A Zone"), Some("")), None);
}

/// A whitespace-only override is set and not empty, so it takes the place of the laptop's zone
/// and fails the check. It does not mean the laptop's zone.
#[test]
fn a_whitespace_only_override_is_used_and_fails() {
    for blank in [" ", "  ", "\t", "\n", " \r\n "] {
        assert_eq!(
            zone(Some("Pacific Standard Time"), Some(blank)),
            None,
            "{blank:?}"
        );
        assert_eq!(zone(None, Some(blank)), None, "{blank:?}");
    }
}

/// The override is not trimmed: surrounding whitespace fails the check.
#[test]
fn the_override_is_not_trimmed() {
    assert_eq!(zone(None, Some(" America/New_York")), None);
    assert_eq!(zone(None, Some("America/New_York ")), None);
}

/// Nothing the function is given makes it panic.
#[test]
fn bad_input_never_panics() {
    let long = "a".repeat(100_000);
    let slashes = "/".repeat(1000);
    for input in [
        "",
        "\0",
        "\u{1f600}",
        "\u{10ffff}",
        long.as_str(),
        slashes.as_str(),
    ] {
        let _ = zone(Some(input), None);
        let _ = zone(None, Some(input));
        let _ = zone(Some(input), Some(input));
    }
    assert!(zone(None, Some(&long)).is_some());
    assert_eq!(zone(None, Some(&slashes)), None);
}
