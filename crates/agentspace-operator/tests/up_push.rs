//! `up`'s token push through the test seam's stand-in `ssh`, in a throwaway home with a fake
//! localswim receipt and a dummy Bot credential (docs/operator.md: up, The token push, The test
//! seam, Secrecy). The push's own parts are tested in agentspace-push; these tests show that the
//! operator's `up` calls it as the page says: when, over which alias, with what on the command
//! line and stdin, what it records, and that it never fails the command.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use rig::*;

const ZONE: &str = "America/New_York";
const SENDING: &str = "Sending the localswim Bot token to the instance...";
const NOT_CONFIRMED: &str = "agentspace: The instance did not confirm the localswim token; the board will not work on the box this session.";
const COULD_NOT_READ: &str = "agentspace: Could not read the localswim Bot token; the board will not work on the box this session.";
const COULD_NOT_SEND: &str = "agentspace: Could not send the localswim token; the board will not work on the box this session.";
const NO_ZONE: &str =
    "agentspace: Could not work out the laptop's time zone; times on the box stay in its own zone.";

/// A rig with a cache, an old running box, a receipt and a credential.
fn pushing_rig() -> Rig {
    let rig = Rig::standard();
    rig.write_cache(ID);
    rig.localswim();
    rig
}

/// Makes the stand-in ssh answer as the box does: the hash of what it was sent, then the zone.
fn box_confirms(rig: &Rig, zone: Option<&str>) {
    let hash = sha256_hex(rig.content().as_bytes());
    let mut out = format!("{hash}  -\n");
    if let Some(z) = zone {
        out.push_str(&format!("zone {z}\n"));
    }
    rig.answer("ssh", None, Some(&out), None, Some(0));
}

fn ssh_calls(rig: &Rig) -> Vec<Call> {
    rig.calls_of("ssh")
}

#[test]
fn a_due_push_goes_over_the_alias_with_the_content_on_stdin_alone() {
    let rig = pushing_rig();
    box_confirms(&rig, Some(ZONE));
    let out = rig
        .op(&["up"])
        .env("0F1E2D3C_4B5A_6978_8796_A5B4C3D2E1F0_POSIX_FD_STATE", "1")
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.out(), "", "up writes nothing to stdout");
    assert_eq!(
        out.err_lines(),
        [format!("Ready: {ID}"), SENDING.to_string()]
    );
    assert_eq!(
        rig.keys(),
        ["ec2-describe-instances", "ec2-describe-instances", "ssh"],
        "the push comes after Ready, and is one ssh"
    );
    let ssh = &ssh_calls(&rig)[0];
    assert_eq!(
        &ssh.args[..5],
        [
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "BatchMode=yes",
            "agentspace"
        ]
    );
    assert_eq!(
        ssh.args.len(),
        6,
        "the remote command is one argument, the last"
    );
    assert_remote_command_rule(&ssh.args[5], "the push's remote command");
    assert!(
        ssh.args[5].contains(ZONE),
        "the command names the zone that is sent"
    );
    assert_eq!(
        String::from_utf8_lossy(&ssh.stdin),
        rig.content(),
        "the two lines, each ending in a line feed, and nothing more"
    );
    assert_eq!(
        ssh.stdin,
        rig.content().into_bytes(),
        "no byte order mark, no CR"
    );
    assert_eq!(
        ssh.nested, "1",
        "everything the operator starts gets AGENTSPACE_NESTED=1"
    );
    assert_eq!(
        ssh.fd_state, 0,
        "every *_POSIX_FD_STATE variable is removed"
    );
    assert_eq!(
        rig.stamp_bytes(),
        Some(rig.stamp_file(ZONE)),
        "the stamp, exactly"
    );
    assert!(rig.tmp_files().is_empty());
}

#[test]
fn the_token_is_nowhere_but_the_pushs_stdin() {
    let rig = pushing_rig();
    box_confirms(&rig, Some(ZONE));
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0));
    assert_eq!(
        rig.calls_of("ssh").len(),
        1,
        "the push ran, so there was a token to leak"
    );
    for c in rig.calls() {
        assert!(
            !c.secret,
            "{}: the token reached an argument or an environment variable",
            c.key
        );
    }
    assert!(!out.err().contains(&rig.token) && !out.out().contains(&rig.token));
    assert_eq!(
        rig.files_holding_the_token(),
        Vec::<std::path::PathBuf>::new(),
        "no file on the laptop holds the token, only a hash that covers it"
    );
    let other_calls = rig.calls().into_iter().filter(|c| c.role != "ssh");
    for c in other_calls {
        assert!(!contains(&c.stdin, rig.token.as_bytes()), "{}", c.key);
    }
}

