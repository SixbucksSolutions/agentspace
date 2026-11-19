//! Tests for issuing a lease's credential onto disk, and for deleting a lease folder
//! (E0.05-T02-S02, #0100): state.md's The lease folder, Creation in order and Release.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every file lives in a throwaway home under
//! the temp folder, never in the real profile.
#![cfg(windows)]

mod common;

use agentspace_credential::{
    CaFailure, CredFailure, Issued, ca_failure_message, issue, remove_lease_folder,
};
use common::*;
use std::fs;

const TTL: u32 = 3600;
const USER: &str = "ubuntu";

fn ok_issue(r: Result<Issued, CredFailure>) -> Issued {
    match r {
        Ok(v) => v,
        Err(e) => panic!("Expected success, got: {}", e.message),
    }
}

fn err_issue(r: Result<Issued, CredFailure>) -> CredFailure {
    match r {
        Ok(_) => panic!("Expected a failure, got success"),
        Err(e) => e,
    }
}

fn ok_unit(r: Result<(), CredFailure>) {
    if let Err(e) = r {
        panic!("Expected success, got: {}", e.message);
    }
}

#[test]
fn issued_key_has_exactly_the_three_entries_protected_and_owned_by_the_user() {
    let h = home_with_ca();
    // A hostile parent: the lease folder passes down a grant to Everyone, which the key MUST NOT keep.
    fs::create_dir_all(h.lease_dir(ID)).unwrap();
    icacls(
        &h.lease_dir(ID),
        &["/grant", &format!("*{SID_EVERYONE}:(OI)(CI)R")],
    );
    ok_issue(issue(&h.home, ID, USER, TTL));
    let sid = user_sid();
    let a = acl(&h.lease_dir(ID).join("id_ed25519"));
    assert_eq!(a.owner, sid, "The owner MUST be the user's SID");
    assert!(a.protected, "The DACL MUST be protected");
    assert_eq!(a.aces.len(), 3, "Exactly three entries: {:?}", a.aces);
    for e in &a.aces {
        assert!(e.allow, "Every entry MUST be an allow entry");
        assert!(!e.inherited, "No entry may be inherited");
    }
    let find = |s: &str| {
        a.aces
            .iter()
            .find(|e| e.sid == s)
            .unwrap_or_else(|| panic!("No entry for {s}"))
    };
    assert_eq!(find(SID_SYSTEM).rights & FULL, FULL, "SYSTEM full control");
    assert_eq!(
        find(SID_ADMINS).rights & FULL,
        FULL,
        "Administrators full control"
    );
    assert_eq!(
        find(&sid).rights & MODIFY,
        MODIFY,
        "The user modify or full"
    );
}

#[test]
fn issued_key_loads_with_an_empty_passphrase_and_no_prompt() {
    let h = home_with_ca();
    ok_issue(issue(&h.home, ID, USER, TTL));
    let dir = h.lease_dir(ID);
    // Stdin is closed in ssh_keygen, so a prompt would fail the call.
    let (ok, out, err) = ssh_keygen(&[
        "-y",
        "-P",
        "",
        "-f",
        dir.join("id_ed25519").to_str().unwrap(),
    ]);
    assert!(ok, "Windows OpenSSH could not load the key: {err}");
    let derived: Vec<&str> = out.split_whitespace().take(2).collect();
    let public = fs::read_to_string(dir.join("id_ed25519.pub")).unwrap();
    let written: Vec<&str> = public.split_whitespace().take(2).collect();
    assert_eq!(
        derived, written,
        "The public key MUST be the private key's own"
    );
    assert_eq!(written[0], "ssh-ed25519");
}

#[test]
fn folder_holds_the_three_files_and_each_public_file_is_one_line_ending_in_lf() {
    let h = home_with_ca();
    ok_issue(issue(&h.home, ID, USER, TTL));
    let dir = h.lease_dir(ID);
    assert_eq!(
        names(&dir),
        vec!["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]
    );
    for name in ["id_ed25519.pub", "id_ed25519-cert.pub"] {
        let bytes = fs::read(dir.join(name)).unwrap();
        assert!(bytes.ends_with(b"\n"), "{name} MUST end in LF");
        assert!(!bytes.ends_with(b"\r\n"), "{name} MUST NOT end in CR LF");
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text.matches('\n').count(), 1, "{name} MUST be one line");
        assert!(!text.contains('\r'), "{name} MUST hold no CR");
        assert!(text.trim_end().starts_with("ssh-ed25519"));
    }
}

