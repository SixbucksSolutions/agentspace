//! The laptop's zone, read from the system, and the override that replaces it (operator.md, The
//! time zone; The test seam). RFC 2119 keywords, and the capitals are load-bearing.

use agentspace_zone::{laptop_zone, windows_zone_id, zone};

#[test]
fn an_override_replaces_the_laptops_zone_and_goes_through_steps_2_and_3() {
    assert_eq!(
        laptop_zone(Some("America/New_York")).as_deref(),
        Some("America/New_York")
    );
    // Step 2: a legacy name is mapped.
    assert_eq!(
        laptop_zone(Some("Asia/Calcutta")).as_deref(),
        Some("Asia/Kolkata")
    );
    // Step 3: a value that fails the check gives the empty zone, and the laptop's own zone is not
    // read in its place.
    for bad in ["not a zone", "-UTC", "a;b", " ", "UTC\n"] {
        assert_eq!(laptop_zone(Some(bad)), None, "{bad:?}");
    }
}

#[test]
fn an_empty_or_missing_override_is_the_laptops_own_zone() {
    let own = zone(windows_zone_id().as_deref(), None);
    assert_eq!(laptop_zone(None), own);
    assert_eq!(laptop_zone(Some("")), own);
}

#[cfg(windows)]
#[test]
fn the_windows_zone_id_is_read_from_the_system() {
    let id = windows_zone_id().expect("The system gives a zone ID");
    assert!(!id.is_empty());
    assert!(!id.contains('\0'));
    // It is a Windows ID, which CLDR's table knows for any zone a laptop is normally set to.
    assert!(
        agentspace_zone::windows_to_iana(&id).is_some(),
        "The table does not know the system's zone ID"
    );
}

#[cfg(not(windows))]
#[test]
fn there_is_no_windows_zone_id_off_windows() {
    assert_eq!(windows_zone_id(), None);
    assert_eq!(laptop_zone(None), None);
}
