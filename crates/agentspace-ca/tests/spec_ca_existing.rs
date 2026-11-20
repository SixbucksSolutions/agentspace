//! An existing CA is checked and left untouched (docs/operator.md, ca, step 5 and the closing
//! rules; docs/state.md, The CA, Its checks and Its access rules).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. A loosened folder or key is made by the
//! test itself through the Windows API, and its rules are put back by a Drop guard. `ca` and
//! `sync`'s check make the same five checks, so each case runs against both.
//!
//! Not covered, and why: `<problems>`' `is owned by <owner>` needs a folder owned by another
//! account, which an unelevated process cannot make; and check 5.5 (the public key's fingerprint
//! cannot be read) cannot be reached once check 5.4 has found the first two words equal to the
//! private key's own.
#![cfg(windows)]

mod spec_common;

use std::fs;

use spec_common::*;

/// A lab with a CA made by `ca`.
fn lab_with_ca() -> Option<Lab> {
    if skip_when_elevated() {
        return None;
    }
    let lab = Lab::new();
    make_ca(&lab);
    Some(lab)
}

#[track_caller]
fn the_ca_is_unchanged(lab: &Lab, before: &Snapshot, what: &str) {
    assert_unchanged(lab, before, what);
}

// ---- A sound CA changes nothing ----

#[test]
fn a_sound_ca_is_left_exactly_as_it_was() {
    let Some(lab) = lab_with_ca() else { return };
    let before = snapshot(&lab);
    for entry in BOTH {
        let o = run(entry, &lab.ctx());
        o.ok();
        the_ca_is_unchanged(&lab, &before, &format!("{entry:?}"));
        assert!(o.only_progress);
    }
}

#[test]
fn the_public_keys_comment_and_spacing_do_not_matter() {
    // Only the first two words, split at whitespace after trimming, are compared.
    let Some(lab) = lab_with_ca() else { return };
    let words = fs::read_to_string(lab.pubkey()).unwrap();
    let words: Vec<&str> = words.split_whitespace().collect();
    let fp = fingerprint_of(&lab.pubkey());
    fs::write(
        lab.pubkey(),
        format!(
            "  {}\t{}    any comment at all \r\n\r\n",
            words[0], words[1]
        ),
    )
    .unwrap();
    for entry in BOTH {
        let o = run(entry, &lab.ctx());
        o.ok();
        if entry == Entry::Sync {
            assert_eq!(o.fingerprint.as_deref(), Some(fp.as_str()));
        }
    }
}

// ---- Check 5.2: owner-only, the folder first, then the key ----

/// The access rules a test loosens to, each with the `<problems>` the spec gives, in order, for a
/// folder (`dir`) or a file.
struct Loosening {
    name: &'static str,
    acl: String,
    /// The `<problems>` text, with `{RIGHTS}` for the one place the rights' words go.
    problems: String,
}

fn account_or_sid(sid: &str) -> String {
    account_name(sid).unwrap_or_else(|| sid.to_string())
}

fn account_name(sid: &str) -> Option<String> {
    spec_common::win::account_name(sid)
}

fn loosenings(dir: bool) -> Vec<Loosening> {
    let me = user_sid();
    let user = user_name();
    let users = account_or_sid(SID_USERS);
    let guests = account_or_sid(SID_GUESTS);
    let mut v = vec![
        Loosening {
            name: "another account may read",
            acl: sddl(dir, true, &[('A', "FA", &me), ('A', "FR", SID_USERS)]),
            problems: format!("has a rule allowing {users}; has 2 access rules instead of one"),
        },
        Loosening {
            name: "another account is denied",
            // The deny is for an account the user is not in, or it would lock the test out.
            acl: sddl(dir, true, &[('D', "FR", SID_GUESTS), ('A', "FA", &me)]),
            problems: format!("has a rule denying {guests}; has 2 access rules instead of one"),
        },
        Loosening {
            name: "the user's own rule is short of full control",
            acl: sddl(dir, true, &[('A', "FR", &me)]),
            problems: format!("gives {user} {{RIGHTS}}, not full control"),
        },
    ];
    // An account Windows cannot name is shown as its SID.
    if account_name(SID_UNKNOWN).is_none() {
        v.push(Loosening {
            name: "an account that cannot be translated",
            acl: sddl(dir, true, &[('A', "FA", &me), ('A', "FR", SID_UNKNOWN)]),
            problems: format!(
                "has a rule allowing {SID_UNKNOWN}; has 2 access rules instead of one"
            ),
        });
    }
    v
}