#[test]
fn certificate_fields_are_those_of_the_table() {
    let h = home_with_ca();
    let before = now();
    let issued = ok_issue(issue(&h.home, ID, USER, TTL));
    let after = now();
    let dir = h.lease_dir(ID);
    let c = cert_info(&dir.join("id_ed25519-cert.pub"));
    assert!(c.cert_type.contains("ssh-ed25519-cert-v01@openssh.com"));
    assert!(
        c.cert_type.contains("user certificate"),
        "Type: {}",
        c.cert_type
    );
    assert_eq!(c.public_fp, fingerprint(&dir.join("id_ed25519.pub")));
    assert_eq!(c.ca_fp, fingerprint(&h.ca_pub()), "Signed by the CA's key");
    assert_eq!(c.key_id, ID);
    assert_eq!(c.serial, "0");
    assert_eq!(
        c.principals,
        vec![USER.to_string()],
        "Exactly one principal"
    );
    assert!(
        c.critical.is_empty(),
        "No critical options: {:?}",
        c.critical
    );
    let mut ext = c.extensions.clone();
    ext.sort();
    assert_eq!(ext, vec!["permit-port-forwarding", "permit-pty"]);
    assert_eq!(
        c.valid_to, issued.expires_at,
        "Valid before is expires_at, exactly"
    );
    assert!(
        issued.expires_at >= before + i64::from(TTL) && issued.expires_at <= after + i64::from(TTL),
        "expires_at is the issue time plus the TTL"
    );
    let issue_time = issued.expires_at - i64::from(TTL);
    assert!(
        c.valid_from <= issue_time,
        "Valid after MUST NOT be later than the issue time"
    );
    assert!(
        c.valid_from >= issue_time - 600,
        "Valid after is back-dated by minutes, not hours"
    );
}

#[test]
fn issue_refuses_an_existing_key_and_changes_nothing() {
    let h = home_with_ca();
    let dir = h.lease_dir(ID);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("id_ed25519"), b"keep me").unwrap();
    let e = err_issue(issue(&h.home, ID, USER, TTL));
    assert!(!e.message.is_empty());
    assert_eq!(names(&dir), vec!["id_ed25519"], "Nothing added or removed");
    assert_eq!(fs::read(dir.join("id_ed25519")).unwrap(), b"keep me");
}

#[test]
fn issue_refuses_an_existing_certificate_and_changes_nothing() {
    let h = home_with_ca();
    let dir = h.lease_dir(ID);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("id_ed25519-cert.pub"), b"keep me").unwrap();
    err_issue(issue(&h.home, ID, USER, TTL));
    assert_eq!(names(&dir), vec!["id_ed25519-cert.pub"]);
    assert_eq!(
        fs::read(dir.join("id_ed25519-cert.pub")).unwrap(),
        b"keep me"
    );
}

#[test]
fn issue_uses_a_folder_that_holds_neither_file() {
    let h = home_with_ca();
    fs::create_dir_all(h.lease_dir(ID)).unwrap();
    ok_issue(issue(&h.home, ID, USER, TTL));
    assert_eq!(names(&h.lease_dir(ID)).len(), 3);
}

#[test]
fn issue_without_a_ca_fails_with_the_missing_message_and_makes_nothing() {
    let h = empty_home();
    let e = err_issue(issue(&h.home, ID, USER, TTL));
    assert_eq!(e.message, ca_failure_message(CaFailure::Missing));
    assert!(e.leftovers.is_empty());
    assert!(
        !h.lease_dir(ID).exists(),
        "The CA is checked before the folder is made"
    );
}

#[test]
fn issue_with_an_unusable_ca_fails_with_the_not_usable_message_and_makes_nothing() {
    let h = home_with_ca();
    icacls(&h.ca_key(), &["/grant", &format!("*{SID_EVERYONE}:R")]);
    let e = err_issue(issue(&h.home, ID, USER, TTL));
    assert_eq!(e.message, ca_failure_message(CaFailure::NotUsable));
    assert!(!h.lease_dir(ID).exists());
}

#[test]
fn issue_does_not_change_the_ca() {
    let h = home_with_ca();
    let before = ca_snapshot(&h);
    ok_issue(issue(&h.home, ID, USER, TTL));
    assert_eq!(before, ca_snapshot(&h));
}