#[test]
fn the_alias_is_the_one_given() {
    let rig = pushing_rig();
    box_confirms(&rig, Some(ZONE));
    let out = rig.run(&["up", "-Alias", "dev-box.2_x"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let ssh = &ssh_calls(&rig)[0];
    assert_eq!(ssh.args[4], "dev-box.2_x");
    assert_eq!(ssh.args.len(), 6);
}

#[test]
fn a_matching_stamp_means_nothing_is_sent_and_nothing_is_printed() {
    let rig = pushing_rig();
    rig.write_stamp_for(ZONE);
    let before = rig.stamp_bytes();
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), format!("Ready: {ID}\n"));
    assert!(ssh_calls(&rig).is_empty());
    assert_eq!(rig.stamp_bytes(), before);
}

#[test]
fn a_push_is_due_when_the_stamp_is_missing_unreadable_or_different() {
    let cases: Vec<(&str, Option<Vec<u8>>)> = vec![
        ("missing", None),
        ("empty", Some(Vec::new())),
        ("not JSON", Some(b"not json".to_vec())),
        ("the prototype's hex alone", Some(
            expected_stamp(ID, ZONE, "x").into_bytes(),
        )),
        ("wrong version", Some(
            br#"{"version":1,"stamp":"0000000000000000000000000000000000000000000000000000000000000000"}"#.to_vec(),
        )),
        ("another zone", Some({
            let rig = pushing_rig();
            rig.stamp_file("Europe/Paris")
        })),
        ("another instance", Some(
            format!(
                "{{\"version\":0,\"stamp\":\"{}\"}}",
                expected_stamp(OTHER_ID, ZONE, "LOCALSWIM_URL=x\nLOCALSWIM_TOKEN=yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy\n")
            )
            .into_bytes(),
        )),
        ("UTF-16", Some({
            let mut b = vec![0xFF, 0xFE];
            b.extend(utf16le(r#"{"version":0,"stamp":"0000000000000000000000000000000000000000000000000000000000000000"}"#));
            b
        })),
    ];
    for (what, bytes) in cases {
        let rig = pushing_rig();
        box_confirms(&rig, Some(ZONE));
        if let Some(b) = bytes {
            std::fs::create_dir_all(rig.state_dir()).unwrap();
            std::fs::write(rig.stamp_path(), b).unwrap();
        }
        let out = rig.run(&["up"]);
        assert_eq!(out.code, Some(0), "{what}: {:?}", out.err());
        assert_eq!(ssh_calls(&rig).len(), 1, "{what}: a push is due");
        assert_eq!(
            rig.stamp_bytes(),
            Some(rig.stamp_file(ZONE)),
            "{what}: and is recorded"
        );
    }
}

#[test]
fn a_stamp_for_another_token_makes_a_push_due() {
    let rig = pushing_rig();
    box_confirms(&rig, Some(ZONE));
    let old_content = format!(
        "LOCALSWIM_URL={BASE}\nLOCALSWIM_TOKEN=an-older-dummy-token-AAAAAAAAAAAAAAAAAAAAAA\n"
    );
    std::fs::create_dir_all(rig.state_dir()).unwrap();
    std::fs::write(
        rig.stamp_path(),
        format!(
            "{{\"version\":0,\"stamp\":\"{}\"}}",
            expected_stamp(ID, ZONE, &old_content)
        ),
    )
    .unwrap();
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0));
    assert_eq!(ssh_calls(&rig).len(), 1);
    assert_eq!(rig.stamp_bytes(), Some(rig.stamp_file(ZONE)));
}

#[test]
fn the_zone_is_mapped_checked_and_sent() {
    let rig = pushing_rig();
    box_confirms(&rig, Some("Asia/Kolkata"));
    let out = rig
        .op(&["up"])
        .env("AGENTSPACE_TIMEZONE", "Asia/Calcutta")
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let ssh = &ssh_calls(&rig)[0];
    assert!(
        ssh.args[5].contains("Asia/Kolkata"),
        "a legacy name is mapped"
    );
    assert!(!ssh.args[5].contains("Asia/Calcutta"));
    assert_eq!(rig.stamp_bytes(), Some(rig.stamp_file("Asia/Kolkata")));
    assert_eq!(
        out.err_lines(),
        [format!("Ready: {ID}"), SENDING.to_string()]
    );
}

