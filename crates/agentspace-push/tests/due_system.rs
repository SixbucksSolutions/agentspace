//! When the push is due, through the system's own parts (operator.md, The token push, When it
//! runs and The stamp; #0167, P14, architect, 2026-11-21, for v0.08-T01; built by E0.08-T02-S04,
//! #0176): the push over `SystemFiles` and `SystemSsh`, a stand-in ssh, a dummy credential and a
//! stamp file on disk. `tests/due.rs` checks the same rule over the push's seams.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Windows only.
//!
//! **The push is due by the stamp's rule alone.** A start of the host reaches the push as nothing
//! at all: the push is given the instance ID, the alias and whether it is forced, and the stamp
//! covers the instance, the zone and the content, which a stop and a start change none of. So a
//! push after a start, with the stamp the last confirmed push wrote, is not due.
#![cfg(windows)]

mod impl_support;

use agentspace_push::{Outcome, push_to_host_using, push_using_with, stamp_path_for};
use impl_support::{ALIAS, CONTENT, INSTANCE, Lines, Sandbox};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const NEW_YORK: &str = "America/New_York";
const OTHER_INSTANCE: &str = "i-0fedcba9876543210";

fn stamp_for(instance: &str, zone: &str) -> String {
    let mut h = Sha256::new();
    h.update(format!("{instance}\n{zone}\n{CONTENT}").as_bytes());
    h.finalize().iter().map(|b| format!("{b:02X}")).collect()
}

fn stamp_bytes(instance: &str, zone: &str) -> Vec<u8> {
    format!(
        "{{\"version\":0,\"stamp\":\"{}\"}}",
        stamp_for(instance, zone)
    )
    .into_bytes()
}

/// A rig with a receipt, a credential and a stand-in ssh that confirms the token and the zone.
fn rig() -> Sandbox {
    let sb = Sandbox::new();
    sb.write_localswim();
    sb.mode(&format!("ok:{NEW_YORK}"));
    sb
}

