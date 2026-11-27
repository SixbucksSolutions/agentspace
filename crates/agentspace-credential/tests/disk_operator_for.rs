//! The operator's key and certificate for a host by its host name (E0.12-T03-S03, #0256, pin P12 of
//! v0.12-T03): `ensure_operator_key_for(home, host_name)` and `sign_operator_certificate_for(home,
//! host_name, principal, now)` (docs/state.md, A host's platform and names; A host's folder; The
//! operator's key and certificate; docs/operator.md, How the operator reaches a host; D47, D91, D95).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every file lives in a throwaway home under
//! the temp folder, and every CA is a throwaway one made in it, never Terry's. Every `ssh-keygen`
//! a test runs has its profile variables pointed into that home through `Command::env` (#0137).
//!
//! - **The key's folder is `hosts\<host name>\`**, and the certificate's key ID is
//!   `operator-<host name>`: `operator-<project>` on `linux/arm64`, byte for byte as before, and
//!   `operator-<project>.<os>-<arch>` on every other platform. **The principal is the caller's**,
//!   never derived from the name: `ubuntu` on both Linux platforms and `agent` on `windows/amd64`.
//! - **A host name is accepted exactly by D91's rule**: a project, or a project, `.` and
//!   `linux-amd64` or `windows-amd64`. `linux-arm64` as a suffix, which no host has, and anything
//!   else is refused with nothing written.
//! - **The old pair, `ensure_operator_key` and `sign_operator_certificate`, is the `linux/arm64`
//!   case of the new**: the same files, the same listing of the certificate, the same comment.
#![cfg(windows)]

mod common;

use agentspace_credential::{
    CredFailure, ensure_operator_key, ensure_operator_key_for, sign_operator_certificate,
    sign_operator_certificate_for,
};
use common::*;
use std::fs;
use std::path::Path;

const KEY: &str = "id_ed25519";
const PUB: &str = "id_ed25519.pub";
const CERT: &str = "id_ed25519-cert.pub";
/// 2027-01-15 08:00:00 UTC, far from the clock, so a window relative to the signer's own clock
/// could not give the answers these tests expect.
const NOW: i64 = 1_800_000_000;

const ARM64: &str = "demo";
const AMD64: &str = "demo.linux-amd64";
const WINDOWS: &str = "demo.windows-amd64";

fn ensured(h: &Home, host: &str) -> bool {
    match ensure_operator_key_for(&h.home, host) {
        Ok(made) => made,
        Err(e) => panic!("Expected success for {host}, got: {}", e.message),
    }
}

fn ensure_err(h: &Home, host: &str) -> CredFailure {
    match ensure_operator_key_for(&h.home, host) {
        Ok(_) => panic!("Expected a failure for {host:?}, got success"),
        Err(e) => e,
    }
}

fn signed(h: &Home, host: &str, principal: &str, now: i64) -> i64 {
    match sign_operator_certificate_for(&h.home, host, principal, now) {
        Ok(valid_before) => valid_before,
        Err(e) => panic!("Expected success for {host}, got: {}", e.message),
    }
}

fn sign_err(h: &Home, host: &str, principal: &str, now: i64) -> CredFailure {
    match sign_operator_certificate_for(&h.home, host, principal, now) {
        Ok(_) => panic!("Expected a failure for {host:?}, got success"),
        Err(e) => e,
    }
}

/// A message is agentspace's own words: a path inside the throwaway home MAY be named, and nothing
/// else of a path, no user name and no key material.
fn assert_plain(msg: &str, h: &Home) {
    assert!(!msg.is_empty(), "The message is empty");
    let m = msg.to_lowercase();
    let home = h.home.to_string_lossy().to_lowercase();
    let rest = m.replace(&home, "").replace(&home.replace('\\', "/"), "");
    assert!(
        !rest.contains(":\\") && !rest.contains(":/"),
        "A path outside the home in: {msg}"
    );
    assert!(
        !rest.contains(&username().to_lowercase()),
        "The user name in: {msg}"
    );
    assert!(!msg.contains("PRIVATE KEY"), "Key material in: {msg}");
}

