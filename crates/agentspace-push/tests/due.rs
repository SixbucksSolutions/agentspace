//! When the push runs and when it is due, from the stamp file's bytes (operator.md, When it runs,
//! The stamp; state.md, The push stamp).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;
use common::*;

/// A run that sends the standard content to the standard instance with the zone `ZONE`, so that
/// the stamp it computes is `STAMP_LA`.
fn run_with_stored(env: &Env, stored: Slot, command: Cmd) -> Run {
    let mut s = env.scenario();
    s.stored_stamp = stored;
    s.command = command;
    env.run(&s)
}

fn stored(text: &str) -> Slot {
    Slot::Bytes(text.as_bytes().to_vec())
}

fn assert_due(r: &Run, what: &str) {
    assert_eq!(r.ssh_calls.len(), 1, "{what}: a push is due: {r:?}");
    assert_eq!(
        r.stamp_after.as_deref(),
        Some(stamp_file_bytes(STAMP_LA).as_slice()),
        "{what}"
    );
}

fn assert_not_due(r: &Run, what: &str, expect_stamp: &[u8]) {
    assert!(!r.pushed(), "{what}: nothing is sent: {r:?}");
    assert_eq!(
        r.printed,
        Vec::<String>::new(),
        "{what}: nothing is printed"
    );
    assert_eq!(
        r.stamp_after.as_deref(),
        Some(expect_stamp),
        "{what}: the stamp is as it was"
    );
}

// ---- Due ---------------------------------------------------------------------------------------

#[test]
fn due_when_there_is_no_stamp() {
    let env = Env::new();
    assert_due(&run_with_stored(&env, Slot::Absent, Cmd::Up), "no stamp");
}

#[test]
fn due_when_the_stored_stamp_is_empty() {
    let env = Env::new();
    assert_due(
        &run_with_stored(&env, Slot::Bytes(vec![]), Cmd::Up),
        "empty",
    );
}

#[test]
fn due_when_the_stamp_cannot_be_read() {
    // A folder at the stamp's path cannot be read as a file.
    let env = Env::new();
    let mut s = env.scenario();
    s.stored_stamp = Slot::Folder;
    let r = env.run(&s);
    assert_eq!(r.ssh_calls.len(), 1, "{r:?}");
}

#[test]
fn due_when_the_stamp_cannot_be_parsed_or_does_not_match_the_schema() {
    let env = Env::new();
    let good = format!("{{\"version\":0,\"stamp\":\"{STAMP_LA}\"}}");
    let lower = STAMP_LA.to_lowercase();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("not json", b"not json".to_vec()),
        ("truncated", good.as_bytes()[..good.len() - 3].to_vec()),
        ("only whitespace", b" \n ".to_vec()),
        ("invalid utf8", vec![0xFF, 0xFE, 0xFD, 0x00]),
        // A stamp file that is not UTF-8, such as one in UTF-16, cannot be parsed, so a push is
        // due (operator.md, What the laptop checks; architect, 2026-11-19, for E0.06-T01-S04).
        ("utf16 json", enc_utf16le_bom(&good)),
        ("utf16be json", enc_utf16be_bom(&good)),
        ("the prototype's bare hex", STAMP_LA.as_bytes().to_vec()),
        ("the prototype's hex, lowercase", lower.as_bytes().to_vec()),
        ("an array", format!("[\"{STAMP_LA}\"]").into_bytes()),
        ("null", b"null".to_vec()),
        ("a string", format!("\"{STAMP_LA}\"").into_bytes()),
        (
            "no version",
            format!("{{\"stamp\":\"{STAMP_LA}\"}}").into_bytes(),
        ),
        ("no stamp", b"{\"version\":0}".to_vec()),
        (
            "version 1",
            format!("{{\"version\":1,\"stamp\":\"{STAMP_LA}\"}}").into_bytes(),
        ),
        (
            "version a string",
            format!("{{\"version\":\"0\",\"stamp\":\"{STAMP_LA}\"}}").into_bytes(),
        ),
        (
            "version a bool",
            format!("{{\"version\":false,\"stamp\":\"{STAMP_LA}\"}}").into_bytes(),
        ),
        (
            "lowercase stamp",
            format!("{{\"version\":0,\"stamp\":\"{lower}\"}}").into_bytes(),
        ),
        (
            "63 digits",
            format!("{{\"version\":0,\"stamp\":\"{}\"}}", &STAMP_LA[1..]).into_bytes(),
        ),
        (
            "65 digits",
            format!("{{\"version\":0,\"stamp\":\"{STAMP_LA}0\"}}").into_bytes(),
        ),
        (
            "a non-hex digit",
            format!("{{\"version\":0,\"stamp\":\"G{}\"}}", &STAMP_LA[1..]).into_bytes(),
        ),
        (
            "a line end after the digits",
            format!("{{\"version\":0,\"stamp\":\"{STAMP_LA}\\n\"}}").into_bytes(),
        ),
        (
            "a space after the digits",
            format!("{{\"version\":0,\"stamp\":\"{STAMP_LA} \"}}").into_bytes(),
        ),
        ("stamp a number", b"{\"version\":0,\"stamp\":5}".to_vec()),
        ("stamp null", b"{\"version\":0,\"stamp\":null}".to_vec()),
        (
            "an extra member",
            format!("{{\"version\":0,\"stamp\":\"{STAMP_LA}\",\"x\":1}}").into_bytes(),
        ),
        ("empty stamp", b"{\"version\":0,\"stamp\":\"\"}".to_vec()),
    ];
    for (what, bytes) in cases {
        assert_due(&run_with_stored(&env, Slot::Bytes(bytes), Cmd::Up), what);
    }
}

