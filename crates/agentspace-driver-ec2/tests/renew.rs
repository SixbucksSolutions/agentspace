//! Renew through the `Driver` trait (docs/driver.md, Renew a lease; state.md, Renewal).
//! A `Renewal` is bound to a name and held until the test commits or rolls it back.
#![cfg(windows)]

mod common;

use agentspace_driver_api::Driver;
use common::*;
use std::fs;

struct Before {
    cert: Vec<u8>,
    key: Vec<u8>,
    public: Vec<u8>,
    alias: Vec<u8>,
    config: Vec<u8>,
    expires: i64,
}

fn placed(fx: &Fx) -> Before {
    let expires = fx.place_ready(600);
    Before {
        cert: fs::read(fx.cert()).unwrap(),
        key: fs::read(fx.key()).unwrap(),
        public: fs::read(fx.pubkey()).unwrap(),
        alias: fs::read(fx.alias_file()).unwrap(),
        config: fs::read(fx.config()).unwrap(),
        expires,
    }
}

fn only_the_original_files(fx: &Fx) {
    assert_eq!(
        Fx::names(&fx.lease_dir(ID)),
        ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"],
        "A temporary file of the renew was left"
    );
}

fn unchanged_but_the_cert(fx: &Fx, b: &Before) {
    assert_eq!(fs::read(fx.key()).unwrap(), b.key);
    assert_eq!(fs::read(fx.pubkey()).unwrap(), b.public);
    assert_eq!(fs::read(fx.alias_file()).unwrap(), b.alias);
    assert_eq!(fs::read(fx.config()).unwrap(), b.config);
}

#[test]
fn a_renew_puts_the_new_certificate_in_place_and_commit_keeps_it() {
    let fx = Fx::with_ca();
    let b = placed(&fx);
    let issued = now();
    let renewal = fx
        .driver()
        .renew(&fx.ctx, &fx.lease(), 3600)
        .unwrap_or_else(|f| panic!("{f:?}"));
    let new = fs::read(fx.cert()).unwrap();
    assert_ne!(new, b.cert, "The new certificate is not in place");
    let exp = renewal.expires_at();
    assert!(exp > b.expires);
    assert!(exp >= issued + 3600 && exp <= now() + 3600);
    let info = cert_info(&fx.cert());
    assert_eq!(info.valid_to, exp);
    assert_eq!(info.key_id, ID);
    assert_eq!(info.principals, ["ubuntu"]);
    assert_eq!(info.public_fp, fingerprint(&fx.pubkey()));
    renewal.commit();
    assert_eq!(fs::read(fx.cert()).unwrap(), new);
    only_the_original_files(&fx);
    unchanged_but_the_cert(&fx, &b);
}

#[test]
fn a_rolled_back_renew_leaves_the_old_certificate_byte_for_byte() {
    let fx = Fx::with_ca();
    let b = placed(&fx);
    let renewal = fx.driver().renew(&fx.ctx, &fx.lease(), 3600).unwrap();
    assert_ne!(fs::read(fx.cert()).unwrap(), b.cert);
    renewal.rollback().unwrap();
    assert_eq!(fs::read(fx.cert()).unwrap(), b.cert);
    only_the_original_files(&fx);
    unchanged_but_the_cert(&fx, &b);
}

#[test]
fn a_dropped_renewal_rolls_back() {
    let fx = Fx::with_ca();
    let b = placed(&fx);
    {
        let renewal = fx.driver().renew(&fx.ctx, &fx.lease(), 3600).unwrap();
        assert_ne!(fs::read(fx.cert()).unwrap(), b.cert);
        drop(renewal);
    }
    assert_eq!(fs::read(fx.cert()).unwrap(), b.cert);
    only_the_original_files(&fx);
}