#[test]
fn a_zone_that_fails_the_check_is_not_sent_and_warns() {
    for bad in [
        "not a zone",
        "-Fx",
        "a;b",
        "UTC\nx",
        "Europe/Pa ris",
        "$(id)",
    ] {
        let rig = pushing_rig();
        box_confirms(&rig, None);
        let out = rig.op(&["up"]).env("AGENTSPACE_TIMEZONE", bad).run();
        assert_eq!(out.code, Some(0), "{bad:?}: {:?}", out.err());
        assert_eq!(
            out.err_lines(),
            [
                format!("Ready: {ID}"),
                SENDING.to_string(),
                NO_ZONE.to_string()
            ],
            "{bad:?}"
        );
        let ssh = &ssh_calls(&rig)[0];
        assert_eq!(ssh.args.len(), 6, "{bad:?}");
        assert!(
            !ssh.args[5].contains(bad),
            "{bad:?}: the bad zone never reaches the box's command line"
        );
        assert_eq!(
            rig.stamp_bytes(),
            Some(rig.stamp_file("")),
            "{bad:?}: the stamp carries the empty zone"
        );
    }
}

// ---- Outcomes: a push never fails the command ----

#[test]
fn an_ssh_that_fails_warns_and_up_still_succeeds() {
    for what in [
        "exit 255",
        "exit 1 with the right hash",
        "no hash line",
        "a wrong hash",
    ] {
        let rig = pushing_rig();
        let right = sha256_hex(rig.content().as_bytes());
        let (out_text, code) = match what {
            "exit 255" => (String::new(), 255),
            "exit 1 with the right hash" => (
                format!(
                    "{right}  -
zone {ZONE}
"
                ),
                1,
            ),
            "no hash line" => (
                format!(
                    "Welcome
zone {ZONE}
"
                ),
                0,
            ),
            _ => (
                format!(
                    "{}  -
zone {ZONE}
",
                    "0".repeat(64)
                ),
                0,
            ),
        };
        rig.answer("ssh", None, Some(&out_text), None, Some(code));
        let out = rig.run(&["up"]);
        assert_eq!(out.code, Some(0), "{what}: a push never fails the command");
        assert_eq!(
            out.err_lines(),
            [
                format!("Ready: {ID}"),
                SENDING.to_string(),
                NOT_CONFIRMED.to_string()
            ],
            "{what}"
        );
        assert_eq!(rig.stamp_bytes(), None, "{what}: nothing is recorded");
        assert_eq!(out.out(), "", "{what}");
    }
}

#[test]
fn a_banner_before_the_hash_line_and_an_upper_case_hash_are_fine() {
    let rig = pushing_rig();
    let hash = sha256_hex(rig.content().as_bytes()).to_ascii_uppercase();
    rig.answer(
        "ssh",
        None,
        Some(&format!(
            "Welcome to the box\r\nnot a hash line\r\n{hash}  -\r\nzone {ZONE}\r\n"
        )),
        Some("The host is ready.\n"),
        Some(0),
    );
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0));
    assert_eq!(
        out.err_lines(),
        [
            format!("Ready: {ID}"),
            SENDING.to_string(),
            "The host is ready.".to_string(),
        ],
        "the ssh's own stderr is the operator's stderr, which carries its nested ProxyCommand's lines"
    );
    assert_eq!(rig.stamp_bytes(), Some(rig.stamp_file(ZONE)));
    assert_eq!(
        out.out(),
        "",
        "the ssh's stdout is read, never passed through"
    );
}

#[test]
fn a_zone_the_box_did_not_take_warns_and_records_nothing() {
    for zone_line in ["zone-failed\n", ""] {
        let rig = pushing_rig();
        let hash = sha256_hex(rig.content().as_bytes());
        rig.answer(
            "ssh",
            None,
            Some(&format!("{hash}  -\n{zone_line}")),
            None,
            Some(0),
        );
        let out = rig.run(&["up"]);
        assert_eq!(out.code, Some(0), "{zone_line:?}");
        assert_eq!(
            out.err_lines(),
            [
                format!("Ready: {ID}"),
                SENDING.to_string(),
                format!(
                    "agentspace: The instance did not take the time zone {ZONE}; times on the box stay in its own zone."
                ),
            ],
            "{zone_line:?}"
        );
        assert_eq!(rig.stamp_bytes(), None, "{zone_line:?}");
    }
}

#[test]
fn an_ssh_that_cannot_be_started_warns_like_any_other_failure() {
    let rig = pushing_rig();
    let missing = rig.root().join("no-such-ssh.exe");
    let out = rig.op(&["up"]).env("AGENTSPACE_SSH_CMD", &missing).run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let lines = out.err_lines();
    assert_eq!(
        lines.first().map(String::as_str),
        Some(format!("Ready: {ID}").as_str())
    );
    assert_eq!(
        lines.last().map(String::as_str),
        Some(COULD_NOT_SEND),
        "{lines:?}"
    );
    assert_eq!(rig.stamp_bytes(), None);
}

