//! Tests for the operator's key and certificate on disk (E0.07-T02-S03, #0148): state.md's A host's
//! folder and The operator's key and certificate.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every file lives in a throwaway home under
//! the temp folder, and every CA is a throwaway one made in it, never Terry's. Every `ssh-keygen`
//! a test runs has its profile variables pointed into that home through `Command::env` (#0137).
#![cfg(windows)]

mod common;

use agentspace_credential::{
    CaFailure, CredFailure, ca_failure_message, ensure_operator_key, sign_operator_certificate,
};
use common::*;
use std::fs;
use std::path::Path;

const PROJECT: &str = "examplehost";
const KEY: &str = "id_ed25519";
const PUB: &str = "id_ed25519.pub";
const CERT: &str = "id_ed25519-cert.pub";
/// 2027-01-15 08:00:00 UTC, far from the clock, so a window relative to the signer's own clock
/// could not give the answers these tests expect.
const NOW: i64 = 1_800_000_000;

fn ensured(h: &Home, project: &str) -> bool {
    match ensure_operator_key(&h.home, project) {
        Ok(made) => made,
        Err(e) => panic!("Expected success, got: {}", e.message),
    }
}

fn ensure_err(h: &Home, project: &str) -> CredFailure {
    match ensure_operator_key(&h.home, project) {
        Ok(_) => panic!("Expected a failure, got success"),
        Err(e) => e,
    }
}

fn signed(h: &Home, project: &str, now: i64) -> i64 {
    match sign_operator_certificate(&h.home, project, now) {
        Ok(valid_before) => valid_before,
        Err(e) => panic!("Expected success, got: {}", e.message),
    }
}

fn sign_err(h: &Home, project: &str, now: i64) -> CredFailure {
    match sign_operator_certificate(&h.home, project, now) {
        Ok(_) => panic!("Expected a failure, got success"),
        Err(e) => e,
    }
}

/// A home with a CA, and the operator's key made in the host's folder.
fn keyed_home() -> Home {
    let h = home_with_ca();
    assert!(ensured(&h, PROJECT), "Setup: the key was not made");
    h
}

/// Every entry of a folder with its bytes (a folder's entry holds its own names instead).
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    names(dir)
        .into_iter()
        .map(|n| {
            let p = dir.join(&n);
            let bytes = if p.is_dir() {
                format!("<folder {:?}>", names(&p)).into_bytes()
            } else {
                fs::read(&p).unwrap()
            };
            (n, bytes)
        })
        .collect()
}

/// The first two words of a public key file, whatever its line ending.
fn two_words(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .split_whitespace()
        .take(2)
        .map(str::to_string)
        .collect()
}