fn put_stamp(sb: &Sandbox, bytes: &[u8]) {
    let path = sb.stamp_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn push(sb: &Sandbox, forced: bool) -> (Outcome, Lines) {
    let say = Lines::default();
    let outcome = push_using_with(
        &sb.context(),
        &say,
        INSTANCE,
        ALIAS,
        forced,
        sb.lookup(Some(NEW_YORK)),
        &|ssh| ssh.with_command_setup(sb.setup()),
    );
    (outcome, say)
}

fn assert_due(sb: &Sandbox, what: &str) {
    let (outcome, say) = push(sb, false);
    assert_eq!(outcome, Outcome::Pushed, "{what}: a push is due");
    assert_eq!(sb.calls(), 1, "{what}: one ssh");
    assert_eq!(
        std::fs::read(sb.stamp_path()).unwrap(),
        stamp_bytes(INSTANCE, NEW_YORK),
        "{what}: the stamp comes back"
    );
    assert!(
        say.printed().iter().any(|l| l.starts_with("Sending")),
        "{what}: the progress line"
    );
}

#[test]
fn due_for_each_case_of_the_stamps_rule() {
    let good = String::from_utf8(stamp_bytes(INSTANCE, NEW_YORK)).unwrap();
    let cases: Vec<(&str, Option<Vec<u8>>)> = vec![
        ("no stamp", None),
        ("an empty stamp file", Some(Vec::new())),
        ("text that is not JSON", Some(b"not json".to_vec())),
        (
            "a truncated stamp file",
            Some(good.as_bytes()[..good.len() - 4].to_vec()),
        ),
        (
            "a stamp that is not in uppercase hex",
            Some(good.to_lowercase().into_bytes()),
        ),
        (
            "a stamp file with another version",
            Some(good.replace("\"version\":0", "\"version\":1").into_bytes()),
        ),
        (
            "a stamp over another instance",
            Some(stamp_bytes(OTHER_INSTANCE, NEW_YORK)),
        ),
        (
            "a stamp over another zone",
            Some(stamp_bytes(INSTANCE, "Asia/Kolkata")),
        ),
        (
            "a stamp over an empty zone",
            Some(stamp_bytes(INSTANCE, "")),
        ),
    ];
    for (what, stored) in cases {
        let sb = rig();
        if let Some(bytes) = stored {
            put_stamp(&sb, &bytes);
        }
        assert_due(&sb, what);
    }
}

#[test]
fn due_when_the_stamp_cannot_be_read() {
    // A folder at the stamp's path cannot be read as a file.
    let sb = rig();
    std::fs::create_dir_all(sb.stamp_path()).unwrap();
    let (outcome, _) = push(&sb, false);
    assert_eq!(sb.calls(), 1, "A push is due; the outcome was {outcome:?}");
}

#[test]
fn not_due_when_the_stamp_is_the_computed_one() {
    let sb = rig();
    let stored = stamp_bytes(INSTANCE, NEW_YORK);
    put_stamp(&sb, &stored);
    let (outcome, say) = push(&sb, false);
    assert_eq!(outcome, Outcome::NotDue);
    assert_eq!(sb.calls(), 0, "Nothing is sent");
    assert!(say.printed().is_empty(), "Nothing is printed");
    assert_eq!(std::fs::read(sb.stamp_path()).unwrap(), stored);
}

#[test]
fn a_forced_push_is_due_whatever_the_stamp_says() {
    let sb = rig();
    put_stamp(&sb, &stamp_bytes(INSTANCE, NEW_YORK));
    let (outcome, _) = push(&sb, true);
    assert_eq!(outcome, Outcome::Pushed);
    assert_eq!(sb.calls(), 1);
}

#[test]
fn a_push_after_a_start_of_the_host_is_not_due_while_the_stamp_is_current() {
    // The first push is due and confirmed. The host is then stopped and started, which the push
    // cannot see: its inputs are the instance ID, the alias, the forced flag, the zone and the two
    // files, and none of them is a start. The next push, in a new run, finds the stamp current.
    let sb = rig();
    assert_due(&sb, "the first push");
    for round in 1..=3 {
        let (outcome, say) = push(&sb, false);
        assert_eq!(outcome, Outcome::NotDue, "After start {round}");
        assert!(say.printed().is_empty(), "After start {round}");
    }
    assert_eq!(sb.calls(), 1, "Only the first push sent anything");
    assert_eq!(
        std::fs::read(sb.stamp_path()).unwrap(),
        stamp_bytes(INSTANCE, NEW_YORK)
    );
}

#[test]
fn a_started_host_is_pushed_to_when_the_instance_the_zone_or_the_token_changed() {
    // What does make the push due after a start is the stamp's own input changing: here the
    // instance ID (a host replaced), then the zone.
    let sb = rig();
    assert_due(&sb, "the first push");

    let say = Lines::default();
    let outcome = push_using_with(
        &sb.context(),
        &say,
        OTHER_INSTANCE,
        ALIAS,
        false,
        sb.lookup(Some(NEW_YORK)),
        &|ssh| ssh.with_command_setup(sb.setup()),
    );
    assert_eq!(outcome, Outcome::Pushed, "Another instance is due");
    assert_eq!(sb.calls(), 2);

    sb.mode("ok:Asia/Kolkata");
    let outcome = push_using_with(
        &sb.context(),
        &Lines::default(),
        OTHER_INSTANCE,
        ALIAS,
        false,
        sb.lookup(Some("Asia/Kolkata")),
        &|ssh| ssh.with_command_setup(sb.setup()),
    );
    assert_eq!(outcome, Outcome::Pushed, "Another zone is due");
    assert_eq!(sb.calls(), 3);
}

#[test]
fn a_hosts_push_reads_its_own_stamp_and_a_start_of_it_adds_nothing() {
    let sb = rig();
    let ctx = sb.context();
    let host_stamp: PathBuf = stamp_path_for(&ctx, Some("orchard"));
    std::fs::create_dir_all(host_stamp.parent().unwrap()).unwrap();
    std::fs::write(&host_stamp, stamp_bytes(INSTANCE, NEW_YORK)).unwrap();
    // The box's stamp is absent, so it would be due if it were consulted.
    let say = Lines::default();
    let outcome = push_to_host_using(
        &ctx,
        &say,
        INSTANCE,
        "agentspace-host.orchard",
        false,
        Some("orchard"),
        sb.lookup(Some(NEW_YORK)),
        &|ssh| ssh.with_command_setup(sb.setup()),
    );
    assert_eq!(outcome, Some(Outcome::NotDue));
    assert_eq!(sb.calls(), 0);
    assert!(say.printed().is_empty());
}
