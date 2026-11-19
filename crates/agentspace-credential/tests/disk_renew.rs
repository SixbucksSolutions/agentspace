//! Tests for renewing a lease's certificate on disk (E0.05-T02-S02, #0100): state.md's Renewal.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every file lives in a throwaway home under
//! the temp folder, never in the real profile.
#![cfg(windows)]

mod common;

use agentspace_credential::{CaFailure, CredFailure, RenewGuard, ca_failure_message, issue, renew};
use common::*;
use std::fs;
use std::path::Path;

const USER: &str = "ubuntu";
const TTL: u32 = 3600;
const TTL2: u32 = 7200;
const CERT: &str = "id_ed25519-cert.pub";

fn issued_home() -> Home {
    let h = home_with_ca();
    if let Err(e) = issue(&h.home, ID, USER, TTL) {
        panic!("Setup: the issue failed: {}", e.message);
    }
    h
}

fn ok_renew(r: Result<RenewGuard, CredFailure>) -> RenewGuard {
    match r {
        Ok(g) => g,
        Err(e) => panic!("Expected success, got: {}", e.message),
    }
}

fn err_renew(r: Result<RenewGuard, CredFailure>) -> CredFailure {
    match r {
        Ok(_) => panic!("Expected a failure, got success"),
        Err(e) => e,
    }
}

/// Every file in the folder with its bytes, to show nothing changed.
fn folder_state(dir: &Path) -> Vec<(String, Vec<u8>)> {
    names(dir)
        .into_iter()
        .map(|n| {
            let b = fs::read(dir.join(&n)).unwrap();
            (n, b)
        })
        .collect()
}

const THREE: [&str; 3] = ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"];

#[test]
fn a_committed_renew_leaves_the_three_files_and_the_new_certificate() {
    let h = issued_home();
    let dir = h.lease_dir(ID);
    let old_cert = fs::read(dir.join(CERT)).unwrap();
    let key = fs::read(dir.join("id_ed25519")).unwrap();
    let public = fs::read(dir.join("id_ed25519.pub")).unwrap();
    let key_acl = acl(&dir.join("id_ed25519"));
    let before = now();
    let g = ok_renew(renew(&h.home, ID, USER, TTL2));
    let after = now();
    let expires = g.expires_at();
    assert!(
        expires >= before + i64::from(TTL2) && expires <= after + i64::from(TTL2),
        "expires_at is the issue time plus the new TTL"
    );
    g.commit();
    assert_eq!(
        names(&dir),
        THREE,
        "The renew files MUST be gone after a commit"
    );
    let new_cert = fs::read(dir.join(CERT)).unwrap();
    assert_ne!(new_cert, old_cert, "The certificate MUST be the new one");
    assert!(new_cert.ends_with(b"\n") && !new_cert.ends_with(b"\r\n"));
    assert_eq!(new_cert.iter().filter(|b| **b == b'\n').count(), 1);
    let c = cert_info(&dir.join(CERT));
    assert_eq!(c.valid_to, expires);
    assert_eq!(c.key_id, ID);
    assert_eq!(c.principals, vec![USER.to_string()]);
    assert_eq!(c.public_fp, fingerprint(&dir.join("id_ed25519.pub")));
    assert_eq!(c.ca_fp, fingerprint(&h.ca_pub()));
    assert_eq!(
        fs::read(dir.join("id_ed25519")).unwrap(),
        key,
        "renew MUST NOT replace the key"
    );
    assert_eq!(fs::read(dir.join("id_ed25519.pub")).unwrap(), public);
    assert_eq!(
        acl(&dir.join("id_ed25519")),
        key_acl,
        "The key's rules are untouched"
    );
}

#[test]
fn the_new_certificate_is_in_place_before_commit() {
    // The record is written between renew and commit, so the new certificate MUST already be there.
    let h = issued_home();
    let dir = h.lease_dir(ID);
    let old_cert = fs::read(dir.join(CERT)).unwrap();
    let g = ok_renew(renew(&h.home, ID, USER, TTL2));
    let now_cert = fs::read(dir.join(CERT)).unwrap();
    assert_ne!(now_cert, old_cert);
    assert_eq!(cert_info(&dir.join(CERT)).valid_to, g.expires_at());
    g.commit();
}

