//! `check_release` and `release` through the `Driver` trait (docs/driver.md, Release a lease;
//! state.md, Release and what it leaves). No test here writes a state record: a release has to
//! work for a lease just placed.
#![cfg(windows)]

mod common;

use agentspace_driver_api::Driver;
use common::*;
use std::fs;

#[test]
fn check_release_of_a_placed_lease_succeeds_and_changes_nothing() {
    let fx = Fx::with_ca();
    fx.place_ready(600);
    let before = fx.snapshot();
    fx.driver().check_release(&fx.ctx, &fx.lease()).unwrap();
    assert_eq!(fx.snapshot(), before);
}

#[test]
fn check_release_of_nothing_succeeds() {
    let fx = Fx::without_ca();
    let before = fx.snapshot();
    fx.driver().check_release(&fx.ctx, &fx.lease()).unwrap();
    assert_eq!(fx.snapshot(), before);
}

#[test]
fn check_release_refuses_a_foreign_alias_file_and_changes_nothing() {
    for foreign in [
        format!("# agentspace lease {OTHER_ID}; written by agentspace\nHost {ALIAS}\n"),
        format!("Host {ALIAS}\n  User someone\n"),
        String::new(),
    ] {
        let fx = Fx::with_ca();
        fx.place_ready(600);
        fs::write(fx.alias_file(), foreign.as_bytes()).unwrap();
        let before = fx.snapshot();
        let f = fx
            .driver()
            .check_release(&fx.ctx, &fx.lease())
            .expect_err("A foreign alias file MUST be refused");
        assert_eq!(f.message, ALIAS_FOREIGN);
        fx.assert_failure_plain(&f);
        assert_eq!(fx.snapshot(), before);

        // Release refuses it too, and deletes nothing, the lease folder included.
        let f = fx.driver().release(&fx.ctx, &fx.lease()).unwrap_err();
        assert_eq!(f.message, ALIAS_FOREIGN);
        assert_eq!(fx.snapshot(), before);
    }
}

#[test]
fn check_release_refuses_a_folder_at_the_alias_path_and_changes_nothing() {
    let fx = Fx::with_ca();
    fx.place_ready(600);
    fs::remove_file(fx.alias_file()).unwrap();
    fs::create_dir_all(fx.alias_file()).unwrap();
    fs::write(fx.alias_file().join("inside"), b"x").unwrap();
    let before = fx.snapshot();
    let f = fx.driver().check_release(&fx.ctx, &fx.lease()).unwrap_err();
    fx.assert_failure_plain(&f);
    assert_eq!(fx.snapshot(), before);
    let f = fx.driver().release(&fx.ctx, &fx.lease()).unwrap_err();
    fx.assert_failure_plain(&f);
    assert_eq!(fx.snapshot(), before);
}

#[test]
fn a_file_where_the_alias_folder_has_to_be_is_refused_and_nothing_is_deleted() {
    let fx = Fx::with_ca();
    fx.place_ready(600);
    fs::remove_dir_all(fx.alias_dir()).unwrap();
    fs::write(fx.alias_dir(), b"mine\n").unwrap();
    let before = fx.snapshot();
    let f = fx.driver().check_release(&fx.ctx, &fx.lease()).unwrap_err();
    fx.assert_failure_plain(&f);
    assert_eq!(fx.snapshot(), before);
    let f = fx.driver().release(&fx.ctx, &fx.lease()).unwrap_err();
    fx.assert_failure_plain(&f);
    assert_eq!(fx.snapshot(), before);
}

#[test]
fn release_removes_the_alias_file_then_the_lease_folder_and_leaves_the_rest() {
    let fx = Fx::with_ca();
    fx.place_ready(600);
    // A leftover of a renew that was cut short goes with the folder.
    fs::write(fx.lease_dir(ID).join("renew-abc.pub"), b"x").unwrap();
    fs::write(fx.lease_dir(ID).join("renew-abc-previous-cert.pub"), b"x").unwrap();
    let config = fs::read(fx.config()).unwrap();
    let ca = fx.snapshot();
    fx.driver().release(&fx.ctx, &fx.lease()).unwrap();
    assert!(!fx.alias_file().exists());
    assert!(!fx.lease_dir(ID).exists());
    // The Include line, agentspace.d, the folder of leases and the CA stay.
    assert_eq!(fs::read(fx.config()).unwrap(), config);
    assert!(fx.alias_dir().is_dir());
    assert!(fx.leases_dir().is_dir());
    let after = fx.snapshot();
    for (k, v) in &ca {
        if k.contains("ca\\") || k.ends_with("\\ca") {
            assert_eq!(after.get(k), Some(v), "The CA changed: {k}");
        }
    }
}

#[test]
fn release_needs_no_ca() {
    let fx = Fx::with_ca();
    fx.place_ready(600);
    fs::remove_dir_all(fx.ca_dir()).unwrap();
    fx.driver().release(&fx.ctx, &fx.lease()).unwrap();
    assert!(!fx.lease_dir(ID).exists());
    assert!(!fx.alias_file().exists());
}