#[test]
fn due_when_the_stamp_differs_from_the_one_just_computed() {
    let env = Env::new();
    for other in [
        STAMP_LA_OTHER_INSTANCE,
        STAMP_KOLKATA,
        STAMP_EMPTY_ZONE,
        STAMP_LONDON,
    ] {
        assert_due(
            &run_with_stored(&env, Slot::Bytes(stamp_file_bytes(other)), Cmd::Up),
            other,
        );
    }
}

#[test]
fn due_when_the_zone_is_the_only_thing_that_changed() {
    // The stored stamp covers an empty zone; this run has one.
    let env = Env::new();
    let r = run_with_stored(
        &env,
        Slot::Bytes(stamp_file_bytes(STAMP_EMPTY_ZONE)),
        Cmd::Up,
    );
    assert_due(&r, "zone changed");
}

#[test]
fn due_when_the_token_changed() {
    let env = Env::new();
    let mut s = env.scenario();
    s.stored_stamp = Slot::Bytes(stamp_file_bytes(STAMP_LA));
    s.unprotect = UnprotectBehavior::Returns(utf16le(TOKEN_B));
    let r = env.run(&s);
    assert_eq!(r.ssh_calls.len(), 1);
    assert_eq!(r.ssh_calls[0].stdin, content(BASE, TOKEN_B));
}

#[test]
fn due_when_the_url_changed() {
    let env = Env::new();
    let mut s = env.scenario();
    s.stored_stamp = Slot::Bytes(stamp_file_bytes(STAMP_LA));
    s.receipt = Slot::Bytes(enc_utf8(&env.receipt_text("\"http://127.0.0.1:8812\"")));
    let r = env.run(&s);
    assert_eq!(r.ssh_calls.len(), 1);
}

#[test]
fn due_when_the_instance_changed() {
    let env = Env::new();
    let mut s = env.scenario();
    s.stored_stamp = Slot::Bytes(stamp_file_bytes(STAMP_LA));
    s.instance = OTHER_INSTANCE.to_string();
    let r = env.run(&s);
    assert_eq!(r.ssh_calls.len(), 1);
}

#[test]
fn the_prototypes_stamp_file_is_never_read_written_or_removed() {
    // localswim-pushed.sha256 holds the matching hex, yet the push is due, and the file stays.
    let env = Env::new();
    let mut s = env.scenario();
    s.proto_stamp = Some(STAMP_LA.to_string());
    let r = env.run(&s);
    assert_due(&r, "prototype's stamp ignored");
    assert_eq!(
        r.proto_after.as_deref(),
        Some(STAMP_LA),
        "its bytes are as they were"
    );
}

#[test]
fn sync_forces_the_push_whatever_the_stamp_says() {
    let env = Env::new();
    let r = run_with_stored(&env, Slot::Bytes(stamp_file_bytes(STAMP_LA)), Cmd::Sync);
    assert_due(&r, "forced");
}

// ---- Not due -----------------------------------------------------------------------------------