#[test]
fn a_loosened_folder_is_refused_with_every_problem_in_its_order_and_left_loosened() {
    let Some(lab) = lab_with_ca() else { return };
    let user = user_name();
    for l in loosenings(true) {
        let guard = AclGuard::new(&lab.ca_dir());
        set_acl(&lab.ca_dir(), &l.acl);
        let loosened = read_acl(&lab.ca_dir());
        let before = snapshot(&lab);
        for entry in BOTH {
            let o = run(entry, &lab.ctx());
            let want = m_loose(&lab.ca_dir(), &l.problems, &lab.ca_dir(), &lab.key(), &user);
            assert!(
                matches_with_rights(&o.message(), &want),
                "{entry:?}, {}: the message is not the spec's",
                l.name
            );
            assert!(
                o.lines.is_empty(),
                "{entry:?} said something before failing"
            );
            assert!(
                read_acl(&lab.ca_dir()) == loosened,
                "{entry:?} changed the folder's rules"
            );
        }
        the_ca_is_unchanged(&lab, &before, l.name);
        drop(guard);
        assert_owner_only(&lab.ca_dir(), true);
    }
}

#[test]
fn a_folder_that_inherits_is_refused_with_the_inheritance_problem_first() {
    let Some(lab) = lab_with_ca() else { return };
    let user = user_name();
    let guard = AclGuard::new(&lab.ca_dir());
    // Unprotected, so the parent's rules flow in beside the user's own.
    set_acl(
        &lab.ca_dir(),
        &sddl(true, false, &[('A', "FA", &user_sid())]),
    );
    assert!(!read_acl(&lab.ca_dir()).protected);
    for entry in BOTH {
        let msg = run(entry, &lab.ctx()).message();
        let head = format!(
            "{}inherits access rules from its parent",
            m_loose_prefix(&lab.ca_dir(), &user)
        );
        assert!(
            msg.starts_with(&head),
            "{entry:?}: The message does not start as the spec's"
        );
        assert!(
            msg.ends_with(&m_loose_suffix(&lab.ca_dir(), &lab.key(), &user)),
            "{entry:?}: The message does not end as the spec's"
        );
    }
    drop(guard);
    assert_owner_only(&lab.ca_dir(), true);
}

#[test]
fn a_loosened_private_key_is_refused_naming_the_key() {
    let Some(lab) = lab_with_ca() else { return };
    let user = user_name();
    for l in loosenings(false) {
        let guard = AclGuard::new(&lab.key());
        set_acl(&lab.key(), &l.acl);
        let loosened = read_acl(&lab.key());
        let before = snapshot(&lab);
        for entry in BOTH {
            let o = run(entry, &lab.ctx());
            let want = m_loose(&lab.key(), &l.problems, &lab.ca_dir(), &lab.key(), &user);
            assert!(
                matches_with_rights(&o.message(), &want),
                "{entry:?}, {}: the message is not the spec's",
                l.name
            );
            assert!(
                read_acl(&lab.key()) == loosened,
                "{entry:?} changed the key's rules"
            );
        }
        the_ca_is_unchanged(&lab, &before, l.name);
        drop(guard);
        assert_owner_only(&lab.key(), false);
    }
}

#[test]
fn a_private_key_that_inherits_is_refused_with_the_inheritance_problem_first() {
    let Some(lab) = lab_with_ca() else { return };
    let user = user_name();
    let guard = AclGuard::new(&lab.key());
    set_acl(&lab.key(), &sddl(false, false, &[('A', "FA", &user_sid())]));
    assert!(!read_acl(&lab.key()).protected);
    for entry in BOTH {
        let msg = run(entry, &lab.ctx()).message();
        let head = format!(
            "{}inherits access rules from its parent",
            m_loose_prefix(&lab.key(), &user)
        );
        assert!(
            msg.starts_with(&head),
            "{entry:?}: The message does not start as the spec's"
        );
        assert!(msg.ends_with(&m_loose_suffix(&lab.ca_dir(), &lab.key(), &user)));
    }
    drop(guard);
    assert_owner_only(&lab.key(), false);
}

#[test]
fn the_folder_is_checked_before_the_key() {
    let Some(lab) = lab_with_ca() else { return };
    let user = user_name();
    let me = user_sid();
    let _key_guard = AclGuard::new(&lab.key());
    let _dir_guard = AclGuard::new(&lab.ca_dir());
    set_acl(
        &lab.key(),
        &sddl(false, true, &[('A', "FA", &me), ('A', "FR", SID_USERS)]),
    );
    set_acl(
        &lab.ca_dir(),
        &sddl(true, true, &[('A', "FA", &me), ('A', "FR", SID_USERS)]),
    );
    let users = account_or_sid(SID_USERS);
    for entry in BOTH {
        let want = m_loose(
            &lab.ca_dir(),
            &format!("has a rule allowing {users}; has 2 access rules instead of one"),
            &lab.ca_dir(),
            &lab.key(),
            &user,
        );
        same_for(run(entry, &lab.ctx()).message(), want, format!("{entry:?}"));
    }
}