#[test]
fn a_failed_signing_leaves_no_file_and_no_folder_it_made() {
    // ARCHITECT-QUESTION: Which input does the spec make fail at signing while the CA passes its
    // check? A certificate needs exactly one valid principal (state.md, The certificate), so an
    // empty principal is the choice here. Read: state.md, The certificate; Creation in order, 4.
    //
    // This test, and the one below it, depend on `issue` checking the user only at signing, which
    // is after the key and the public key are made. If the user were checked before the key, they
    // would still pass. That the failure comes after the key is shown by the unit test
    // `a_failure_at_signing_comes_after_the_key_and_the_public_key_were_made` (src/lease/tests.rs),
    // which sees the files `issue` made before it removes them.
    let h = home_with_ca();
    assert!(!h.lease_dir(ID).exists());
    let e = err_issue(issue(&h.home, ID, "", TTL));
    assert!(!e.message.is_empty());
    assert!(
        e.leftovers.is_empty(),
        "Nothing was left: {:?}",
        e.leftovers
    );
    assert!(
        !h.lease_dir(ID).exists(),
        "The folder this call made MUST be removed"
    );
    // The leases folder may remain; nothing of this lease may.
    if h.leases_dir().exists() {
        assert!(names(&h.leases_dir()).is_empty());
    }
}

#[test]
fn a_failed_signing_in_a_folder_it_did_not_make_leaves_that_folder_empty() {
    // As above, this depends on the user being checked at signing, after the key and the public
    // key were made in the folder. The folder is not this call's, so it stays; the two files are
    // this call's, so they go.
    let h = home_with_ca();
    let dir = h.lease_dir(ID);
    fs::create_dir_all(&dir).unwrap();
    let e = err_issue(issue(&h.home, ID, "", TTL));
    assert!(
        e.leftovers.is_empty(),
        "Nothing was left: {:?}",
        e.leftovers
    );
    assert!(dir.is_dir(), "A folder this call did not make MUST stay");
    assert!(
        names(&dir).is_empty(),
        "The key and the public key it made MUST be gone"
    );
}

#[test]
fn a_failure_after_the_key_is_written_removes_the_key_and_keeps_a_folder_it_did_not_make() {
    // The lease folder exists, holding neither file (so it is used), and a folder sits where the
    // public key goes, so the key is written and the next step fails.
    let h = home_with_ca();
    let dir = h.lease_dir(ID);
    fs::create_dir_all(dir.join("id_ed25519.pub")).unwrap();
    let e = err_issue(issue(&h.home, ID, USER, TTL));
    assert!(
        e.leftovers.is_empty(),
        "Everything it made was removed: {:?}",
        e.leftovers
    );
    assert!(dir.is_dir(), "A folder this call did not make MUST stay");
    assert_eq!(
        names(&dir),
        vec!["id_ed25519.pub"],
        "The key and the certificate it made MUST be gone"
    );
}

#[test]
fn a_failure_message_names_no_path_and_no_user_name() {
    let h = home_with_ca();
    fs::create_dir_all(h.lease_dir(ID).join("id_ed25519.pub")).unwrap();
    let e = err_issue(issue(&h.home, ID, USER, TTL));
    let m = e.message.to_lowercase();
    assert!(
        !m.contains(&h.home.to_string_lossy().to_lowercase()),
        "A path in: {}",
        e.message
    );
    assert!(
        !m.contains(&username().to_lowercase()),
        "The user name in: {}",
        e.message
    );
    assert!(!m.contains('\\'), "A path in: {}", e.message);
}

#[test]
fn remove_lease_folder_deletes_it_with_everything_in_it() {
    let h = home_with_ca();
    ok_issue(issue(&h.home, ID, USER, TTL));
    fs::write(
        h.lease_dir(ID).join("renew-abc.pub"),
        b"left by a killed renew",
    )
    .unwrap();
    let other = "11111111-2222-4333-8444-555555555555";
    fs::create_dir_all(h.lease_dir(other)).unwrap();
    ok_unit(remove_lease_folder(&h.home, ID));
    assert!(!h.lease_dir(ID).exists());
    assert!(
        h.lease_dir(other).is_dir(),
        "Only the lease's own folder goes"
    );
    assert!(h.leases_dir().is_dir(), "The leases folder stays");
}

#[test]
fn remove_lease_folder_of_a_missing_folder_is_ok() {
    let h = empty_home();
    ok_unit(remove_lease_folder(&h.home, ID));
    let h2 = home_with_ca();
    fs::create_dir_all(h2.leases_dir()).unwrap();
    ok_unit(remove_lease_folder(&h2.home, ID));
}

#[test]
fn remove_lease_folder_needs_no_ca() {
    // Release MUST NOT need a CA, so the removal works on a home whose CA is gone.
    let h = home_with_ca();
    ok_issue(issue(&h.home, ID, USER, TTL));
    fs::remove_dir_all(h.ca_dir()).unwrap();
    ok_unit(remove_lease_folder(&h.home, ID));
    assert!(!h.lease_dir(ID).exists());
}
