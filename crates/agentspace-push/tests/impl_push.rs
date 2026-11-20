//! The Windows half of the push end to end (operator.md, The token push): DPAPI, the laptop's
//! zone and the override, the stand-in ssh, the stamp, and every outcome. The implementer's own
//! tests, in throwaway folders, with a dummy credential written for the current user with
//! `CryptProtectData`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Windows only.
#![cfg(windows)]

mod impl_support;

use agentspace_push::{
    COULD_NOT_READ, COULD_NOT_SEND, DpapiUnprotect, NO_ZONE, NOT_CONFIRMED, Outcome, SENDING,
    Seams, SystemFiles, SystemSsh, TIMED_OUT, Unprotect, push_using, push_using_with, push_with,
    remote_command, ssh_args, zone_not_taken,
};
use impl_support::{
    ALIAS, CONTENT, INSTANCE, Lines, Sandbox, TOKEN, end_own_process, ended_within, protect,
    utf16le,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;

const NEW_YORK: &str = "America/New_York";

fn stamp_for(zone: &str) -> String {
    let mut h = Sha256::new();
    h.update(format!("{INSTANCE}\n{zone}\n{CONTENT}").as_bytes());
    h.finalize().iter().map(|b| format!("{b:02X}")).collect()
}

fn stamp_bytes(zone: &str) -> Vec<u8> {
    format!("{{\"version\":0,\"stamp\":\"{}\"}}", stamp_for(zone)).into_bytes()
}

/// A rig with a receipt, a credential and a stand-in ssh.
fn rig() -> Sandbox {
    let sb = Sandbox::new();
    sb.write_localswim();
    sb
}

/// The push, over the sandbox's seam variables, with its ssh in a throwaway folder and home.
fn push(sb: &Sandbox, zone: Option<&str>, forced: bool) -> (Outcome, Lines) {
    let say = Lines::default();
    let outcome = push_using_with(
        &sb.context(),
        &say,
        INSTANCE,
        ALIAS,
        forced,
        sb.lookup(zone),
        // The stand-in runs in a throwaway working folder, with a throwaway home.
        &|ssh| ssh.with_command_setup(sb.setup()),
    );
    (outcome, say)
}

// ---- DPAPI ----

#[test]
fn dpapi_decrypts_what_localswim_writes() {
    let plain = utf16le(TOKEN);
    let blob = protect(&plain, None);
    assert_eq!(blob[..4], [1, 0, 0, 0], "A DPAPI blob begins 01 00 00 00");
    let got = DpapiUnprotect.unprotect(&blob).expect("The blob decrypts");
    assert_eq!(got.as_slice(), plain.as_slice());
}

#[test]
fn dpapi_refuses_a_blob_it_cannot_decrypt() {
    let blob = protect(&utf16le(TOKEN), None);
    // Corrupted, truncated, another provider's head, empty, not a blob at all, and one made with
    // entropy, which the call without entropy cannot open.
    let mut flipped = blob.clone();
    flipped[0] ^= 0xFF;
    let mut damaged = blob.clone();
    let last = damaged.len() - 1;
    damaged[last] ^= 0xFF;
    let with_entropy = protect(&utf16le(TOKEN), Some(b"some entropy"));
    let cases: Vec<Vec<u8>> = vec![
        flipped,
        damaged,
        blob[..blob.len() / 2].to_vec(),
        Vec::new(),
        b"not a blob".to_vec(),
        with_entropy,
    ];
    for (i, case) in cases.iter().enumerate() {
        assert!(
            DpapiUnprotect.unprotect(case).is_none(),
            "Case {i} was decrypted"
        );
    }
}

// ---- the push end to end ----

#[test]
fn a_first_push_sends_the_content_and_writes_the_stamp() {
    let sb = rig();
    sb.mode(&format!("ok:{NEW_YORK}"));
    let (outcome, say) = push(&sb, Some(NEW_YORK), false);
    assert_eq!(outcome, Outcome::Pushed);
    assert_eq!(say.printed(), [SENDING]);

    assert_eq!(sb.calls(), 1);
    assert_eq!(sb.args(1), ssh_args(ALIAS, &remote_command(NEW_YORK)));
    assert_eq!(sb.stdin(1), CONTENT.as_bytes());
    assert_eq!(
        std::fs::read(sb.stamp_path()).unwrap(),
        stamp_bytes(NEW_YORK)
    );

    // The stamp's folder holds the stamp alone: no temporary file is left.
    let names: Vec<_> = std::fs::read_dir(sb.home.join(".agentspace"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["localswim-pushed.json"]);

    // Secrecy: the token is on the stdin and nowhere else a stand-in, a message or the disk can
    // see it.
    assert!(!sb.args(1).iter().any(|a| a.contains(TOKEN)));
    assert!(!sb.env_text(1).contains(TOKEN));
    assert!(!say.printed().iter().any(|l| l.contains(TOKEN)));
    assert!(!String::from_utf8_lossy(&std::fs::read(sb.stamp_path()).unwrap()).contains(TOKEN));
    let nested = sb
        .env(1)
        .into_iter()
        .find(|(n, _)| n == "AGENTSPACE_NESTED");
    // A captured environment's values are never printed: a mismatch is reported by name.
    assert!(
        nested.is_some_and(|(_, v)| v == "1"),
        "AGENTSPACE_NESTED is not 1"
    );
}

#[test]
fn an_unchanged_push_is_not_due_and_a_forced_one_is() {
    let sb = rig();
    sb.mode(&format!("ok:{NEW_YORK}"));
    assert_eq!(push(&sb, Some(NEW_YORK), false).0, Outcome::Pushed);
    let (outcome, say) = push(&sb, Some(NEW_YORK), false);
    assert_eq!(outcome, Outcome::NotDue);
    assert!(say.printed().is_empty());
    assert_eq!(sb.calls(), 1);
    // `sync` forces it.
    assert_eq!(push(&sb, Some(NEW_YORK), true).0, Outcome::Pushed);
    assert_eq!(sb.calls(), 2);
    // A changed zone makes it due again, and the stamp then covers the new zone.
    sb.mode("ok:Europe/Paris");
    assert_eq!(push(&sb, Some("Europe/Paris"), false).0, Outcome::Pushed);
    assert_eq!(
        std::fs::read(sb.stamp_path()).unwrap(),
        stamp_bytes("Europe/Paris")
    );
}

#[test]
fn the_override_replaces_the_laptops_zone_and_a_legacy_name_is_mapped() {
    let sb = rig();
    sb.mode("ok:Asia/Kolkata");
    let (outcome, _) = push(&sb, Some("Asia/Calcutta"), false);
    assert_eq!(outcome, Outcome::Pushed);
    let command = sb.args(1).pop().unwrap();
    assert_eq!(command, remote_command("Asia/Kolkata"));
    assert!(!command.contains("Calcutta"));
    assert_eq!(
        std::fs::read(sb.stamp_path()).unwrap(),
        stamp_bytes("Asia/Kolkata")
    );
}

#[test]
fn a_zone_that_fails_the_check_is_the_empty_zone_and_is_never_sent() {
    for bad in ["not a zone", "-UTC", "a;b", "Europe//Paris"] {
        let sb = rig();
        sb.mode("ok");
        let (outcome, say) = push(&sb, Some(bad), false);
        assert_eq!(outcome, Outcome::Pushed, "{bad:?}");
        assert_eq!(say.printed(), [SENDING, NO_ZONE], "{bad:?}");
        let command = sb.args(1).pop().unwrap();
        assert_eq!(command, remote_command(""), "{bad:?}");
        assert!(!command.contains("timedatectl"));
        assert_eq!(
            std::fs::read(sb.stamp_path()).unwrap(),
            stamp_bytes(""),
            "{bad:?}"
        );
    }
}

#[test]
fn with_no_override_the_zone_is_the_laptops_own_from_the_system() {
    let sb = rig();
    let own = agentspace_zone::laptop_zone(None);
    sb.mode(&format!("ok:{}", own.as_deref().unwrap_or("")));
    let (outcome, say) = push(&sb, None, false);
    assert_eq!(outcome, Outcome::Pushed);
    let zone = own.unwrap_or_default();
    assert_eq!(sb.args(1).pop().unwrap(), remote_command(&zone));
    assert_eq!(
        say.printed().contains(&NO_ZONE.to_string()),
        zone.is_empty()
    );
    assert_eq!(std::fs::read(sb.stamp_path()).unwrap(), stamp_bytes(&zone));
}

#[test]
fn a_box_that_does_not_confirm_leaves_the_stamp_as_it_was() {
    let sb = rig();
    let old = b"{\"version\":0,\"stamp\":\"OLD\"}".to_vec();
    std::fs::create_dir_all(sb.home.join(".agentspace")).unwrap();
    std::fs::write(sb.stamp_path(), &old).unwrap();
    for mode in ["exit 255", "exit 1", "noread-exit"] {
        sb.mode(mode);
        let (outcome, say) = push(&sb, Some(NEW_YORK), false);
        assert_eq!(outcome, Outcome::NotConfirmed, "{mode}");
        assert_eq!(say.printed(), [SENDING, NOT_CONFIRMED], "{mode}");
        assert_eq!(std::fs::read(sb.stamp_path()).unwrap(), old, "{mode}");
    }
    // A box that took the token and not the zone.
    sb.mode("zonefailed");
    let (outcome, say) = push(&sb, Some(NEW_YORK), false);
    assert_eq!(outcome, Outcome::ZoneNotTaken);
    assert_eq!(
        say.printed(),
        [SENDING.to_string(), zone_not_taken(NEW_YORK)]
    );
    assert_eq!(std::fs::read(sb.stamp_path()).unwrap(), old);
    // A box that confirms the token and sends no zone line.
    sb.mode("ok");
    assert_eq!(push(&sb, Some(NEW_YORK), false).0, Outcome::ZoneNotTaken);
    assert_eq!(std::fs::read(sb.stamp_path()).unwrap(), old);
}

#[test]
fn a_banner_ahead_of_the_hash_line_is_skipped() {
    let sb = rig();
    sb.mode("noise");
    let (outcome, _) = push(&sb, Some("not a zone"), false);
    assert_eq!(outcome, Outcome::Pushed);
}

#[test]
fn an_ssh_that_cannot_be_started_warns_could_not_send_and_never_falls_back() {
    let sb = rig();
    let say = Lines::default();
    let missing = sb.root.join("no such ssh.cmd").into_os_string();
    let lookup: Arc<agentspace_push::Lookup> =
        Arc::new(move |name: &str| (name == "AGENTSPACE_SSH_CMD").then(|| missing.clone()));
    let outcome = push_using(&sb.context(), &say, INSTANCE, ALIAS, false, lookup);
    assert_eq!(outcome, Outcome::CouldNotSend);
    assert_eq!(
        say.printed().last().map(String::as_str),
        Some(COULD_NOT_SEND)
    );
    assert!(!sb.stamp_path().exists());
    assert_eq!(sb.calls(), 0);
}

#[test]
fn a_stamp_that_cannot_be_written_warns_could_not_send() {
    let sb = rig();
    sb.mode(&format!("ok:{NEW_YORK}"));
    // A file where the stamp's folder would be.
    std::fs::write(sb.home.join(".agentspace"), b"in the way").unwrap();
    let (outcome, say) = push(&sb, Some(NEW_YORK), false);
    assert_eq!(outcome, Outcome::CouldNotSend);
    assert_eq!(say.printed(), [SENDING, COULD_NOT_SEND]);
}

#[test]
fn nothing_is_read_or_started_for_a_nested_up_and_sync_pushes_whatever_nested_says() {
    let sb = rig();
    sb.mode(&format!("ok:{NEW_YORK}"));
    let mut ctx = sb.context();
    ctx.nested = true;
    let say = Lines::default();
    let outcome = push_using(
        &ctx,
        &say,
        INSTANCE,
        ALIAS,
        false,
        sb.lookup(Some(NEW_YORK)),
    );
    assert_eq!(outcome, Outcome::Nested);
    assert!(say.printed().is_empty());
    assert_eq!(sb.calls(), 0);
    let outcome = push_using_with(
        &ctx,
        &say,
        INSTANCE,
        ALIAS,
        true,
        sb.lookup(Some(NEW_YORK)),
        &|ssh| ssh.with_command_setup(sb.setup()),
    );
    assert_eq!(outcome, Outcome::Pushed);
}

#[test]
fn a_missing_receipt_is_silent_and_an_unreadable_credential_warns_and_starts_no_ssh() {
    let sb = Sandbox::new();
    let (outcome, say) = push(&sb, Some(NEW_YORK), false);
    assert_eq!(outcome, Outcome::NoReceipt);
    assert!(say.printed().is_empty());

    sb.write_localswim();
    std::fs::write(sb.credential_dir().join("bot.xml"), "not a credential").unwrap();
    let (outcome, say) = push(&sb, Some(NEW_YORK), false);
    assert_eq!(outcome, Outcome::CouldNotRead);
    assert_eq!(say.printed(), [COULD_NOT_READ]);
    assert_eq!(sb.calls(), 0);

    // A credential protected for no one who can read it here: a blob that does not decrypt.
    let hex = impl_support::hex_lower(&[1, 0, 0, 0, 0xD0, 0x8C, 0x9D, 0xDF, 1, 2, 3]);
    std::fs::write(
        sb.credential_dir().join("bot.xml"),
        impl_support::clixml(&hex),
    )
    .unwrap();
    let (outcome, say) = push(&sb, Some(NEW_YORK), false);
    assert_eq!(outcome, Outcome::CouldNotRead);
    assert_eq!(say.printed(), [COULD_NOT_READ]);
    assert_eq!(sb.calls(), 0);
}

#[test]
fn a_hung_ssh_is_stopped_with_its_tree_and_the_push_warns_and_records_nothing() {
    let sb = rig();
    sb.mode("hangtree");
    let say = Lines::default();
    // The push's own pieces, with the ssh's limit shortened: the real 20 seconds are measured in
    // impl_ssh.rs.
    let ssh = SystemSsh::with_lookup(&sb.context(), sb.lookup(Some(NEW_YORK)))
        .with_command_setup(sb.setup())
        .with_limits(Duration::from_secs(10), Duration::from_secs(10));
    let zone = || Some(NEW_YORK.to_string());
    let seams = Seams {
        files: &SystemFiles,
        unprotect: &DpapiUnprotect,
        ssh: &ssh,
        zone: &zone,
    };
    let outcome = push_with(&sb.context(), &say, INSTANCE, ALIAS, false, &seams);
    let pids: Vec<impl_support::Proc> = ["ps", "grand", "parent"]
        .into_iter()
        .filter_map(|kind| sb.pid(kind, 1))
        .collect();
    let gone: Vec<bool> = pids
        .iter()
        .map(|p| ended_within(*p, 3 * Duration::from_secs(1)))
        .collect();
    for pid in &pids {
        end_own_process(*pid);
    }
    assert_eq!(outcome, Outcome::TimedOut);
    assert_eq!(say.printed(), [SENDING, TIMED_OUT]);
    assert!(!sb.stamp_path().exists(), "A stamp was written");
    assert_eq!(
        pids.len(),
        3,
        "The stand-in did not record its processes in time"
    );
    assert!(
        gone.iter().all(|g| *g),
        "A process of the ssh's tree is still running"
    );
}