#[test]
fn a_second_release_succeeds_and_changes_nothing_but_the_release_log() {
    let fx = Fx::with_ca();
    fx.place_ready(600);
    fx.driver().release(&fx.ctx, &fx.lease()).unwrap();
    let mut before = fx.snapshot();
    fx.driver().release(&fx.ctx, &fx.lease()).unwrap();
    fx.driver().check_release(&fx.ctx, &fx.lease()).unwrap();
    let mut after = fx.snapshot();

    // Every release appends one line to release.log (driver.md, "Where the count is logged"), and
    // the check step none: the log is the only file that changes.
    let log = fx
        .ctx
        .state_dir
        .join("release.log")
        .strip_prefix(&fx.root)
        .unwrap()
        .display()
        .to_string();
    let lines = |s: &Snapshot| -> usize {
        s.get(&log)
            .and_then(|b| b.as_ref())
            .map_or(0, |b| b.iter().filter(|c| **c == b'\n').count())
    };
    assert_eq!(lines(&before), 1, "The first release's line");
    assert_eq!(
        lines(&after),
        2,
        "The second release's line, and none from the check"
    );
    before.remove(&log);
    after.remove(&log);
    assert_eq!(after, before);
}

#[test]
fn a_release_of_a_lease_that_never_existed_succeeds() {
    let fx = Fx::without_ca();
    fx.driver().release(&fx.ctx, &fx.lease()).unwrap();
    assert!(!fx.home.join(".agentspace").exists());
}

#[test]
fn a_release_finishes_what_a_place_left_when_the_alias_is_missing() {
    let fx = Fx::with_ca();
    fx.place_ready(600);
    fs::remove_file(fx.alias_file()).unwrap();
    fx.driver().release(&fx.ctx, &fx.lease()).unwrap();
    assert!(!fx.lease_dir(ID).exists());
}

#[test]
fn release_leaves_another_leases_files() {
    let fx = Fx::with_ca();
    fx.place_ready(600);
    fs::create_dir_all(fx.lease_dir(OTHER_ID)).unwrap();
    fs::write(fx.lease_dir(OTHER_ID).join("id_ed25519"), b"theirs").unwrap();
    fs::write(
        fx.alias_dir().join(OTHER_ALIAS),
        format!("# agentspace lease {OTHER_ID}; x\nHost {OTHER_ALIAS}\n"),
    )
    .unwrap();
    fx.driver().release(&fx.ctx, &fx.lease()).unwrap();
    assert!(fx.lease_dir(OTHER_ID).join("id_ed25519").is_file());
    assert!(fx.alias_dir().join(OTHER_ALIAS).is_file());
}

#[test]
fn a_file_where_leases_has_to_be_fails_in_plain_words() {
    let fx = Fx::without_ca();
    fs::create_dir_all(fx.home.join(".agentspace")).unwrap();
    fs::write(fx.leases_dir(), b"x").unwrap();
    let f = fx.driver().release(&fx.ctx, &fx.lease()).unwrap_err();
    fx.assert_failure_plain(&f);
    assert!(
        f.message.contains("folder for leases is a file"),
        "{}",
        f.message
    );
    assert_eq!(fs::read(fx.leases_dir()).unwrap(), b"x");
}

#[test]
fn a_release_that_cannot_remove_files_names_them_and_a_second_one_finishes() {
    let fx = Fx::with_ca();
    fx.place_ready(600);
    // The deny reaches the files already there, but not the key's own protected rules.
    let deny = Deny::new(&fx.lease_dir(ID), "(OI)(CI)(DE,DC)");
    let f = fx.driver().release(&fx.ctx, &fx.lease()).unwrap_err();
    fx.assert_failure_plain(&f);
    assert!(
        !f.leftovers.is_empty(),
        "A removal failed, so it MUST be named"
    );
    assert!(f.message.contains("could not be deleted"), "{}", f.message);
    // The alias file goes before the lease folder, so no alias points at a key that is gone.
    assert!(!fx.alias_file().exists());
    deny.lift();
    fx.assert_leftovers_exact(&f, ID);
    for l in &f.leftovers {
        assert!(l.starts_with(&format!("~/.agentspace/leases/{ID}")), "{l}");
        assert!(
            f.message.contains(l.as_str()),
            "{l} unnamed in: {}",
            f.message
        );
    }
    // The message's `<why>` names the leftovers once; no second list is appended.
    assert!(!f.message.contains("Still there:"), "{}", f.message);
    // A retry finishes the job.
    fx.driver().release(&fx.ctx, &fx.lease()).unwrap();
    assert!(!fx.lease_dir(ID).exists());
}

#[test]
fn an_alias_outside_the_home_that_cannot_be_removed_is_named_by_what_it_is() {
    let fx = Fx::with_ca_ssh_elsewhere();
    fx.place_ready(600);
    let deny = Deny::new(&fx.alias_dir(), "(OI)(CI)(DE,DC)");
    let f = fx.driver().release(&fx.ctx, &fx.lease()).unwrap_err();
    fx.assert_failure_plain(&f);
    assert_eq!(
        f.leftovers
            .iter()
            .filter(|l| **l == format!("the host alias file {ALIAS}"))
            .count(),
        1,
        "{:?}",
        f.leftovers
    );
    assert_eq!(
        f.message
            .matches(&format!("the host alias file {ALIAS}"))
            .count(),
        1,
        "{}",
        f.message
    );
    // The alias still stands, so the key it points at MUST stand too.
    assert!(fx.key().is_file());
    deny.lift();
    fx.driver().release(&fx.ctx, &fx.lease()).unwrap();
    assert!(!fx.alias_file().exists());
    assert!(!fx.lease_dir(ID).exists());
}
