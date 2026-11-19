//! Tests for the CA's check on disk (E0.05-T02-S02, #0100): state.md's The CA, Its checks.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every CA lives in a throwaway home under
//! the temp folder, made by the operator's `ca` as conformance/lease-cli/harness.ps1 makes one.
#![cfg(windows)]

mod common;

use agentspace_credential::{CaFailure, ca_failure_message, check_ca};
use common::*;
use std::fs;

#[test]
fn operator_made_ca_passes_check_ca() {
    let h = home_with_ca();
    assert!(
        h.by_operator,
        "The operator's ca MUST have made this CA, not the fallback"
    );
    assert!(matches!(check_ca(&h.home), Ok(())));
}

#[test]
fn check_ca_does_not_change_the_ca() {
    // The CA's hashes and access rules are the same after a pass and after each failure.
    let h = home_with_ca();
    let before = ca_snapshot(&h);
    assert!(matches!(check_ca(&h.home), Ok(())));
    assert_eq!(before, ca_snapshot(&h));
    icacls(&h.ca_key(), &["/grant", &format!("*{SID_EVERYONE}:R")]);
    let loosened = ca_snapshot(&h);
    assert!(matches!(check_ca(&h.home), Err(CaFailure::NotUsable)));
    assert_eq!(
        loosened,
        ca_snapshot(&h),
        "A failed check MUST NOT repair the CA"
    );
}

#[test]
fn no_ca_folder_is_missing() {
    let h = empty_home();
    assert!(matches!(check_ca(&h.home), Err(CaFailure::Missing)));
}

#[test]
fn missing_private_key_is_missing() {
    let h = home_with_ca();
    fs::remove_file(h.ca_key()).unwrap();
    assert!(matches!(check_ca(&h.home), Err(CaFailure::Missing)));
}

#[test]
fn missing_public_key_is_missing() {
    let h = home_with_ca();
    fs::remove_file(h.ca_pub()).unwrap();
    assert!(matches!(check_ca(&h.home), Err(CaFailure::Missing)));
}

#[test]
fn a_missing_file_is_found_before_the_rules_are_read() {
    // Check 1 comes before check 2: a loosened key with no public key is Missing, not NotUsable.
    let h = home_with_ca();
    icacls(&h.ca_key(), &["/grant", &format!("*{SID_EVERYONE}:R")]);
    fs::remove_file(h.ca_pub()).unwrap();
    assert!(matches!(check_ca(&h.home), Err(CaFailure::Missing)));
}

#[test]
fn one_more_allow_entry_on_the_key_is_not_usable() {
    let h = home_with_ca();
    icacls(&h.ca_key(), &["/grant", &format!("*{SID_EVERYONE}:R")]);
    assert!(matches!(check_ca(&h.home), Err(CaFailure::NotUsable)));
}

#[test]
fn one_more_entry_for_the_user_is_not_usable() {
    // A second entry for the user's own SID breaks it too (state.md, Owner-only, 3).
    let h = home_with_ca();
    let sid = user_sid();
    icacls(&h.ca_key(), &["/deny", &format!("*{sid}:(DE)")]);
    assert!(matches!(check_ca(&h.home), Err(CaFailure::NotUsable)));
}

#[test]
fn a_deny_entry_on_the_key_is_not_usable() {
    let h = home_with_ca();
    icacls(&h.ca_key(), &["/deny", &format!("*{SID_EVERYONE}:(WD)")]);
    assert!(matches!(check_ca(&h.home), Err(CaFailure::NotUsable)));
}

#[test]
fn a_user_entry_short_of_full_control_on_the_key_is_not_usable() {
    let h = home_with_ca();
    let sid = user_sid();
    icacls(&h.ca_key(), &["/grant:r", &format!("*{sid}:(R)")]);
    assert!(matches!(check_ca(&h.home), Err(CaFailure::NotUsable)));
}

#[test]
fn inheritance_turned_on_for_the_key_is_not_usable() {
    let h = home_with_ca();
    icacls(&h.ca_key(), &["/inheritance:e"]);
    assert!(!acl(&h.ca_key()).protected, "Setup: inheritance is on");
    assert!(matches!(check_ca(&h.home), Err(CaFailure::NotUsable)));
}

#[test]
fn an_extra_entry_on_the_folder_is_not_usable() {
    // The folder is checked before the key, and its rules count too.
    let h = home_with_ca();
    icacls(
        &h.ca_dir(),
        &["/grant", &format!("*{SID_EVERYONE}:(OI)(CI)R")],
    );
    assert!(matches!(check_ca(&h.home), Err(CaFailure::NotUsable)));
}

#[test]
fn inheritance_turned_on_for_the_folder_is_not_usable() {
    let h = home_with_ca();
    icacls(&h.ca_dir(), &["/inheritance:e"]);
    assert!(!acl(&h.ca_dir()).protected, "Setup: inheritance is on");
    assert!(matches!(check_ca(&h.home), Err(CaFailure::NotUsable)));
}

#[test]
fn a_ca_in_the_operators_shape_has_the_one_entry_the_check_wants() {
    // Pins what the operator's ca makes, so a loosening test above starts from owner-only.
    let h = home_with_ca();
    let sid = user_sid();
    for p in [h.ca_dir(), h.ca_key()] {
        let a = acl(&p);
        assert_eq!(a.owner, sid);
        assert!(a.protected);
        assert_eq!(a.aces.len(), 1);
        assert_eq!(a.aces[0].sid, sid);
        assert!(a.aces[0].allow);
        assert_eq!(a.aces[0].rights & FULL, FULL);
    }
}

#[test]
fn ca_failure_messages_are_the_wording_of_lease_api() {
    let missing = ca_failure_message(CaFailure::Missing);
    let unusable = ca_failure_message(CaFailure::NotUsable);
    assert!(missing.contains("certificate authority has not been set up"));
    assert!(missing.contains("Nothing was changed"));
    assert!(unusable.contains("certificate authority is not usable"));
    assert!(unusable.contains("ca command says why"));
    for m in [missing, unusable] {
        assert!(
            !m.starts_with("agentspace lease"),
            "The CLI adds the prefix, not the library"
        );
    }
}