#[test]
fn the_access_rules_guard_puts_the_rules_back_as_they_were() {
    let Some(lab) = lab_with_ca() else { return };
    let original_dir = read_acl(&lab.ca_dir());
    let original_key = read_acl(&lab.key());
    {
        let _g1 = AclGuard::new(&lab.ca_dir());
        let _g2 = AclGuard::new(&lab.key());
        set_acl(&lab.ca_dir(), &sddl(true, false, &[('A', "FR", SID_USERS)]));
        set_acl(&lab.key(), &sddl(false, false, &[('A', "FR", SID_USERS)]));
        assert!(read_acl(&lab.ca_dir()) != original_dir);
        assert!(read_acl(&lab.key()) != original_key);
    }
    assert!(
        read_acl(&lab.ca_dir()) == original_dir,
        "The guard did not put the folder's rules back"
    );
    assert!(
        read_acl(&lab.key()) == original_key,
        "The guard did not put the key's rules back"
    );
    run(Entry::Ca, &lab.ctx()).ok();
}

// ---- Check 5.3: the private key loads with an empty passphrase ----

#[test]
fn a_private_key_with_a_passphrase_is_refused() {
    let Some(lab) = lab_with_ca() else { return };
    let aux = lab.root().join("aux");
    fs::create_dir_all(&aux).unwrap();
    let enc = aux.join("encrypted");
    assert!(
        ssh_keygen(
            &aux,
            &[
                "-q",
                "-t",
                "ed25519",
                "-N",
                "spec-passphrase",
                "-f",
                enc.to_str().unwrap()
            ]
        ),
        "ssh-keygen could not make the encrypted key the test needs"
    );
    // Written over the key in place, so the key keeps its owner-only rules.
    fs::write(lab.key(), fs::read(&enc).unwrap()).unwrap();
    let before = snapshot(&lab);
    for entry in BOTH {
        same_for(
            run(entry, &lab.ctx()).message(),
            m_no_passphrase(&lab.key()),
            format!("{entry:?}"),
        );
    }
    the_ca_is_unchanged(&lab, &before, "a key with a passphrase");
}

#[test]
fn a_private_key_that_is_not_a_key_is_refused() {
    let Some(lab) = lab_with_ca() else { return };
    fs::write(lab.key(), "this is not a private key\n").unwrap();
    let before = snapshot(&lab);
    for entry in BOTH {
        same_for(
            run(entry, &lab.ctx()).message(),
            m_no_passphrase(&lab.key()),
            format!("{entry:?}"),
        );
    }
    the_ca_is_unchanged(&lab, &before, "a key that is not a key");
}

// ---- Check 5.4: the public key is the private key's own, and ssh-ed25519 ----

#[test]
fn a_public_key_that_is_not_the_private_keys_own_is_refused_and_not_repaired() {
    let Some(lab) = lab_with_ca() else { return };
    fs::write(lab.pubkey(), format!("{}\n", other_public_key_line())).unwrap();
    let before = snapshot(&lab);
    for entry in BOTH {
        same_for(
            run(entry, &lab.ctx()).message(),
            m_mismatch(&lab.pubkey(), &lab.key()),
            format!("{entry:?}"),
        );
    }
    the_ca_is_unchanged(&lab, &before, "a mismatched pair");
}

#[test]
fn a_public_key_with_no_usable_words_is_a_failure_and_never_a_panic() {
    // The spec lets a build treat a public key of fewer than two words as any of the checks'
    // failures (state.md, Its checks), so only the failure's shape is pinned.
    let Some(lab) = lab_with_ca() else { return };
    for content in ["", "\n", "ssh-ed25519\n", "   \t  \n"] {
        fs::write(lab.pubkey(), content).unwrap();
        let before = snapshot(&lab);
        for entry in BOTH {
            let msg = run(entry, &lab.ctx()).message();
            assert!(
                msg.starts_with("agentspace: "),
                "{entry:?}: not an agentspace message"
            );
            assert!(!msg.contains('\n'), "{entry:?}: not one line");
        }
        the_ca_is_unchanged(&lab, &before, "an unusable public key");
    }
}

// ---- The checks' order ----

#[test]
fn the_access_rules_are_checked_before_the_passphrase_and_the_pair() {
    let Some(lab) = lab_with_ca() else { return };
    let user = user_name();
    let me = user_sid();
    // A bad key AND a loose folder: the folder (check 2) comes before the key's content (3).
    fs::write(lab.key(), "this is not a private key\n").unwrap();
    let _guard = AclGuard::new(&lab.ca_dir());
    set_acl(
        &lab.ca_dir(),
        &sddl(true, true, &[('A', "FA", &me), ('A', "FR", SID_USERS)]),
    );
    let users = account_or_sid(SID_USERS);
    for entry in BOTH {
        let want = m_loose(
            &lab.ca_dir(),
            &format!("has a rule allowing {users}; has 2 access rules instead of one"),
            &lab.ca_dir(),
            &lab.key(),
            &user,
        );
        same_for(run(entry, &lab.ctx()).message(), want, format!("{entry:?}"));
    }
}
