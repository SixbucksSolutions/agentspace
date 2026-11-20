//! `ca` makes the CA (docs/operator.md, ca, steps 4 and 6; docs/state.md, The CA).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The elevated refusal (step 4's first
//! bullet) needs an elevated shell, and these tests run unelevated, so it can only be shown NOT to
//! fire; when the process is elevated, the test shows the refusal instead and the others skip.
#![cfg(windows)]

mod spec_common;

use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use spec_common::*;

fn expected_four_lines(lab: &Lab, first: &str) -> Vec<String> {
    let fp = fingerprint_of(&lab.pubkey());
    vec![
        first.to_string(),
        format!("Private key  {}", lab.key().display()),
        format!("Public key   {}", lab.pubkey().display()),
        format!("Fingerprint  {fp}"),
    ]
}

#[test]
fn an_unelevated_run_is_not_refused_as_elevated() {
    let lab = Lab::new();
    let o = run(Entry::Ca, &lab.ctx());
    if elevated() {
        // The refusal, shown instead: the test process is itself elevated.
        same(o.message(), M_ELEVATED);
        assert!(
            !lab.key().exists() && !lab.pubkey().exists(),
            "A key was made"
        );
    } else {
        o.ok();
        assert!(lab.key().is_file() && lab.pubkey().is_file());
    }
}

#[test]
fn a_fresh_home_gets_a_ca_and_the_exact_progress_lines() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    let o = make_ca(&lab);
    let mut want = vec![m_creating(&lab.ca_dir())];
    want.extend(expected_four_lines(&lab, "Created the agentspace SSH CA."));
    same_lines(&o.lines, &want);
    assert!(o.only_progress, "ca said something that is not progress");
}

#[test]
fn the_key_is_ed25519_with_no_passphrase_and_the_public_key_is_its_own() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    make_ca(&lab);
    let text = fs::read_to_string(lab.key()).expect("The private key could not be read");
    assert!(text.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
    let private = ssh_key::PrivateKey::from_openssh(&text).expect("The private key does not parse");
    assert!(!private.is_encrypted(), "The private key has a passphrase");
    assert_eq!(private.algorithm(), ssh_key::Algorithm::Ed25519);

    let pub_text = fs::read_to_string(lab.pubkey()).unwrap();
    assert_eq!(
        pub_text.trim().lines().count(),
        1,
        "The public key is not one line"
    );
    let words: Vec<&str> = pub_text.split_whitespace().collect();
    assert_eq!(words[0], "ssh-ed25519");
    let derived = private.public_key().to_openssh().unwrap();
    let derived_words: Vec<&str> = derived.split_whitespace().collect();
    assert_eq!(
        words[1], derived_words[1],
        "The public key is not the private key's own"
    );
}

#[test]
fn the_printed_fingerprint_is_the_public_keys_own() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    let o = make_ca(&lab);
    let fp = fingerprint_of(&lab.pubkey());
    assert!(fp.starts_with("SHA256:"));
    same(o.lines.last().unwrap(), format!("Fingerprint  {fp}"));
}

#[test]
fn the_folder_and_the_private_key_are_owner_only_through_the_windows_api() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    make_ca(&lab);
    assert_owner_only(&lab.ca_dir(), true);
    assert_owner_only(&lab.key(), false);
}

#[test]
fn the_folder_holds_exactly_the_two_files() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    make_ca(&lab);
    let mut names: Vec<String> = fs::read_dir(lab.ca_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["agentspace_ca", "agentspace_ca.pub"]);
}

#[test]
fn a_folder_that_exists_with_no_key_is_locked_before_the_ca_is_made_in_it() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    // A folder made the ordinary way inherits its parent's rules and so is open to others.
    fs::create_dir_all(lab.ca_dir()).unwrap();
    assert!(
        read_acl(&lab.ca_dir()).aces.len() > 1 || !read_acl(&lab.ca_dir()).protected,
        "The throwaway folder was not open to start with"
    );
    let o = make_ca(&lab);
    same(&o.lines[0], m_creating(&lab.ca_dir()));
    assert_owner_only(&lab.ca_dir(), true);
    assert_owner_only(&lab.key(), false);
    assert!(lab.pubkey().is_file());
}

#[test]
fn a_folder_open_to_users_is_locked_to_the_user_alone_before_the_key_exists() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    fs::create_dir_all(lab.ca_dir()).unwrap();
    let me = user_sid();
    set_acl(
        &lab.ca_dir(),
        &sddl(true, true, &[('A', "FA", &me), ('A', "FR", SID_USERS)]),
    );
    assert_eq!(read_acl(&lab.ca_dir()).aces.len(), 2);
    make_ca(&lab);
    assert_owner_only(&lab.ca_dir(), true);
}

#[test]
fn the_folder_is_already_owner_only_when_the_first_key_file_appears() {
    // This can only show a violation, never prove its absence: a poller reads the folder's access
    // rules the moment a key file shows in it. A build that makes the key and then locks the
    // folder is caught when the poller is quick enough, and the check repeats to give it a chance.
    if skip_when_elevated() {
        return;
    }
    for _ in 0..6 {
        let lab = Lab::new();
        let seen: Arc<Mutex<Option<Acl>>> = Arc::new(Mutex::new(None));
        let done = Arc::new(AtomicBool::new(false));
        let (ca_dir, key, pubkey) = (lab.ca_dir(), lab.key(), lab.pubkey());
        let poller = {
            let (seen, done) = (seen.clone(), done.clone());
            std::thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                    if key.exists() || pubkey.exists() {
                        *seen.lock().unwrap() = Some(read_acl(&ca_dir));
                        return;
                    }
                    std::thread::yield_now();
                }
            })
        };
        let o = run(Entry::Ca, &lab.ctx());
        done.store(true, Ordering::Relaxed);
        poller.join().unwrap();
        o.ok();
        if let Some(a) = seen.lock().unwrap().as_ref() {
            let me = user_sid();
            assert!(
                a.owner == me,
                "A key appeared in a folder the user did not own"
            );
            assert!(
                a.protected,
                "A key appeared in a folder that still inherited rules"
            );
            assert_eq!(a.aces.len(), 1, "A key appeared in a folder open to others");
            assert!(
                a.aces[0].sid == me,
                "A key appeared in a folder whose one rule was not the user's"
            );
        }
    }
}

#[test]
fn ca_says_only_progress_and_makes_no_warning_or_detail() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    let o = make_ca(&lab);
    assert!(o.only_progress);
    assert_eq!(o.lines.len(), 5);
}

#[test]
fn a_second_run_says_the_ca_already_exists() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    make_ca(&lab);
    let o = run(Entry::Ca, &lab.ctx());
    o.ok();
    same_lines(
        &o.lines,
        &expected_four_lines(
            &lab,
            "The agentspace SSH CA already exists; nothing changed.",
        ),
    );
    assert!(o.only_progress);
}