fn hosts_dir(h: &Home) -> std::path::PathBuf {
    h.home.join(".agentspace").join("hosts")
}

fn no_sign_files(dir: &Path) {
    let left: Vec<String> = names(dir)
        .into_iter()
        .filter(|n| n.starts_with("sign-"))
        .collect();
    assert!(left.is_empty(), "A sign-<hex> file was left: {left:?}");
}

/// The comment, the last field, of a public key or certificate line.
fn comment_of(file: &Path) -> String {
    let text = fs::read_to_string(file).unwrap();
    let words: Vec<&str> = text.split_whitespace().collect();
    words[2..].join(" ")
}

// ---------------------------------------------------------------- ensure_operator_key_for

#[test]
fn each_host_name_has_its_own_key_in_its_own_folder_and_nothing_else_is_written() {
    let h = home_with_ca();
    for host in [ARM64, AMD64, WINDOWS] {
        assert!(ensured(&h, host), "{host}: the first call makes the key");
    }
    assert_eq!(
        names(&hosts_dir(&h)),
        vec![ARM64, AMD64, WINDOWS],
        "Three hosts of one project have three folders, named by host name"
    );
    for host in [ARM64, AMD64, WINDOWS] {
        assert_eq!(names(&h.host_dir(host)), vec![KEY, PUB], "{host}");
    }
    let keys: Vec<String> = [ARM64, AMD64, WINDOWS]
        .iter()
        .map(|host| fs::read_to_string(h.host_dir(host).join(PUB)).unwrap())
        .map(|text| text.split_whitespace().nth(1).unwrap().to_string())
        .collect();
    assert!(
        keys[0] != keys[1] && keys[1] != keys[2] && keys[0] != keys[2],
        "No two hosts share a key"
    );
    assert!(!h.leases_dir().exists());
    assert!(names(&h.home.join(".ssh")).is_empty());
}

#[test]
fn ensure_says_whether_it_made_the_key_and_never_replaces_one() {
    let h = home_with_ca();
    assert!(ensured(&h, WINDOWS));
    let before = fs::read(h.host_dir(WINDOWS).join(KEY)).unwrap();
    assert!(!ensured(&h, WINDOWS), "The second call MUST NOT make one");
    assert_eq!(
        fs::read(h.host_dir(WINDOWS).join(KEY)).unwrap(),
        before,
        "An existing key MUST never be replaced"
    );
}

#[test]
fn the_made_keys_comment_names_the_host() {
    // state.md: the comment SHOULD be `agentspace host <host name>`, and holds no line break.
    let h = home_with_ca();
    for host in [ARM64, AMD64, WINDOWS] {
        assert!(ensured(&h, host));
        let text = fs::read_to_string(h.host_dir(host).join(PUB)).unwrap();
        assert_eq!(text.trim_end().lines().count(), 1, "{host}");
        assert!(
            text.trim_end()
                .ends_with(&format!(" agentspace host {host}")),
            "{host}: {text}"
        );
    }
}

#[test]
fn the_made_private_key_has_a_lease_private_keys_access_rules_on_every_platform() {
    let h = home_with_ca();
    let sid = user_sid();
    assert!(ensured(&h, WINDOWS));
    let a = acl(&h.host_dir(WINDOWS).join(KEY));
    assert!(a.protected, "The DACL MUST be protected");
    for e in &a.aces {
        assert!(e.allow && !e.inherited);
        assert!(
            e.sid == sid || e.sid == SID_SYSTEM || e.sid == SID_ADMINS,
            "A principal other than the user, SYSTEM and Administrators can reach the key: {}",
            e.sid
        );
    }
}

// ---------------------------------------------------------------- sign_operator_certificate_for