#[test]
fn a_rolled_back_renew_puts_the_old_certificate_back_byte_for_byte() {
    let h = issued_home();
    let dir = h.lease_dir(ID);
    let old_cert = fs::read(dir.join(CERT)).unwrap();
    let g = ok_renew(renew(&h.home, ID, USER, TTL2));
    assert_ne!(
        fs::read(dir.join(CERT)).unwrap(),
        old_cert,
        "Setup: the new one is in place"
    );
    if let Err(e) = g.rollback() {
        panic!("The rollback failed: {}", e.message);
    }
    assert_eq!(fs::read(dir.join(CERT)).unwrap(), old_cert, "Byte for byte");
    assert_eq!(
        names(&dir),
        THREE,
        "Every renew file MUST be deleted after a rollback"
    );
}

#[test]
fn a_rolled_back_renew_with_no_previous_certificate_deletes_the_new_one() {
    let h = issued_home();
    let dir = h.lease_dir(ID);
    fs::remove_file(dir.join(CERT)).unwrap();
    let g = ok_renew(renew(&h.home, ID, USER, TTL2)); // A missing certificate is not an error.
    assert!(
        dir.join(CERT).is_file(),
        "Setup: the new certificate is in place"
    );
    if let Err(e) = g.rollback() {
        panic!("The rollback failed: {}", e.message);
    }
    assert_eq!(names(&dir), vec!["id_ed25519", "id_ed25519.pub"]);
}

#[test]
fn a_committed_renew_with_no_previous_certificate_leaves_the_three_files() {
    let h = issued_home();
    let dir = h.lease_dir(ID);
    fs::remove_file(dir.join(CERT)).unwrap();
    ok_renew(renew(&h.home, ID, USER, TTL2)).commit();
    assert_eq!(names(&dir), THREE);
}

#[test]
fn renew_without_the_private_key_changes_nothing() {
    let h = issued_home();
    let dir = h.lease_dir(ID);
    fs::remove_file(dir.join("id_ed25519")).unwrap();
    let before = folder_state(&dir);
    let e = err_renew(renew(&h.home, ID, USER, TTL2));
    assert!(!e.message.is_empty());
    assert_eq!(folder_state(&dir), before);
}

#[test]
fn renew_without_the_public_key_changes_nothing() {
    let h = issued_home();
    let dir = h.lease_dir(ID);
    fs::remove_file(dir.join("id_ed25519.pub")).unwrap();
    let before = folder_state(&dir);
    err_renew(renew(&h.home, ID, USER, TTL2));
    assert_eq!(folder_state(&dir), before);
}

#[test]
fn renew_with_no_ca_fails_with_the_missing_message_and_changes_nothing() {
    let h = issued_home();
    let dir = h.lease_dir(ID);
    fs::remove_file(h.ca_pub()).unwrap();
    let before = folder_state(&dir);
    let e = err_renew(renew(&h.home, ID, USER, TTL2));
    assert_eq!(e.message, ca_failure_message(CaFailure::Missing));
    assert_eq!(folder_state(&dir), before);
}

#[test]
fn renew_with_an_unusable_ca_fails_with_the_not_usable_message_and_changes_nothing() {
    let h = issued_home();
    let dir = h.lease_dir(ID);
    icacls(&h.ca_key(), &["/grant", &format!("*{SID_EVERYONE}:R")]);
    let before = folder_state(&dir);
    let e = err_renew(renew(&h.home, ID, USER, TTL2));
    assert_eq!(e.message, ca_failure_message(CaFailure::NotUsable));
    assert_eq!(folder_state(&dir), before);
}

#[test]
fn a_renew_that_fails_at_signing_leaves_the_old_certificate_and_no_renew_file() {
    // ARCHITECT-QUESTION: As in disk_issue.rs, an empty principal is the input chosen to fail at
    // signing, since the spec names no other. Read: state.md, Renewal, steps 3 and 8.
    let h = issued_home();
    let dir = h.lease_dir(ID);
    let before = folder_state(&dir);
    let e = err_renew(renew(&h.home, ID, "", TTL2));
    assert!(
        e.leftovers.is_empty(),
        "Nothing was left: {:?}",
        e.leftovers
    );
    assert_eq!(folder_state(&dir), before, "The folder MUST be as it was");
}

#[test]
fn renew_does_not_change_the_ca() {
    let h = issued_home();
    let before = ca_snapshot(&h);
    ok_renew(renew(&h.home, ID, USER, TTL2)).commit();
    assert_eq!(before, ca_snapshot(&h));
}

#[test]
fn a_renew_failure_message_names_no_path_and_no_user_name() {
    let h = issued_home();
    fs::remove_file(h.lease_dir(ID).join("id_ed25519")).unwrap();
    let e = err_renew(renew(&h.home, ID, USER, TTL2));
    let m = e.message.to_lowercase();
    assert!(!m.contains(&h.home.to_string_lossy().to_lowercase()));
    assert!(!m.contains(&username().to_lowercase()));
    assert!(!m.contains('\\'));
}