#[test]
fn a_stamp_that_cannot_be_written_warns_and_up_still_succeeds() {
    let rig = pushing_rig();
    box_confirms(&rig, Some(ZONE));
    std::fs::create_dir_all(rig.stamp_path()).unwrap();
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err_lines().last().map(String::as_str),
        Some(COULD_NOT_SEND),
        "{:?}",
        out.err()
    );
}

#[test]
fn quiet_hides_every_push_message_and_warning() {
    for confirmed in [true, false] {
        let rig = pushing_rig();
        if confirmed {
            box_confirms(&rig, Some(ZONE));
        } else {
            rig.answer("ssh", None, None, None, Some(255));
        }
        let out = rig.run(&["up", "-Quiet"]);
        assert_eq!(out.code, Some(0), "confirmed {confirmed}");
        assert_eq!(out.err(), "", "confirmed {confirmed}");
        assert_eq!(ssh_calls(&rig).len(), 1, "-Quiet still pushes");
    }
    let rig = pushing_rig();
    box_confirms(&rig, None);
    let out = rig
        .op(&["up", "-Quiet"])
        .env("AGENTSPACE_TIMEZONE", "not a zone")
        .run();
    assert_eq!(out.err(), "", "the no-zone warning is quiet too");
}

// ---- Nested up ----

#[test]
fn a_nested_up_does_not_push_and_does_not_read_the_receipt_for_a_message() {
    for value in ["1", "yes", "0"] {
        let rig = pushing_rig();
        box_confirms(&rig, Some(ZONE));
        let out = rig.op(&["up"]).env("AGENTSPACE_NESTED", value).run();
        assert_eq!(out.code, Some(0), "{value}: {:?}", out.err());
        assert_eq!(
            out.err(),
            format!("Ready: {ID}\n"),
            "{value}: no Sending line"
        );
        assert!(
            ssh_calls(&rig).is_empty(),
            "{value}: AGENTSPACE_NESTED set stops the push"
        );
        assert_eq!(rig.stamp_bytes(), None, "{value}");
        assert_eq!(
            rig.keys(),
            ["ec2-describe-instances", "ec2-describe-instances"]
        );
    }
}

#[test]
fn a_nested_up_is_not_told_of_a_bad_credential() {
    // The push never starts, so a credential that could not be read is not reported either.
    let rig = pushing_rig();
    std::fs::write(rig.credential_dir().join("bot.json"), "garbage").unwrap();
    let out = rig.op(&["up"]).env("AGENTSPACE_NESTED", "1").run();
    assert_eq!(out.code, Some(0));
    assert_eq!(out.err(), format!("Ready: {ID}\n"));
}

#[test]
fn an_empty_agentspace_nested_does_not_count_as_set() {
    let rig = pushing_rig();
    box_confirms(&rig, Some(ZONE));
    let out = rig.op(&["up"]).env("AGENTSPACE_NESTED", "").run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        ssh_calls(&rig).len(),
        1,
        "set and not empty stops the push; empty does not"
    );
}

// ---- What the laptop reads ----

#[test]
fn no_receipt_means_no_push_and_no_message() {
    let rig = Rig::standard();
    rig.write_cache(ID);
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0));
    assert_eq!(out.err(), format!("Ready: {ID}\n"));
    assert!(ssh_calls(&rig).is_empty());

    // LOCALAPPDATA unset or empty is the same, and fails no command.
    for how in ["unset", "empty", "blank"] {
        let rig = pushing_rig();
        box_confirms(&rig, Some(ZONE));
        let op = rig.op(&["up"]);
        let out = match how {
            "unset" => op.env_remove("LOCALAPPDATA").run(),
            "empty" => op.env("LOCALAPPDATA", "").run(),
            _ => op.env("LOCALAPPDATA", "  ").run(),
        };
        assert_eq!(out.code, Some(0), "{how}: {:?}", out.err());
        assert_eq!(out.err(), format!("Ready: {ID}\n"), "{how}");
        assert!(ssh_calls(&rig).is_empty(), "{how}");
    }
}