#[test]
fn the_certificate_names_operator_host_name_and_the_callers_principal() {
    let h = home_with_ca();
    for (host, principal) in [(ARM64, "ubuntu"), (AMD64, "ubuntu"), (WINDOWS, "agent")] {
        assert!(ensured(&h, host));
        let valid_before = signed(&h, host, principal, NOW);
        assert_eq!(valid_before, NOW + 3600, "{host}: valid before");
        let c = cert_info_in(&h, &h.host_dir(host).join(CERT));
        assert_eq!(
            c.key_id,
            format!("operator-{host}"),
            "{host}: the key ID is operator-<host name>"
        );
        assert_eq!(
            c.principals,
            vec![principal.to_string()],
            "{host}: exactly one principal, the caller's"
        );
        assert!(c.cert_type.contains("user certificate"), "{host}");
        assert_eq!(c.serial, "0");
        assert!(c.critical.is_empty());
        let mut ext = c.extensions.clone();
        ext.sort();
        assert_eq!(ext, vec!["permit-port-forwarding", "permit-pty"]);
        assert_eq!(c.valid_from, NOW - 300, "{host}");
        assert_eq!(c.valid_to, NOW + 3600, "{host}");
        assert_eq!(c.public_fp, fingerprint_in(&h, &h.host_dir(host).join(PUB)));
        assert_eq!(c.ca_fp, fingerprint_in(&h, &h.ca_pub()), "Signed by the CA");
    }
}

#[test]
fn the_principal_is_the_callers_and_is_never_derived_from_the_name() {
    // The login user comes from the host's platform, which the caller knows and the name does not
    // carry (pin P17 of v0.12-T01): the same name signs with whatever principal it is given.
    let h = home_with_ca();
    assert!(ensured(&h, WINDOWS));
    signed(&h, WINDOWS, "ubuntu", NOW);
    assert_eq!(
        cert_info_in(&h, &h.host_dir(WINDOWS).join(CERT)).principals,
        vec!["ubuntu".to_string()]
    );
    assert!(ensured(&h, ARM64));
    signed(&h, ARM64, "agent", NOW);
    assert_eq!(
        cert_info_in(&h, &h.host_dir(ARM64).join(CERT)).principals,
        vec!["agent".to_string()]
    );
}

#[test]
fn two_hosts_of_one_project_get_two_certificates_for_two_keys() {
    let h = home_with_ca();
    for host in [ARM64, WINDOWS] {
        assert!(ensured(&h, host));
    }
    signed(&h, ARM64, "ubuntu", NOW);
    signed(&h, WINDOWS, "agent", NOW);
    let a = cert_info_in(&h, &h.host_dir(ARM64).join(CERT));
    let b = cert_info_in(&h, &h.host_dir(WINDOWS).join(CERT));
    assert_ne!(
        a.public_fp, b.public_fp,
        "Each certificate is for its own key"
    );
    assert_eq!(a.key_id, "operator-demo");
    assert_eq!(b.key_id, "operator-demo.windows-amd64");
}

#[test]
fn a_new_certificate_replaces_the_old_one_and_leaves_no_sign_file() {
    let h = home_with_ca();
    assert!(ensured(&h, AMD64));
    let dir = h.host_dir(AMD64);
    signed(&h, AMD64, "ubuntu", NOW);
    let key = fs::read(dir.join(KEY)).unwrap();
    let first = fs::read(dir.join(CERT)).unwrap();
    signed(&h, AMD64, "ubuntu", NOW + 10);
    assert_ne!(first, fs::read(dir.join(CERT)).unwrap());
    assert_eq!(key, fs::read(dir.join(KEY)).unwrap(), "The key stays");
    assert_eq!(names(&dir), vec![KEY, CERT, PUB]);
    no_sign_files(&dir);
}

#[test]
fn a_signing_without_a_key_fails_in_plain_words_and_leaves_no_sign_file() {
    let h = home_with_ca();
    fs::create_dir_all(h.host_dir(WINDOWS)).unwrap();
    let e = sign_err(&h, WINDOWS, "agent", NOW);
    assert_plain(&e.message, &h);
    assert!(!h.host_dir(WINDOWS).join(CERT).exists());
    no_sign_files(&h.host_dir(WINDOWS));
}