/// The public key `ssh-keygen` derives from a private key with an empty passphrase, first two
/// words. Stdin is closed in the helper, so a prompt fails the call.
fn derived(h: &Home, key: &Path) -> Vec<String> {
    let (ok, out, err) = ssh_keygen_in(h, &["-y", "-P", "", "-f", key.to_str().unwrap()]);
    assert!(ok, "Windows OpenSSH could not load the key: {err}");
    two_words(out.as_bytes())
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

fn no_sign_files(dir: &Path) {
    let left: Vec<String> = names(dir)
        .into_iter()
        .filter(|n| n.starts_with("sign-"))
        .collect();
    assert!(left.is_empty(), "A sign-<hex> file was left: {left:?}");
}

// ---------------------------------------------------------------- ensure_operator_key

#[test]
fn the_key_is_made_once_and_ensure_says_so() {
    let h = home_with_ca();
    assert!(!h.host_dir(PROJECT).exists(), "Setup: no host folder yet");
    assert!(ensured(&h, PROJECT), "The first call MUST make the key");
    assert_eq!(names(&h.host_dir(PROJECT)), vec![KEY, PUB]);
    assert!(!ensured(&h, PROJECT), "The second call MUST NOT make one");
    assert!(!ensured(&h, PROJECT), "Nor the third");
    assert_eq!(names(&h.host_dir(PROJECT)), vec![KEY, PUB]);
}

#[test]
fn the_made_key_is_ed25519_unencrypted_and_its_public_key_is_its_own() {
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    // ssh-keygen loads it with an empty passphrase and no prompt, which an encrypted key would not.
    let words = derived(&h, &dir.join(KEY));
    assert_eq!(words[0], "ssh-ed25519");
    let public = fs::read(dir.join(PUB)).unwrap();
    assert_eq!(
        two_words(&public),
        words,
        "The public key MUST be the private key's own"
    );
    // One line, in OpenSSH's form, which a reader reads with either line ending.
    let text = String::from_utf8(public).unwrap();
    assert_eq!(text.trim_end().lines().count(), 1, "One line");
    assert!(text.starts_with("ssh-ed25519 "));
    let private = fs::read_to_string(dir.join(KEY)).unwrap();
    assert!(
        private.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"),
        "OpenSSH's own private key format"
    );
}

#[test]
fn the_made_keys_comment_is_agentspace_host_and_the_project() {
    // The comment SHOULD be `agentspace host <project>`, and MUST hold no line break.
    let h = keyed_home();
    let text = fs::read_to_string(h.host_dir(PROJECT).join(PUB)).unwrap();
    let line = text.trim_end();
    assert_eq!(line.lines().count(), 1, "No line break in the comment");
    assert!(
        line.ends_with(" agentspace host examplehost"),
        "The comment is: {line}"
    );
}

#[test]
fn the_made_private_key_is_openssh_safe_and_readable_by_the_user_system_and_administrators_only() {
    let h = home_with_ca();
    // A hostile parent: the host's folder passes down a grant to Everyone, which the key MUST NOT keep.
    fs::create_dir_all(h.host_dir(PROJECT)).unwrap();
    icacls(
        &h.host_dir(PROJECT),
        &["/grant", &format!("*{SID_EVERYONE}:(OI)(CI)R")],
    );
    assert!(ensured(&h, PROJECT));
    let sid = user_sid();
    let a = acl(&h.host_dir(PROJECT).join(KEY));
    assert!(
        a.owner == sid || a.owner == SID_ADMINS || a.owner == SID_SYSTEM,
        "The owner MUST be the user, SYSTEM or Administrators"
    );
    assert!(a.protected, "The DACL MUST be protected");
    for e in &a.aces {
        assert!(e.allow, "Every entry MUST be an allow entry");
        assert!(!e.inherited, "No entry may be inherited");
        assert!(
            e.sid == sid || e.sid == SID_SYSTEM || e.sid == SID_ADMINS,
            "A principal other than the user, SYSTEM and Administrators can reach the key: {}",
            e.sid
        );
    }
    let user_entry = a
        .aces
        .iter()
        .find(|e| e.sid == sid)
        .expect("The user MUST have an entry");
    assert_eq!(
        user_entry.rights & MODIFY,
        MODIFY,
        "The user modify or full"
    );
    // OpenSSH accepts it: a key another principal can read is refused as unprotected.
    derived(&h, &h.host_dir(PROJECT).join(KEY));
}

#[test]
fn an_existing_key_is_kept_byte_for_byte() {
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    let before = snapshot(&dir);
    let key_acl = acl(&dir.join(KEY));
    assert!(!ensured(&h, PROJECT));
    assert_eq!(
        snapshot(&dir),
        before,
        "An existing key MUST never be replaced"
    );
    assert_eq!(acl(&dir.join(KEY)), key_acl);
}

#[test]
fn a_key_a_person_made_with_ssh_keygen_is_kept_byte_for_byte() {
    let h = home_with_ca();
    let dir = h.host_dir(PROJECT);
    fs::create_dir_all(&dir).unwrap();
    let (ok, _, err) = ssh_keygen_in(
        &h,
        &[
            "-q",
            "-t",
            "ed25519",
            "-N",
            "",
            "-C",
            "a person's own key",
            "-f",
            dir.join(KEY).to_str().unwrap(),
        ],
    );
    assert!(ok, "ssh-keygen could not make the key: {err}");
    let before = snapshot(&dir);
    assert!(!ensured(&h, PROJECT), "A good key is not made again");
    assert_eq!(snapshot(&dir), before);
    // Its public key ends in CR LF on Windows, which the writer's reader accepts. Signing works.
    assert_eq!(signed(&h, PROJECT, NOW), NOW + 3600);
}

#[test]
fn a_leftover_public_key_or_certificate_without_the_private_key_is_replaced() {
    let leftover_pub: &[u8] = b"ssh-ed25519 AAAAleftover leftover pub\r\n";
    let leftover_cert: &[u8] = b"ssh-ed25519-cert-v01@openssh.com AAAAleftover leftover cert\r\n";
    for (tag, with_pub, with_cert) in [
        ("pub only", true, false),
        ("cert only", false, true),
        ("both", true, true),
    ] {
        let h = home_with_ca();
        let dir = h.host_dir(PROJECT);
        fs::create_dir_all(&dir).unwrap();
        if with_pub {
            fs::write(dir.join(PUB), leftover_pub).unwrap();
        }
        if with_cert {
            fs::write(dir.join(CERT), leftover_cert).unwrap();
        }
        assert!(ensured(&h, PROJECT), "{tag}: a missing key is made");
        let key_words = derived(&h, &dir.join(KEY));
        let public = fs::read(dir.join(PUB)).unwrap();
        assert_ne!(public, leftover_pub, "{tag}: the leftover public key stays");
        assert_eq!(
            two_words(&public),
            key_words,
            "{tag}: the public key MUST be the new key's"
        );
        if let Ok(cert) = fs::read(dir.join(CERT)) {
            assert_ne!(cert, leftover_cert, "{tag}: the leftover certificate stays");
        }
    }
}

#[test]
fn a_key_that_does_not_load_or_is_not_ed25519_is_refused_and_nothing_changes() {
    // Every case holds a person's public key and certificate beside the bad private key, which MUST
    // stay too: an existing key is never replaced, and nothing else is touched.
    let sentinel = "SENTINEL-DO-NOT-ECHO";
    type Make = Box<dyn Fn(&Home, &Path)>;
    let cases: Vec<(&str, Make)> = vec![
        (
            "garbage",
            Box::new(move |_, key| {
                fs::write(key, format!("not a key {sentinel}\n")).unwrap();
            }),
        ),
        (
            "empty",
            Box::new(|_, key| {
                fs::write(key, b"").unwrap();
            }),
        ),
        (
            "ecdsa",
            Box::new(|h, key| {
                let (ok, _, err) = ssh_keygen_in(
                    h,
                    &[
                        "-q",
                        "-t",
                        "ecdsa",
                        "-b",
                        "256",
                        "-N",
                        "",
                        "-f",
                        key.to_str().unwrap(),
                    ],
                );
                assert!(ok, "ssh-keygen could not make the ecdsa key: {err}");
            }),
        ),
        (
            "passphrase",
            Box::new(|h, key| {
                let (ok, _, err) = ssh_keygen_in(
                    h,
                    &[
                        "-q",
                        "-t",
                        "ed25519",
                        "-N",
                        "test-only-passphrase",
                        "-f",
                        key.to_str().unwrap(),
                    ],
                );
                assert!(ok, "ssh-keygen could not make the encrypted key: {err}");
            }),
        ),
        (
            "folder",
            Box::new(|_, key| {
                fs::create_dir(key).unwrap();
                fs::write(key.join("inside.txt"), b"x").unwrap();
            }),
        ),
    ];
    for (tag, make) in cases {
        let h = home_with_ca();
        let dir = h.host_dir(PROJECT);
        fs::create_dir_all(&dir).unwrap();
        make(&h, &dir.join(KEY));
        fs::write(dir.join(PUB), b"a person's public key\r\n").unwrap();
        fs::write(dir.join(CERT), b"a person's certificate\r\n").unwrap();
        let before = snapshot(&dir);
        let e = ensure_err(&h, PROJECT);
        assert!(
            e.message.contains(KEY),
            "{tag}: the message MUST name the file: {}",
            e.message
        );
        assert!(
            !e.message.contains(sentinel),
            "{tag}: the file's text in the message"
        );
        assert_plain(&e.message, &h);
        assert_eq!(
            snapshot(&dir),
            before,
            "{tag}: nothing in the folder MAY change"
        );
    }
}

#[test]
fn a_file_where_hosts_or_the_hosts_folder_must_be_is_refused_and_writes_nothing() {
    // State.md, A host's folder: something at `hosts\` or `hosts\<project>\` that is not a folder
    // fails every command that needs a file in it.
    let h = home_with_ca();
    let hosts = h.home.join(".agentspace").join("hosts");
    fs::write(&hosts, b"not a folder").unwrap();
    let e = ensure_err(&h, PROJECT);
    assert_plain(&e.message, &h);
    let e = sign_err(&h, PROJECT, NOW);
    assert_plain(&e.message, &h);
    assert_eq!(fs::read(&hosts).unwrap(), b"not a folder");

    let h = home_with_ca();
    fs::create_dir_all(h.home.join(".agentspace").join("hosts")).unwrap();
    fs::write(h.host_dir(PROJECT), b"not a folder").unwrap();
    ensure_err(&h, PROJECT);
    sign_err(&h, PROJECT, NOW);
    assert_eq!(fs::read(h.host_dir(PROJECT)).unwrap(), b"not a folder");
    assert_eq!(
        names(&h.home.join(".agentspace").join("hosts")),
        vec![PROJECT]
    );
}

#[test]
fn each_project_has_its_own_key_in_its_own_folder_and_nothing_else_is_written() {
    let h = home_with_ca();
    assert!(ensured(&h, PROJECT));
    assert!(ensured(&h, "other-2"), "Another project's key is made too");
    assert_eq!(
        names(&h.home.join(".agentspace").join("hosts")),
        vec!["examplehost", "other-2"]
    );
    assert_ne!(
        two_words(&fs::read(h.host_dir(PROJECT).join(PUB)).unwrap()),
        two_words(&fs::read(h.host_dir("other-2").join(PUB)).unwrap())
    );
    // No lease folder, nothing in the ssh folder, and the CA as it was.
    assert!(!h.leases_dir().exists());
    assert!(names(&h.home.join(".ssh")).is_empty());
}

#[test]
fn ensure_does_not_change_the_ca() {
    let h = home_with_ca();
    let before = ca_snapshot(&h);
    assert!(ensured(&h, PROJECT));
    assert_eq!(before, ca_snapshot(&h));
}

// ---------------------------------------------------------------- sign_operator_certificate

#[test]
fn the_certificate_has_the_principal_key_id_and_extensions_of_state_md() {
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    let valid_before = signed(&h, PROJECT, NOW);
    assert_eq!(
        valid_before,
        NOW + 3600,
        "Valid before is the issue time plus 3600"
    );
    let c = cert_info_in(&h, &dir.join(CERT));
    assert!(c.cert_type.contains("ssh-ed25519-cert-v01@openssh.com"));
    assert!(
        c.cert_type.contains("user certificate"),
        "A user certificate, never a host's: {}",
        c.cert_type
    );
    assert_eq!(
        c.principals,
        vec!["ubuntu".to_string()],
        "Exactly one principal, ubuntu"
    );
    assert_eq!(c.key_id, "operator-examplehost");
    assert_eq!(c.serial, "0");
    assert!(
        c.critical.is_empty(),
        "No critical options: {:?}",
        c.critical
    );
    let mut ext = c.extensions.clone();
    ext.sort();
    assert_eq!(
        ext,
        vec!["permit-port-forwarding", "permit-pty"],
        "A lease certificate's two extensions"
    );
    assert_eq!(c.public_fp, fingerprint_in(&h, &dir.join(PUB)));
    assert_eq!(
        c.ca_fp,
        fingerprint_in(&h, &h.ca_pub()),
        "Signed by the CA's key"
    );
}

#[test]
fn the_key_id_follows_the_project() {
    let h = home_with_ca();
    for project in [
        "demo-2",
        "x",
        "a0-b1-c2",
        "abcdefghijklmnopqrstuvwxyz012345",
    ] {
        assert!(ensured(&h, project));
        signed(&h, project, NOW);
        let c = cert_info_in(&h, &h.host_dir(project).join(CERT));
        assert_eq!(c.key_id, format!("operator-{project}"));
        assert_eq!(c.principals, vec!["ubuntu".to_string()]);
    }
}

#[test]
fn a_project_that_is_not_a_normalized_name_is_refused_and_writes_nothing() {
    // The host folder is named by the host record's `project`, whose form is state.md's
    // `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$` (The host registry), so nothing else names a folder,
    // and no name reaches outside `hosts\`.
    let h = home_with_ca();
    let too_long = "a".repeat(33);
    for bad in [
        "", "..", ".", "a/b", "a\\b", "..\\x", "Upper", "-a", "a-", "a b", "a\nb", "é", &too_long,
    ] {
        let e = ensure_err(&h, bad);
        assert_plain(&e.message, &h);
        let e = sign_err(&h, bad, NOW);
        assert_plain(&e.message, &h);
        assert!(
            !h.home.join(".agentspace").join("hosts").exists(),
            "{bad:?} made a folder"
        );
    }
    assert!(!h.leases_dir().exists());
}

#[test]
fn the_window_is_absolute_and_valid_before_is_the_issue_time_plus_one_hour() {
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    // Times far from the clock, past and future, and the clock's own: the window follows `now`.
    for now in [1_700_000_000_i64, NOW, common::now()] {
        let valid_before = signed(&h, PROJECT, now);
        assert_eq!(valid_before, now + 3600, "The returned valid-before time");
        let c = cert_info_in(&h, &dir.join(CERT));
        assert_eq!(
            c.valid_to,
            now + 3600,
            "Valid before, exactly, as ssh-keygen -L reads it"
        );
        assert!(
            c.valid_from <= now,
            "Valid after MUST NOT be later than the issue time"
        );
        assert!(
            c.valid_from >= now - 600,
            "Valid after is back-dated by minutes, not hours"
        );
    }
}

#[test]
fn valid_after_is_five_minutes_before_the_issue_time() {
    // The SHOULD of state.md, as a lease's: 5 minutes before the issue time (D24, #0029).
    let h = keyed_home();
    signed(&h, PROJECT, NOW);
    let c = cert_info_in(&h, &h.host_dir(PROJECT).join(CERT));
    assert_eq!(c.valid_from, NOW - 300);
    assert_eq!(
        c.valid_to,
        NOW + 3600,
        "The back-dating never moves valid-before"
    );
}

#[test]
fn signing_leaves_the_three_files_and_no_sign_file_and_changes_neither_key_nor_ca() {
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    let key = fs::read(dir.join(KEY)).unwrap();
    let public = fs::read(dir.join(PUB)).unwrap();
    let key_acl = acl(&dir.join(KEY));
    let ca_before = ca_snapshot(&h);
    signed(&h, PROJECT, NOW);
    assert_eq!(
        names(&dir),
        vec![KEY, CERT, PUB],
        "No sign-<hex> file is left"
    );
    no_sign_files(&dir);
    assert_eq!(
        fs::read(dir.join(KEY)).unwrap(),
        key,
        "The key is never replaced"
    );
    assert_eq!(fs::read(dir.join(PUB)).unwrap(), public);
    assert_eq!(acl(&dir.join(KEY)), key_acl);
    assert_eq!(ca_before, ca_snapshot(&h), "Signing MUST NOT change the CA");
    // One line, in the certificate's format, which a reader reads with either line ending.
    let text = String::from_utf8(fs::read(dir.join(CERT)).unwrap()).unwrap();
    assert_eq!(text.trim_end().lines().count(), 1, "One line");
    assert!(
        text.starts_with("ssh-ed25519-cert-v01@openssh.com "),
        "The certificate's own format"
    );
}

#[test]
fn a_new_certificate_replaces_the_old_one_and_the_key_stays() {
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    signed(&h, PROJECT, 1_700_000_000);
    let old = fs::read(dir.join(CERT)).unwrap();
    let key = fs::read(dir.join(KEY)).unwrap();
    assert_eq!(signed(&h, PROJECT, NOW), NOW + 3600);
    let new = fs::read(dir.join(CERT)).unwrap();
    assert_ne!(new, old, "The certificate MUST be the new one");
    assert_eq!(cert_info_in(&h, &dir.join(CERT)).valid_to, NOW + 3600);
    assert_eq!(fs::read(dir.join(KEY)).unwrap(), key);
    assert_eq!(names(&dir), vec![KEY, CERT, PUB]);
}

#[test]
fn signing_leaves_a_persons_other_files_in_the_folder_alone() {
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    fs::write(dir.join("provision.log"), b"2026-11-20T00:00:00Z a line\n").unwrap();
    fs::write(dir.join("provision.lock"), b"").unwrap();
    fs::write(dir.join("localswim-pushed.json"), b"{\"version\":0}").unwrap();
    fs::write(dir.join("notes.txt"), b"a person's own file\n").unwrap();
    let others = [
        "provision.log",
        "provision.lock",
        "localswim-pushed.json",
        "notes.txt",
    ]
    .map(|n| (n, fs::read(dir.join(n)).unwrap()));
    signed(&h, PROJECT, NOW);
    signed(&h, PROJECT, NOW + 1);
    for (name, bytes) in others {
        assert_eq!(fs::read(dir.join(name)).unwrap(), bytes, "{name}");
    }
    no_sign_files(&dir);
}

#[test]
fn a_ca_that_is_missing_fails_first_and_writes_nothing() {
    // The CA is checked before anything else, so not even the host's folder is made.
    let h = empty_home();
    let e = sign_err(&h, PROJECT, NOW);
    assert_eq!(e.message, ca_failure_message(CaFailure::Missing));
    assert!(e.leftovers.is_empty());
    assert!(
        !h.home.join(".agentspace").join("hosts").exists(),
        "The CA is checked before the host's folder is touched"
    );

    // With a key and an older certificate in place, both stay as they were.
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    signed(&h, PROJECT, 1_700_000_000);
    fs::remove_file(h.ca_pub()).unwrap();
    let before = snapshot(&dir);
    let e = sign_err(&h, PROJECT, NOW);
    assert_eq!(e.message, ca_failure_message(CaFailure::Missing));
    assert_eq!(
        snapshot(&dir),
        before,
        "The old certificate stays, byte for byte"
    );
    no_sign_files(&dir);
}

#[test]
fn a_ca_that_is_not_usable_fails_and_writes_no_certificate() {
    // An owner-only CA key that another principal can read is not usable.
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    icacls(&h.ca_key(), &["/grant", &format!("*{SID_EVERYONE}:R")]);
    let loosened = ca_snapshot(&h);
    let e = sign_err(&h, PROJECT, NOW);
    assert_eq!(e.message, ca_failure_message(CaFailure::NotUsable));
    assert!(e.leftovers.is_empty());
    assert!(!dir.join(CERT).exists(), "No certificate was written");
    assert_eq!(names(&dir), vec![KEY, PUB]);
    no_sign_files(&dir);
    assert_eq!(
        loosened,
        ca_snapshot(&h),
        "A failed check MUST NOT repair the CA"
    );
}

#[test]
fn a_ca_that_is_not_usable_keeps_the_old_certificate_byte_for_byte() {
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    signed(&h, PROJECT, 1_700_000_000);
    let old = fs::read(dir.join(CERT)).unwrap();
    // A public key that is no key at all: the CA fails its checks.
    fs::write(h.ca_pub(), b"not a public key\n").unwrap();
    let e = sign_err(&h, PROJECT, NOW);
    assert_eq!(e.message, ca_failure_message(CaFailure::NotUsable));
    assert_eq!(fs::read(dir.join(CERT)).unwrap(), old);
    assert_eq!(names(&dir), vec![KEY, CERT, PUB]);
    no_sign_files(&dir);
}

#[test]
fn the_ca_is_checked_before_the_key_and_before_the_folder() {
    // An unusable CA and no key: the answer is the CA's, and the folder is not made.
    let h = home_with_ca();
    icacls(&h.ca_key(), &["/grant", &format!("*{SID_EVERYONE}:R")]);
    let e = sign_err(&h, PROJECT, NOW);
    assert_eq!(e.message, ca_failure_message(CaFailure::NotUsable));
    assert!(!h.home.join(".agentspace").join("hosts").exists());
}

#[test]
fn a_signing_that_fails_leaves_the_old_certificate_and_no_sign_file() {
    // The copy of the public key is made and ssh-keygen then refuses it.
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    signed(&h, PROJECT, 1_700_000_000);
    let old = fs::read(dir.join(CERT)).unwrap();
    fs::write(dir.join(PUB), b"garbage, not a public key\n").unwrap();
    let before = snapshot(&dir);
    let e = sign_err(&h, PROJECT, NOW);
    assert_plain(&e.message, &h);
    assert!(
        e.leftovers.is_empty(),
        "Nothing was left: {:?}",
        e.leftovers
    );
    assert_eq!(fs::read(dir.join(CERT)).unwrap(), old);
    assert_eq!(snapshot(&dir), before, "The folder is as it was");
    no_sign_files(&dir);
}

#[test]
fn a_signing_without_a_key_fails_and_leaves_no_sign_file() {
    let h = home_with_ca();
    fs::create_dir_all(h.host_dir(PROJECT)).unwrap();
    let e = sign_err(&h, PROJECT, NOW);
    assert_plain(&e.message, &h);
    assert!(names(&h.host_dir(PROJECT)).is_empty());

    // A private key with no public key beside it.
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    fs::remove_file(dir.join(PUB)).unwrap();
    sign_err(&h, PROJECT, NOW);
    assert_eq!(
        names(&dir),
        vec![KEY],
        "No certificate and no sign-<hex> file"
    );
}

#[test]
fn a_failed_move_of_the_certificate_still_deletes_every_sign_file() {
    // A folder sits where the certificate goes, so signing works and the one rename fails.
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    fs::create_dir(dir.join(CERT)).unwrap();
    fs::write(dir.join(CERT).join("keep.txt"), b"x").unwrap();
    let before = snapshot(&dir);
    let e = sign_err(&h, PROJECT, NOW);
    assert_plain(&e.message, &h);
    assert!(
        e.leftovers.is_empty(),
        "Nothing was left: {:?}",
        e.leftovers
    );
    no_sign_files(&dir);
    assert_eq!(snapshot(&dir), before, "The folder is as it was");
    assert_eq!(fs::read(dir.join(CERT).join("keep.txt")).unwrap(), b"x");
}

#[test]
fn an_open_reader_does_not_make_the_certificates_replacement_fail() {
    // ssh reads the certificate while another command signs: the one rename replaces a file that a
    // reader holds open sharing read, write and delete (state.md, Replacing a file).
    use std::os::windows::fs::OpenOptionsExt;
    let h = keyed_home();
    let dir = h.host_dir(PROJECT);
    signed(&h, PROJECT, 1_700_000_000);
    let reader = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2 | 0x4)
        .open(dir.join(CERT))
        .unwrap();
    assert_eq!(signed(&h, PROJECT, NOW), NOW + 3600);
    drop(reader);
    assert_eq!(cert_info_in(&h, &dir.join(CERT)).valid_to, NOW + 3600);
    no_sign_files(&dir);
}

#[test]
fn a_failure_message_names_no_path_outside_the_home_and_no_user_name() {
    let h = home_with_ca();
    fs::remove_file(h.ca_key()).unwrap();
    let e = sign_err(&h, PROJECT, NOW);
    assert_plain(&e.message, &h);
    assert!(
        !e.message
            .to_lowercase()
            .contains(&username().to_lowercase())
    );
}

#[test]
fn ensure_then_sign_gives_a_certificate_for_that_key_in_one_call_each() {
    // The two together are what T03 calls: the key under hosts.lock, then a fresh certificate.
    let h = home_with_ca();
    assert!(ensured(&h, PROJECT));
    assert_eq!(signed(&h, PROJECT, NOW), NOW + 3600);
    assert!(!ensured(&h, PROJECT));
    assert_eq!(signed(&h, PROJECT, NOW + 10), NOW + 3610);
    let dir = h.host_dir(PROJECT);
    assert_eq!(names(&dir), vec![KEY, CERT, PUB]);
    let c = cert_info_in(&h, &dir.join(CERT));
    assert_eq!(c.public_fp, fingerprint_in(&h, &dir.join(PUB)));
    assert_eq!(c.valid_to, NOW + 3610);
}
