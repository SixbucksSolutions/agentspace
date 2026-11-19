//! A removal made to fail is named home-relative (lease-api.md, Concurrency and atomicity).
//! A deny rule from `icacls` makes it, and it is lifted when the guard drops, also when a test fails.
#![cfg(windows)]

mod common;

use agentspace_driver_api::Driver;
use common::*;
use std::fs;

#[test]
fn a_failed_cleanup_after_a_failed_place_names_each_leftover_home_relative() {
    let fx = Fx::with_ca();
    fs::create_dir_all(fx.leases_dir()).unwrap();
    // A folder-only inheritable deny: the lease folder cannot be deleted, its files can.
    let deny = Deny::new(&fx.leases_dir(), "(CI)(DE,DC)");
    fs::write(fx.alias_dir(), b"mine\n").unwrap();

    let f = fx.place(600).unwrap_err();
    fx.assert_failure_plain(&f);
    assert!(
        !f.leftovers.is_empty(),
        "The folder could not be removed, so it MUST be named"
    );
    for l in &f.leftovers {
        assert!(
            l.starts_with(&format!("~/.agentspace/leases/{ID}")),
            "Unexpected leftover {l}"
        );
        assert_eq!(f.message.matches(l.as_str()).count(), 1, "{}", f.message);
    }
    assert_eq!(fs::read(fx.alias_dir()).unwrap(), b"mine\n");
    deny.lift();
    fx.assert_leftovers_exact(&f, ID);

    // A release finishes the job once the rule is gone.
    fs::remove_file(fx.alias_dir()).unwrap();
    fx.driver().release(&fx.ctx, &fx.lease()).unwrap();
    assert!(!fx.lease_dir(ID).exists());
}

#[test]
fn a_failed_cleanup_leaves_no_alias_and_still_reports_the_first_failure() {
    let fx = Fx::with_ca();
    fs::create_dir_all(fx.leases_dir()).unwrap();
    let deny = Deny::new(&fx.leases_dir(), "(CI)(DE,DC)");
    fs::write(fx.alias_dir(), b"mine\n").unwrap();
    let f = fx.place(600).unwrap_err();
    // The failure that began it is the one reported: the alias could not be set up.
    assert!(
        f.message.contains("OpenSSH client configuration"),
        "{}",
        f.message
    );
    deny.lift();
}