#[test]
fn a_receipt_or_credential_that_cannot_be_read_warns_and_sends_nothing() {
    type Breaker = Box<dyn Fn(&Rig)>;
    let breakages: Vec<(&str, Breaker)> = vec![
        (
            "a receipt that is not JSON",
            Box::new(|r: &Rig| {
                std::fs::write(r.localswim_dir().join("service.json"), "{").unwrap();
            }),
        ),
        (
            "a receipt that is an array",
            Box::new(|r: &Rig| {
                std::fs::write(r.localswim_dir().join("service.json"), "[]").unwrap();
            }),
        ),
        (
            "a receipt that is a folder",
            Box::new(|r: &Rig| {
                std::fs::remove_file(r.localswim_dir().join("service.json")).unwrap();
                std::fs::create_dir(r.localswim_dir().join("service.json")).unwrap();
            }),
        ),
        (
            "a receipt with no credential folder",
            Box::new(|r: &Rig| {
                std::fs::write(
                    r.localswim_dir().join("service.json"),
                    format!("{{\"base\":\"{BASE}\"}}"),
                )
                .unwrap();
            }),
        ),
        (
            "a missing bot.json",
            Box::new(|r: &Rig| {
                std::fs::remove_file(r.credential_dir().join("bot.json")).unwrap();
            }),
        ),
        (
            "a bot.json that is not a credential",
            Box::new(|r: &Rig| {
                std::fs::write(r.credential_dir().join("bot.json"), "{}").unwrap();
            }),
        ),
        (
            "a bot.json with a member too many",
            Box::new(|r: &Rig| {
                let text = std::fs::read_to_string(r.credential_dir().join("bot.json")).unwrap();
                let text = text.replacen('{', "{\"extra\":1,", 1);
                std::fs::write(r.credential_dir().join("bot.json"), text).unwrap();
            }),
        ),
    ];
    for (what, breaker) in breakages {
        let rig = pushing_rig();
        box_confirms(&rig, Some(ZONE));
        breaker(&rig);
        let out = rig.run(&["up"]);
        assert_eq!(out.code, Some(0), "{what}: {:?}", out.err());
        assert_eq!(
            out.err_lines(),
            [format!("Ready: {ID}"), COULD_NOT_READ.to_string()],
            "{what}: the warning is a fixed text, and no Sending line"
        );
        assert!(ssh_calls(&rig).is_empty(), "{what}");
        assert_eq!(rig.stamp_bytes(), None, "{what}");
    }
}

#[test]
fn the_credential_is_read_even_when_the_stamp_says_nothing_is_due() {
    // The stamp covers the token, so a credential that cannot be read warns at every up.
    let rig = pushing_rig();
    rig.write_stamp_for(ZONE);
    std::fs::write(rig.credential_dir().join("bot.json"), "garbage").unwrap();
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0));
    assert_eq!(
        out.err_lines().last().map(String::as_str),
        Some(COULD_NOT_READ)
    );
}

#[test]
fn quiet_hides_the_could_not_read_warning() {
    let rig = pushing_rig();
    std::fs::write(rig.credential_dir().join("bot.json"), "garbage").unwrap();
    let out = rig.run(&["up", "-Quiet"]);
    assert_eq!(out.code, Some(0));
    assert_eq!(out.err(), "");
}

// ---- Nothing an up starts reads or writes where it must not ----

#[test]
fn nothing_up_starts_writes_to_stdout() {
    let rig = pushing_rig();
    rig.instance("stopping");
    rig.answer("ec2-wait", None, Some("WAIT-STDOUT\n"), None, Some(0));
    rig.answer(
        "ec2-start-instances",
        None,
        Some("{\"StartingInstances\":[]}\n"),
        None,
        Some(0),
    );
    box_confirms(&rig, Some(ZONE));
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.out(),
        "",
        "up MUST NOT let anything it starts write to stdout"
    );
}

#[test]
fn no_program_up_starts_gets_the_operators_stdin_but_the_push_gets_its_own() {
    let rig = pushing_rig();
    rig.instance("stopped");
    box_confirms(&rig, Some(ZONE));
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    for c in rig.calls() {
        if c.role == "ssh" {
            assert_eq!(
                c.stdin,
                rig.content().into_bytes(),
                "the push's stdin is the content alone"
            );
        } else {
            assert!(
                c.stdin.is_empty(),
                "{}: every other program gets an empty or closed stdin ({} bytes seen)",
                c.key,
                c.stdin.len()
            );
        }
    }
}

#[test]
fn up_does_not_wait_for_its_stdin() {
    // The stdin is open and empty for the whole run; a run that read it, or let a program read
    // it, would wait until the rig's time limit and fail there.
    let rig = pushing_rig();
    rig.instance("stopped");
    box_confirms(&rig, Some(ZONE));
    let out = rig
        .op(&["up"])
        .hold_stdin()
        .timeout(std::time::Duration::from_secs(60))
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(rig.calls_of("ssh").len(), 1);
}
