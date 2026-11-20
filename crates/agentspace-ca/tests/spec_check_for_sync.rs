//! `sync`'s CA check (docs/operator.md, sync step 1, gaps FA-14 and FB-12; docs/state.md, The CA,
//! Its checks): `ca`'s steps 1, 2, 3 and 5 in that order, each with `ca`'s own message, and never
//! step 4, so a laptop with no CA fails at `<key> does not exist; run ca.` and makes nothing.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The messages of steps 1, 2, 3 and the
//! checks of step 5 are also run against both entry points in spec_ca_refusals.rs and
//! spec_ca_existing.rs; this file holds what is `check_for_sync`'s own.
#![cfg(windows)]

mod spec_common;

use std::fs;

use spec_common::*;

#[test]
fn a_sound_ca_gives_its_fingerprint_and_changes_nothing() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    let made = make_ca(&lab);
    let printed = made.lines.last().unwrap().clone();
    let before = snapshot(&lab);
    let fp = sync_ok(&lab.ctx());
    assert!(
        fp.starts_with("SHA256:"),
        "The fingerprint is not SHA256:..."
    );
    assert_eq!(fp, fingerprint_of(&lab.pubkey()));
    // The very one `ca` printed, with no label.
    same(printed, format!("Fingerprint  {fp}"));
    assert_unchanged(&lab, &before, "sync's check");
    // And the same again.
    assert_eq!(sync_ok(&lab.ctx()), fp);
}

#[test]
fn a_laptop_with_no_ca_fails_at_the_first_check_and_makes_nothing() {
    // Never step 4: sync never creates a CA (operator.md, sync step 1).
    let lab = Lab::new();
    let o = run(Entry::Sync, &lab.ctx());
    same(o.message(), m_no_key(&lab.key()));
    assert!(!lab.state_dir().exists(), "The check made the state folder");
    assert!(!lab.ca_dir().exists(), "The check made the CA's folder");
}

#[test]
fn a_ca_folder_with_neither_file_fails_the_same_way_and_stays_as_it_was() {
    let lab = Lab::new();
    fs::create_dir_all(lab.ca_dir()).unwrap();
    let open_before = read_acl(&lab.ca_dir());
    let o = run(Entry::Sync, &lab.ctx());
    same(o.message(), m_no_key(&lab.key()));
    assert!(!lab.key().exists() && !lab.pubkey().exists());
    assert!(
        read_acl(&lab.ca_dir()) == open_before,
        "The check locked the folder"
    );
}

#[test]
fn sync_never_creates_a_ca_even_where_ca_would() {
    // The same home, in turn: sync's check fails and makes nothing, then ca makes the CA.
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    assert!(agentspace_ca::check_for_sync(&lab.ctx(), &ca_env()).is_err());
    assert!(!lab.ca_dir().exists());
    make_ca(&lab);
    assert!(agentspace_ca::check_for_sync(&lab.ctx(), &ca_env()).is_ok());
}

#[test]
fn the_failure_is_one_line_that_starts_agentspace() {
    let lab = Lab::new();
    let msg = run(Entry::Sync, &lab.ctx()).message();
    assert!(msg.starts_with("agentspace: "));
    assert!(!msg.contains('\n'));
}

#[test]
fn a_loosened_ca_never_becomes_a_hosts_trusted_ca_so_the_check_fails_and_names_the_path() {
    // operator.md, sync step 1: a missing, half-made, mismatched or loosened CA fails the check.
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    make_ca(&lab);
    let _guard = AclGuard::new(&lab.key());
    set_acl(
        &lab.key(),
        &sddl(
            false,
            true,
            &[('A', "FA", &user_sid()), ('A', "FR", SID_USERS)],
        ),
    );
    let msg = agentspace_ca::check_for_sync(&lab.ctx(), &ca_env())
        .unwrap_err()
        .message;
    assert!(msg.starts_with(&m_loose_prefix(&lab.key(), &user_name())));
}

#[test]
fn a_mismatched_ca_fails_the_check_and_gives_no_fingerprint() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    make_ca(&lab);
    fs::write(lab.pubkey(), format!("{}\n", other_public_key_line())).unwrap();
    let r = agentspace_ca::check_for_sync(&lab.ctx(), &ca_env());
    same(
        r.unwrap_err().message,
        m_mismatch(&lab.pubkey(), &lab.key()),
    );
}