#[test]
fn a_ca_that_is_missing_fails_first_and_writes_nothing() {
    let h = empty_home();
    let e = sign_err(&h, WINDOWS, "agent", NOW);
    assert_plain(&e.message, &h);
    assert!(!hosts_dir(&h).exists(), "Nothing was written");
}

// ---------------------------------------------------------------- The host name's rule

#[test]
fn a_project_and_the_two_other_platforms_names_are_accepted() {
    let h = home_with_ca();
    let longest = "a".repeat(32);
    let hosts = [
        "x".to_string(),
        "demo-2".to_string(),
        "a0-b1-c2".to_string(),
        longest.clone(),
        format!("{longest}.linux-amd64"),
        format!("{longest}.windows-amd64"),
        "demo-2.linux-amd64".to_string(),
        "x.windows-amd64".to_string(),
    ];
    for host in &hosts {
        assert!(ensured(&h, host), "{host} is a host name");
        signed(&h, host, "ubuntu", NOW);
        assert_eq!(
            cert_info_in(&h, &h.host_dir(host).join(CERT)).key_id,
            format!("operator-{host}")
        );
    }
}

#[test]
fn a_name_that_is_not_a_hosts_is_refused_and_writes_nothing() {
    let h = home_with_ca();
    let too_long = "a".repeat(33);
    let bad: Vec<String> = [
        // linux/arm64 never appears in a name (D91).
        "demo.linux-arm64",
        "demo.windows-arm64",
        // A platform that is not supported, or not written as `<os>-<arch>`.
        "demo.macos-arm64",
        "demo.linux-amd64v3",
        "demo.linux_amd64",
        "demo.linux/amd64",
        "demo.windows",
        "demo.amd64",
        "demo.Linux-amd64",
        "demo.LINUX-AMD64",
        "demo.windows-amd64.linux-amd64",
        "demo.windows-amd64.",
        "demo.windows-amd64 ",
        " demo.windows-amd64",
        "demo.windows-amd64\n",
        // No project, or one that is not a normalized project.
        "",
        ".",
        "..",
        ".windows-amd64",
        "..windows-amd64",
        "demo..windows-amd64",
        "demo.",
        "a.b",
        "-a.windows-amd64",
        "a-.windows-amd64",
        "Demo.windows-amd64",
        "Demo",
        "a b",
        "é.windows-amd64",
        // A name that would reach outside hosts\.
        "../x",
        "..\\x",
        "a/b",
        "a\\b",
        "demo.windows-amd64/..",
        "demo.windows-amd64\\..\\..",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([
        format!("{too_long}.windows-amd64"),
        format!("{too_long}.linux-amd64"),
        too_long.clone(),
    ])
    .collect();
    for host in &bad {
        let e = ensure_err(&h, host);
        assert_plain(&e.message, &h);
        let e = sign_err(&h, host, "ubuntu", NOW);
        assert_plain(&e.message, &h);
        assert!(
            !hosts_dir(&h).exists(),
            "{host:?} made the hosts folder or something in it"
        );
    }
    assert!(!h.leases_dir().exists());
}

#[test]
fn a_refused_name_changes_no_file_of_a_host_that_has_a_key() {
    let h = home_with_ca();
    assert!(ensured(&h, WINDOWS));
    signed(&h, WINDOWS, "agent", NOW);
    let snap = |dir: &Path| -> Vec<(String, Vec<u8>)> {
        names(dir)
            .into_iter()
            .map(|n| (n.clone(), fs::read(dir.join(&n)).unwrap()))
            .collect()
    };
    let before = snap(&h.host_dir(WINDOWS));
    ensure_err(&h, "demo.linux-arm64");
    sign_err(&h, "demo.linux-arm64", "ubuntu", NOW);
    assert_eq!(snap(&h.host_dir(WINDOWS)), before);
    assert_eq!(names(&hosts_dir(&h)), vec![WINDOWS]);
}

// ---------------------------------------------------------------- The old pair is the new pair's linux/arm64 case

#[test]
fn a_linux_arm64_certificate_from_the_new_pair_is_what_the_old_pair_makes() {
    // The same key, the same CA and the same time, signed once by the old function and once by the
    // new: ssh-keygen -L lists the two alike, and so does the comment. (A certificate carries a
    // random nonce, so the bytes of two signings differ; the listing is what the nonce is not in.)
    let h = home_with_ca();
    assert!(
        ensure_operator_key(&h.home, "demo").is_ok_and(|made| made),
        "Setup: the old pair makes the key"
    );
    let dir = h.host_dir("demo");
    let cert = dir.join(CERT);
    sign_operator_certificate(&h.home, "demo", NOW).unwrap();
    let old = cert_info_in(&h, &cert);
    let old_comment = comment_of(&cert);
    signed(&h, "demo", "ubuntu", NOW);
    let new = cert_info_in(&h, &cert);
    let new_comment = comment_of(&cert);
    assert_eq!(old.cert_type, new.cert_type);
    assert_eq!(old.public_fp, new.public_fp);
    assert_eq!(old.ca_fp, new.ca_fp);
    assert_eq!(old.key_id, new.key_id);
    assert_eq!(new.key_id, "operator-demo");
    assert_eq!(old.serial, new.serial);
    assert_eq!(old.valid_from, new.valid_from);
    assert_eq!(old.valid_to, new.valid_to);
    assert_eq!(old.principals, new.principals);
    assert_eq!(old.critical, new.critical);
    assert_eq!(old.extensions, new.extensions);
    assert_eq!(old_comment, new_comment);
    assert_eq!(new_comment, "agentspace host demo");
    assert_eq!(names(&dir), vec![KEY, CERT, PUB]);
}

#[test]
fn a_linux_arm64_key_from_the_new_pair_is_what_the_old_pair_makes() {
    // Two homes, one key from each pair: the same files, the same comment, the same access rules,
    // the same format and the same answers.
    let a = home_with_ca();
    let b = home_with_ca();
    assert!(ensure_operator_key(&a.home, "demo").unwrap());
    assert!(ensured(&b, "demo"));
    assert!(!ensure_operator_key(&a.home, "demo").unwrap());
    assert!(!ensured(&b, "demo"));
    assert_eq!(names(&a.host_dir("demo")), names(&b.host_dir("demo")));
    assert_eq!(
        comment_of(&a.host_dir("demo").join(PUB)),
        comment_of(&b.host_dir("demo").join(PUB))
    );
    assert_eq!(
        acl(&a.host_dir("demo").join(KEY)),
        acl(&b.host_dir("demo").join(KEY))
    );
    for h in [&a, &b] {
        let private = fs::read_to_string(h.host_dir("demo").join(KEY)).unwrap();
        assert!(private.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        let public = fs::read_to_string(h.host_dir("demo").join(PUB)).unwrap();
        assert!(public.starts_with("ssh-ed25519 "));
    }
}

#[test]
fn the_old_pairs_key_is_the_new_pairs_for_the_same_name_and_the_other_way_round() {
    // Neither pair replaces a key the other made: they read and write the same folder.
    let h = home_with_ca();
    assert!(ensure_operator_key(&h.home, "demo").unwrap());
    let key = fs::read(h.host_dir("demo").join(KEY)).unwrap();
    assert!(
        !ensured(&h, "demo"),
        "The new pair finds the old pair's key"
    );
    assert_eq!(fs::read(h.host_dir("demo").join(KEY)).unwrap(), key);

    let h = home_with_ca();
    assert!(ensured(&h, "demo"));
    let key = fs::read(h.host_dir("demo").join(KEY)).unwrap();
    assert!(
        !ensure_operator_key(&h.home, "demo").unwrap(),
        "The old pair finds the new pair's key"
    );
    assert_eq!(fs::read(h.host_dir("demo").join(KEY)).unwrap(), key);
}