#[test]
fn a_renew_of_a_lease_with_no_certificate_works_and_rollback_removes_the_new_one() {
    let fx = Fx::with_ca();
    placed(&fx);
    fs::remove_file(fx.cert()).unwrap();
    let renewal = fx.driver().renew(&fx.ctx, &fx.lease(), 3600).unwrap();
    assert!(fx.cert().is_file());
    renewal.rollback().unwrap();
    assert!(!fx.cert().exists());
    assert_eq!(
        Fx::names(&fx.lease_dir(ID)),
        ["id_ed25519", "id_ed25519.pub"]
    );

    let renewal = fx.driver().renew(&fx.ctx, &fx.lease(), 3600).unwrap();
    renewal.commit();
    assert!(fx.cert().is_file());
}

#[test]
fn a_renew_changes_only_the_certificate_and_does_not_touch_the_ssh_folder() {
    let fx = Fx::with_ca();
    let b = placed(&fx);
    let ssh_before: Vec<_> = fx
        .snapshot()
        .into_iter()
        .filter(|(k, _)| k.starts_with("home dir\\.ssh"))
        .collect();
    let renewal = fx.driver().renew(&fx.ctx, &fx.lease(), 7200).unwrap();
    renewal.commit();
    let ssh_after: Vec<_> = fx
        .snapshot()
        .into_iter()
        .filter(|(k, _)| k.starts_with("home dir\\.ssh"))
        .collect();
    assert_eq!(ssh_before, ssh_after);
    unchanged_but_the_cert(&fx, &b);
}

#[test]
fn a_renew_ignores_the_ssh_folder_and_the_override() {
    let mut fx = Fx::with_ca();
    let b = placed(&fx);
    fx.ctx.ssh_dir = fx.root.join("nowhere");
    fx.ctx.proxy_override = Some("a\nb".to_string());
    let renewal = fx.driver().renew(&fx.ctx, &fx.lease(), 3600).unwrap();
    renewal.commit();
    assert_ne!(fs::read(fx.cert()).unwrap(), b.cert);
    assert!(!fx.root.join("nowhere").exists());
}

#[test]
fn a_renew_with_no_ca_fails_and_changes_nothing_and_makes_no_ca() {
    let fx = Fx::with_ca();
    placed(&fx);
    fs::remove_dir_all(fx.ca_dir()).unwrap();
    let before = fx.snapshot();
    let f = fx
        .driver()
        .renew(&fx.ctx, &fx.lease(), 3600)
        .err()
        .expect("A renew with no CA MUST fail");
    assert_eq!(f.message, NO_CA);
    assert!(f.leftovers.is_empty());
    assert_eq!(fx.snapshot(), before);
    assert!(!fx.ca_dir().exists());
}

#[test]
fn a_renew_with_an_unusable_ca_fails_and_changes_nothing() {
    let fx = Fx::with_ca();
    placed(&fx);
    fs::write(fx.ca_dir().join("agentspace_ca.pub"), b"not a key\n").unwrap();
    let before = fx.snapshot();
    let f = fx.driver().renew(&fx.ctx, &fx.lease(), 3600).err().unwrap();
    assert_eq!(f.message, CA_UNUSABLE);
    assert_eq!(fx.snapshot(), before);
}

#[test]
fn a_renew_with_no_key_fails_in_plain_words_and_changes_nothing() {
    for missing in ["id_ed25519", "id_ed25519.pub"] {
        let fx = Fx::with_ca();
        placed(&fx);
        fs::remove_file(fx.lease_dir(ID).join(missing)).unwrap();
        let before = fx.snapshot();
        let f = fx.driver().renew(&fx.ctx, &fx.lease(), 3600).err().unwrap();
        assert_eq!(f.message, KEY_MISSING, "{missing}");
        fx.assert_failure_plain(&f);
        assert_eq!(fx.snapshot(), before, "{missing}");
    }
}

#[test]
fn a_renew_of_a_lease_that_was_never_placed_fails_in_plain_words() {
    let fx = Fx::with_ca();
    let f = fx.driver().renew(&fx.ctx, &fx.lease(), 3600).err().unwrap();
    fx.assert_failure_plain(&f);
    assert_eq!(f.message, KEY_MISSING);
}