#[test]
fn not_due_when_the_stored_stamp_equals_the_computed_one() {
    let env = Env::new();
    for mode in MODES {
        let mut s = env.scenario();
        s.mode = mode;
        s.stored_stamp = Slot::Bytes(stamp_file_bytes(STAMP_LA));
        let r = env.run(&s);
        assert_not_due(&r, "equal", &stamp_file_bytes(STAMP_LA));
    }
}

#[test]
fn not_due_with_the_stamp_in_any_form_json_and_the_schema_allow() {
    let env = Env::new();
    let compact = format!("{{\"version\":0,\"stamp\":\"{STAMP_LA}\"}}");
    let forms: Vec<Vec<u8>> = vec![
        enc_utf8_bom(&compact),
        enc_utf8(&format!("{compact}\n")),
        enc_utf8(&format!(
            "{{\n  \"version\": 0,\n  \"stamp\": \"{STAMP_LA}\"\n}}\r\n"
        )),
        enc_utf8(&format!("{{\"stamp\":\"{STAMP_LA}\",\"version\":0}}")),
        enc_utf8(&format!("  \t{compact}  ")),
    ];
    for bytes in forms {
        let r = run_with_stored(&env, Slot::Bytes(bytes.clone()), Cmd::Up);
        assert_not_due(&r, "an accepted form", &bytes);
    }
}

#[test]
fn not_due_compares_the_stamp_exactly() {
    // The stored stamp is uppercase by the schema; a lowercase copy is not a stamp at all and so
    // is due, rather than equal without regard to case.
    let env = Env::new();
    let lower = format!(
        "{{\"version\":0,\"stamp\":\"{}\"}}",
        STAMP_LA.to_lowercase()
    );
    assert_due(&run_with_stored(&env, stored(&lower), Cmd::Up), "lowercase");
}

#[test]
fn not_due_still_reads_both_files_and_so_still_warns_when_the_credential_is_bad() {
    let env = Env::new();
    let mut s = env.scenario();
    s.stored_stamp = Slot::Bytes(stamp_file_bytes(STAMP_LA));
    s.credential = Slot::Bytes(enc_utf8("not xml"));
    let r = env.run(&s);
    assert!(!r.pushed());
    assert_eq!(r.printed, vec![W_COULD_NOT_READ.to_string()]);
    assert_eq!(r.stamp_after, Some(stamp_file_bytes(STAMP_LA)));
}

// ---- When it runs ------------------------------------------------------------------------------

#[test]
fn a_nested_up_does_not_push_and_reads_nothing() {
    let env = Env::new();
    for mode in MODES {
        let mut s = env.scenario();
        s.mode = mode;
        s.nested = true;
        // Even with a credential that would warn, a nested up prints nothing.
        s.credential = Slot::Absent;
        let r = env.run(&s);
        assert!(!r.pushed());
        assert_eq!(r.printed, Vec::<String>::new(), "{mode:?}");
        assert!(r.unprotect_calls.is_empty());
        assert!(r.stamp_after.is_none());
    }
}

#[test]
fn a_nested_sync_still_pushes() {
    let env = Env::new();
    let mut s = env.scenario();
    s.nested = true;
    s.command = Cmd::Sync;
    s.stored_stamp = Slot::Bytes(stamp_file_bytes(STAMP_LA));
    assert_due(&env.run(&s), "nested sync");
}

#[test]
fn up_over_a_lease_alias_pushes_over_that_alias_and_sync_over_agentspace() {
    let env = Env::new();
    for (cmd, alias) in [
        (Cmd::Up, "agentspace-proj-0a1b2c3d"),
        (Cmd::Up, "agentspace"),
        (Cmd::Sync, "agentspace"),
    ] {
        let mut s = env.scenario();
        s.command = cmd;
        s.alias = alias.to_string();
        let r = env.run(&s);
        assert_eq!(r.ssh_calls.len(), 1);
        let a = &r.ssh_calls[0].args;
        assert_eq!(a.len(), 6, "{a:?}");
        assert_eq!(
            a[..4],
            ["-o", "ClearAllForwardings=yes", "-o", "BatchMode=yes"]
        );
        assert_eq!(a[4], alias);
        assert_remote_command_ok(&a[5]);
    }
}

#[test]
fn the_stamp_is_written_and_then_the_next_up_is_not_due() {
    let env = Env::new();
    let first = env.run(&env.scenario());
    assert_due(&first, "first");
    let mut again = env.scenario();
    again.keep_home = true;
    let second = env.run(&again);
    assert_not_due(&second, "second", &stamp_file_bytes(STAMP_LA));
}
