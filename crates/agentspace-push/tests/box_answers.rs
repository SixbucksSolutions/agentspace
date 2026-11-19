//! What the laptop sends and checks: the ssh's arguments, the box's answer lines and each
//! outcome's warning, in a person's mode and in the caller's, and when the stamp is written
//! (operator.md, What the laptop sends, What the box does, What the laptop checks).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;
use agentspace_operator_api::Mode;
use common::*;

fn answer(code: i32, stdout: &str) -> SshBehavior {
    SshBehavior::Answer {
        code,
        stdout: stdout.as_bytes().to_vec(),
    }
}

fn scenario_with(env: &Env, mode: Mode, zone: Option<&str>, ssh: SshBehavior) -> Scenario {
    let mut s = env.scenario();
    s.mode = mode;
    s.zone = zone.map(str::to_string);
    s.confirm_zone = zone.map(str::to_string);
    s.ssh = ssh;
    s
}

fn stamp_bytes(stamp: &str) -> Vec<u8> {
    stamp_file_bytes(stamp)
}

/// Asserts the push was sent once with the standard content and the stamp stayed as stored.
fn assert_sent_and_unrecorded(r: &Run, before: Option<&[u8]>) {
    assert_eq!(r.ssh_calls.len(), 1, "{r:?}");
    assert_eq!(r.ssh_calls[0].stdin, CONTENT.as_bytes());
    assert_eq!(
        r.stamp_after.as_deref(),
        before,
        "the stored stamp is as it was"
    );
}

// ---- What is sent -----------------------------------------------------------------------------

#[test]
fn ssh_gets_the_two_options_the_alias_and_one_remote_command_last() {
    let env = Env::new();
    let r = env.run(&env.scenario());
    let c = &r.ssh_calls[0];
    assert_eq!(c.args.len(), 6, "{:?}", c.args);
    assert_eq!(
        c.args[..5],
        [
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "BatchMode=yes",
            ALIAS
        ]
    );
    assert_remote_command_ok(&c.args[5]);
    assert!(
        c.args[5].contains(ZONE),
        "the remote command names the zone: {:?}",
        c.args[5]
    );
}

#[test]
fn the_push_passes_no_identity_option_of_its_own() {
    let env = Env::new();
    let r = env.run(&env.scenario());
    let joined = r.ssh_calls[0].args.join(" ");
    assert!(
        !joined.contains("-i ")
            && !joined.contains("IdentityFile")
            && !joined.contains("Certificate")
    );
}

#[test]
fn the_content_goes_on_stdin_alone_with_no_bom_and_exactly_two_lf_lines() {
    let env = Env::new();
    let r = env.run(&env.scenario());
    let stdin = &r.ssh_calls[0].stdin;
    assert_eq!(stdin.as_slice(), CONTENT.as_bytes());
    assert!(!stdin.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert_eq!(stdin.iter().filter(|&&b| b == b'\n').count(), 2);
    assert!(!stdin.contains(&b'\r'));
}

#[test]
fn the_remote_command_obeys_its_rules_for_every_zone_that_passes() {
    let env = Env::new();
    for z in [
        "America/Los_Angeles",
        "UTC",
        "Etc/GMT+5",
        "America/Argentina/Buenos_Aires",
        "EST5EDT",
    ] {
        let mut s = scenario_with(&env, Mode::Person, Some(z), SshBehavior::Confirm);
        s.confirm_zone = Some(z.to_string());
        let r = env.run(&s);
        let cmd = r.ssh_calls[0].args.last().unwrap().clone();
        assert_remote_command_ok(&cmd);
        assert!(cmd.contains(z), "{cmd:?} names {z}");
    }
}

#[test]
fn a_zone_that_fails_its_check_is_never_put_on_the_command_line() {
    let env = Env::new();
    for bad in ["-Foo", "a b", "UTC\n", "a;b", "a\"b", "$(x)", "..", ""] {
        let s = scenario_with(&env, Mode::Person, Some(bad), SshBehavior::Confirm);
        let r = env.run(&s);
        assert_eq!(r.ssh_calls.len(), 1, "{bad:?}");
        let cmd = r.ssh_calls[0].args.last().unwrap();
        assert_remote_command_ok(cmd);
        if !bad.is_empty() {
            assert!(!cmd.contains(bad), "{bad:?} in {cmd:?}");
        }
        // The stamp carries the empty zone.
        assert_eq!(
            r.stamp_after,
            Some(stamp_bytes(STAMP_EMPTY_ZONE)),
            "{bad:?}"
        );
        assert_eq!(
            r.printed,
            vec![SENDING.to_string(), W_NO_ZONE.to_string()],
            "{bad:?}"
        );
    }
}

#[test]
fn a_legacy_zone_name_is_renamed_before_it_is_sent_and_stamped() {
    // The rename is `agentspace-zone`'s, and the push holds no copy of the table (architect,
    // 2026-11-19, for E0.06-T01-S04): the zone reaches the push as the operator works it out.
    let env = Env::new();
    let zone = operator_zone("Asia/Calcutta");
    let mut s = scenario_with(&env, Mode::Person, zone.as_deref(), SshBehavior::Confirm);
    s.confirm_zone = Some("Asia/Kolkata".to_string());
    let r = env.run(&s);
    let cmd = r.ssh_calls[0].args.last().unwrap();
    assert!(
        cmd.contains("Asia/Kolkata") && !cmd.contains("Calcutta"),
        "{cmd:?}"
    );
    assert_eq!(r.stamp_after, Some(stamp_bytes(STAMP_KOLKATA)));
    assert_eq!(r.printed, vec![SENDING.to_string()]);

    // A box that answers with the old name did not take the new one.
    s.ssh = answer(0, &format!("{HASH_LOWER}  -\nzone Asia/Calcutta\n"));
    let r = env.run(&s);
    assert_eq!(
        r.printed,
        vec![SENDING.to_string(), w_zone_not_taken("Asia/Kolkata")]
    );
}

// ---- Confirmed ---------------------------------------------------------------------------------

#[test]
fn token_and_zone_confirmed_writes_the_stamp_and_says_only_that_it_is_sending() {
    let env = Env::new();
    for mode in MODES {
        let s = scenario_with(&env, mode, Some(ZONE), SshBehavior::Confirm);
        let r = env.run(&s);
        assert_eq!(
            r.printed,
            expected_printed(mode, &[Item::Sending]),
            "{mode:?}"
        );
        assert_eq!(r.stamp_after, Some(stamp_bytes(STAMP_LA)));
        assert_eq!(r.ssh_calls.len(), 1);
        assert_eq!(r.ssh_calls[0].stdin, CONTENT.as_bytes());
    }
}

#[test]
fn sending_is_a_person_line_with_no_caller_line() {
    let env = Env::new();
    let r = env.run(&env.scenario());
    assert_eq!(
        r.events.first(),
        Some(&Ev::Progress {
            person: SENDING.to_string(),
            caller: None
        })
    );
    // In caller mode it prints nothing at all.
    let mut s = env.scenario();
    s.mode = Mode::Caller;
    assert_eq!(env.run(&s).printed, Vec::<String>::new());
}

#[test]
fn token_confirmed_with_no_zone_warns_about_the_zone_and_records_the_empty_zone() {
    let env = Env::new();
    for mode in MODES {
        let s = scenario_with(&env, mode, None, SshBehavior::Confirm);
        let r = env.run(&s);
        assert_eq!(
            r.printed,
            expected_printed(mode, &[Item::Sending, Item::Warn(W_NO_ZONE)]),
            "{mode:?}"
        );
        assert_eq!(r.stamp_after, Some(stamp_bytes(STAMP_EMPTY_ZONE)));
    }
}

#[test]
fn no_zone_line_is_needed_when_no_zone_was_sent() {
    let env = Env::new();
    for stdout in [
        format!("{HASH_LOWER}  -\n"),
        format!("{HASH_LOWER}  -\nzone-failed\n"),
        format!("{HASH_LOWER}  -\nzone America/Los_Angeles\n"),
    ] {
        let s = scenario_with(&env, Mode::Person, None, answer(0, &stdout));
        let r = env.run(&s);
        assert_eq!(
            r.printed,
            vec![SENDING.to_string(), W_NO_ZONE.to_string()],
            "{stdout:?}"
        );
        assert_eq!(r.stamp_after, Some(stamp_bytes(STAMP_EMPTY_ZONE)));
    }
}

#[test]
fn the_hash_line_is_found_among_other_lines_and_compared_without_regard_to_case() {
    let env = Env::new();
    let z = format!("zone {ZONE}");
    let ok = [
        format!("{HASH_LOWER}  -\n{z}\n"),
        format!("{HASH_UPPER}  -\n{z}\n"),
        format!("{HASH_LOWER} -\n{z}\n"),
        format!("{HASH_LOWER}\t-\n{z}\n"),
        format!("{HASH_LOWER}\n{z}\n"),
        format!("{HASH_LOWER}\r\n{z}\r\n"),
        format!("{HASH_LOWER}  -\r\n{z}\r\n"),
        format!("Welcome to the box\nLast login: never\n{HASH_LOWER}  -\n{z}\n"),
        format!("\n\n{HASH_LOWER}  -\nsome banner after\n{z}\n"),
        format!("banner mentions {HASH_UPPER} inside a longer line\n{HASH_LOWER}  -\n{z}\n"),
        format!("{z}\n{HASH_LOWER}  -\n"),
    ];
    for stdout in ok {
        let s = scenario_with(&env, Mode::Person, Some(ZONE), answer(0, &stdout));
        let r = env.run(&s);
        assert_eq!(r.printed, vec![SENDING.to_string()], "{stdout:?}");
        assert_eq!(r.stamp_after, Some(stamp_bytes(STAMP_LA)), "{stdout:?}");
    }
}

// ---- The token is not confirmed ------------------------------------------------------------------

#[test]
fn the_token_is_not_confirmed_without_a_matching_first_hash_line_and_a_zero_exit() {
    let env = Env::new();
    let other = "0".repeat(64);
    let off_by_one = format!(
        "{}{}",
        &HASH_LOWER[..63],
        if HASH_LOWER.ends_with('0') { '1' } else { '0' }
    );
    let z = format!("zone {ZONE}\n");
    let cases: Vec<(&str, i32, String)> = vec![
        (
            "exit 1 with the right hash",
            1,
            format!("{HASH_LOWER}  -\n{z}"),
        ),
        (
            "exit 255 with the right hash",
            255,
            format!("{HASH_LOWER}  -\n{z}"),
        ),
        (
            "exit 2 with the right hash",
            2,
            format!("{HASH_LOWER}  -\n{z}"),
        ),
        ("no output", 0, String::new()),
        ("only a zone line", 0, z.clone()),
        ("a wrong hash", 0, format!("{other}  -\n{z}")),
        (
            "a hash off by one digit",
            0,
            format!("{off_by_one}  -\n{z}"),
        ),
        (
            "the wrong hash first, the right one second",
            0,
            format!("{other}  -\n{HASH_LOWER}  -\n{z}"),
        ),
        ("63 digits", 0, format!("{}  -\n{z}", &HASH_LOWER[1..])),
        ("65 digits", 0, format!("{HASH_LOWER}0  -\n{z}")),
        (
            "a hash followed by a non-space",
            0,
            format!("{HASH_LOWER}x -\n{z}"),
        ),
        (
            "a hash with leading space",
            0,
            format!(" {HASH_LOWER}  -\n{z}"),
        ),
        (
            "a non-hex digit",
            0,
            format!("g{}  -\n{z}", &HASH_LOWER[1..]),
        ),
        ("a banner line of words", 0, format!("hello there\n{z}")),
        ("zone-failed alone", 0, "zone-failed\n".to_string()),
    ];
    for mode in MODES {
        for (what, code, stdout) in &cases {
            let mut s = scenario_with(&env, mode, Some(ZONE), answer(*code, stdout));
            s.stored_stamp = Slot::Bytes(stamp_bytes(STAMP_LONDON));
            let r = env.run(&s);
            assert_eq!(
                r.printed,
                expected_printed(mode, &[Item::Sending, Item::Warn(W_NOT_CONFIRMED)]),
                "{what} {mode:?}"
            );
            assert_sent_and_unrecorded(&r, Some(&stamp_bytes(STAMP_LONDON)));
        }
    }
}

#[test]
fn an_unconfirmed_token_checks_no_zone() {
    // The zone failed too, and only the token's warning prints.
    let env = Env::new();
    let s = scenario_with(&env, Mode::Person, Some(ZONE), answer(1, "zone-failed\n"));
    let r = env.run(&s);
    assert_eq!(
        r.printed,
        vec![SENDING.to_string(), W_NOT_CONFIRMED.to_string()]
    );
    assert!(r.stamp_after.is_none());
}

#[test]
fn an_unconfirmed_token_with_no_zone_warns_about_the_zone_first_then_the_token() {
    let env = Env::new();
    let s = scenario_with(&env, Mode::Person, None, answer(255, ""));
    let r = env.run(&s);
    assert_eq!(
        r.printed,
        vec![
            SENDING.to_string(),
            W_NO_ZONE.to_string(),
            W_NOT_CONFIRMED.to_string()
        ]
    );
    assert!(r.stamp_after.is_none());
}

// ---- The zone is not confirmed --------------------------------------------------------------------

#[test]
fn a_zone_that_is_not_confirmed_warns_and_records_nothing_but_the_token_stays() {
    let env = Env::new();
    let h = format!("{HASH_LOWER}  -\n");
    let cases: Vec<(&str, String)> = vec![
        ("zone-failed", format!("{h}zone-failed\n")),
        ("no zone line", h.clone()),
        ("another zone", format!("{h}zone Europe/London\n")),
        (
            "the zone in another case",
            format!("{h}zone america/los_angeles\n"),
        ),
        (
            "a trailing space",
            format!("{h}zone America/Los_Angeles \n"),
        ),
        ("a leading space", format!("{h} zone America/Los_Angeles\n")),
        ("a longer zone", format!("{h}zone America/Los_Angeles2\n")),
        (
            "no space after zone",
            format!("{h}zoneAmerica/Los_Angeles\n"),
        ),
        ("the zone alone", format!("{h}America/Los_Angeles\n")),
    ];
    for mode in MODES {
        for (what, stdout) in &cases {
            let mut s = scenario_with(&env, mode, Some(ZONE), answer(0, stdout));
            s.stored_stamp = Slot::Bytes(stamp_bytes(STAMP_LONDON));
            let r = env.run(&s);
            assert_eq!(
                r.printed,
                expected_printed(mode, &[Item::Sending, Item::Warn(&w_zone_not_taken(ZONE))]),
                "{what} {mode:?}"
            );
            assert_sent_and_unrecorded(&r, Some(&stamp_bytes(STAMP_LONDON)));
        }
    }
}

#[test]
fn a_zone_line_ending_in_crlf_is_confirmed() {
    let env = Env::new();
    let s = scenario_with(
        &env,
        Mode::Person,
        Some(ZONE),
        answer(0, &format!("{HASH_LOWER}  -\r\nzone {ZONE}\r\n")),
    );
    let r = env.run(&s);
    assert_eq!(r.printed, vec![SENDING.to_string()]);
    assert_eq!(r.stamp_after, Some(stamp_bytes(STAMP_LA)));
}

#[test]
fn the_zone_warning_names_the_checked_zone() {
    let env = Env::new();
    let s = scenario_with(
        &env,
        Mode::Person,
        Some("Europe/London"),
        answer(0, &format!("{HASH_LOWER}  -\nzone-failed\n")),
    );
    let r = env.run(&s);
    assert_eq!(
        r.printed,
        vec![SENDING.to_string(), w_zone_not_taken("Europe/London")]
    );
}

// ---- Other failures -------------------------------------------------------------------------------

#[test]
fn an_ssh_that_runs_past_twenty_seconds_warns_and_records_nothing() {
    let env = Env::new();
    for mode in MODES {
        let mut s = scenario_with(&env, mode, Some(ZONE), SshBehavior::TimedOut);
        s.stored_stamp = Slot::Bytes(stamp_bytes(STAMP_LONDON));
        let r = env.run(&s);
        assert_eq!(
            r.printed,
            expected_printed(mode, &[Item::Sending, Item::Warn(W_TIMED_OUT)]),
            "{mode:?}"
        );
        assert_sent_and_unrecorded(&r, Some(&stamp_bytes(STAMP_LONDON)));
    }
}

#[test]
fn an_ssh_that_cannot_be_started_warns_could_not_send_and_records_nothing() {
    let env = Env::new();
    for mode in MODES {
        let mut s = scenario_with(&env, mode, Some(ZONE), SshBehavior::CouldNotStart);
        s.stored_stamp = Slot::Bytes(stamp_bytes(STAMP_LONDON));
        let r = env.run(&s);
        assert_eq!(
            r.printed,
            expected_printed(mode, &[Item::Sending, Item::Warn(W_COULD_NOT_SEND)]),
            "{mode:?}"
        );
        assert_eq!(r.stamp_after, Some(stamp_bytes(STAMP_LONDON)));
    }
}

#[test]
fn a_stamp_that_cannot_be_written_warns_could_not_send() {
    let env = Env::new();
    for mode in MODES {
        let mut s = scenario_with(&env, mode, Some(ZONE), SshBehavior::Confirm);
        s.block_stamp_dir = true;
        let r = env.run(&s);
        assert_eq!(
            r.printed,
            expected_printed(mode, &[Item::Sending, Item::Warn(W_COULD_NOT_SEND)]),
            "{mode:?}"
        );
        assert!(r.stamp_after.is_none());
    }
}

#[test]
fn a_bad_alias_warns_and_sends_nothing() {
    // By ruling (architect, 2026-11-19, for E0.06-T01-S04): an alias that breaks `-Alias`'s rule
    // is "any other failure" if one ever reaches the push: no `Sending` line, the one "Could not
    // send" warning, and nothing read before the check.
    let env = Env::new();
    for alias in [
        "",
        "-oProxyCommand=x",
        "-",
        "a b",
        "a;b",
        "a\nb",
        "a/b",
        "a\"b",
        "a$b",
        "a%b",
    ] {
        for mode in MODES {
            let mut s = env.scenario();
            s.mode = mode;
            s.alias = alias.to_string();
            let r = env.run(&s);
            assert!(!r.pushed(), "{alias:?}: no ssh");
            assert!(r.stamp_after.is_none(), "{alias:?}: nothing recorded");
            assert!(
                r.unprotect_calls.is_empty(),
                "{alias:?}: the credential is not read"
            );
            assert_eq!(
                r.printed,
                expected_printed(mode, &[Item::Warn(W_COULD_NOT_SEND)]),
                "{alias:?} {mode:?}: no Sending line, and the one warning"
            );
        }
    }
    // `sync`'s forced push checks its alias the same way, and a bad credential behind a bad alias
    // adds no warning, since nothing is read.
    let mut s = env.scenario();
    s.command = Cmd::Sync;
    s.alias = "a b".to_string();
    s.credential = Slot::Absent;
    let r = env.run(&s);
    assert!(!r.pushed());
    assert_eq!(r.printed, vec![W_COULD_NOT_SEND.to_string()]);
}

#[test]
fn the_last_stdout_line_counts_as_a_line_even_with_no_lf_after_it() {
    // By ruling (architect, 2026-11-19, for E0.06-T01-S04): a hash line that ends the output is
    // still read, and a line cut short is not 64 hex digits, so it is no hash line.
    let env = Env::new();
    for stdout in [
        HASH_LOWER.to_string(),
        HASH_UPPER.to_string(),
        format!("{HASH_LOWER}  -"),
        format!("Welcome to the box\n{HASH_LOWER}"),
        format!("Welcome to the box\r\n{HASH_LOWER}\t-"),
    ] {
        let s = scenario_with(&env, Mode::Person, None, answer(0, &stdout));
        let r = env.run(&s);
        assert_eq!(
            r.printed,
            vec![SENDING.to_string(), W_NO_ZONE.to_string()],
            "{stdout:?}"
        );
        assert_eq!(
            r.stamp_after,
            Some(stamp_bytes(STAMP_EMPTY_ZONE)),
            "{stdout:?}"
        );
    }
    // With a zone sent, the zone line may be the one that ends the output.
    let s = scenario_with(
        &env,
        Mode::Person,
        Some(ZONE),
        answer(0, &format!("{HASH_LOWER}  -\nzone {ZONE}")),
    );
    let r = env.run(&s);
    assert_eq!(r.printed, vec![SENDING.to_string()]);
    assert_eq!(r.stamp_after, Some(stamp_bytes(STAMP_LA)));

    // A hash line cut short, as by the 10-second bound, confirms nothing.
    for mode in MODES {
        for cut in [1, 32, 63] {
            let stdout = format!("Welcome to the box\n{}", &HASH_LOWER[..cut]);
            let mut s = scenario_with(&env, mode, Some(ZONE), answer(0, &stdout));
            s.stored_stamp = Slot::Bytes(stamp_bytes(STAMP_LONDON));
            let r = env.run(&s);
            assert_eq!(
                r.printed,
                expected_printed(mode, &[Item::Sending, Item::Warn(W_NOT_CONFIRMED)]),
                "{cut} {mode:?}"
            );
            assert_sent_and_unrecorded(&r, Some(&stamp_bytes(STAMP_LONDON)));
        }
    }
    // The hash confirmed, and the zone line cut short: the zone is not confirmed.
    let cut_zone = format!("{HASH_LOWER}  -\nzone {}", &ZONE[..ZONE.len() - 1]);
    let s = scenario_with(&env, Mode::Person, Some(ZONE), answer(0, &cut_zone));
    let r = env.run(&s);
    assert_eq!(r.printed, vec![SENDING.to_string(), w_zone_not_taken(ZONE)]);
    assert!(r.stamp_after.is_none());
}

#[test]
fn a_valid_alias_is_not_refused() {
    let env = Env::new();
    for alias in ["agentspace", "agentspace-proj-0a1b2c3d", "a.b_c-d", "A1"] {
        let mut s = env.scenario();
        s.alias = alias.to_string();
        let r = env.run(&s);
        assert_eq!(r.ssh_calls.len(), 1, "{alias:?}");
    }
}

// ---- Every warning in both modes -----------------------------------------------------------------

#[test]
fn every_warning_reaches_say_as_the_persons_text_and_the_caller_form_is_the_rewrite() {
    let env = Env::new();
    let mut s = scenario_with(&env, Mode::Caller, Some(ZONE), answer(1, ""));
    let r = env.run(&s);
    // Say receives the person's text, `The instance` and all: the rewrite is Say's own.
    assert!(
        r.events.contains(&Ev::Warning(W_NOT_CONFIRMED.to_string())),
        "{:?}",
        r.events
    );
    assert_eq!(
        r.printed,
        vec![
            "agentspace: The host did not confirm the localswim token; the board will not work on the box this session."
                .to_string()
        ]
    );
    s.ssh = answer(0, &format!("{HASH_LOWER}  -\nzone-failed\n"));
    let r = env.run(&s);
    assert!(r.events.contains(&Ev::Warning(w_zone_not_taken(ZONE))));
    assert_eq!(
        r.printed,
        vec![format!(
            "agentspace: The host did not take the time zone {ZONE}; times on the box stay in its own zone."
        )]
    );
}

#[test]
fn every_warning_starts_with_the_agentspace_prefix_and_is_a_fixed_text() {
    let env = Env::new();
    let runs = vec![
        {
            let mut s = env.scenario();
            s.credential = Slot::Absent;
            s
        },
        scenario_with(&env, Mode::Person, None, answer(1, "")),
        scenario_with(&env, Mode::Person, Some(ZONE), answer(0, "x\n")),
        scenario_with(&env, Mode::Person, Some(ZONE), SshBehavior::TimedOut),
        scenario_with(&env, Mode::Person, Some(ZONE), SshBehavior::CouldNotStart),
    ];
    for s in runs {
        let r = env.run(&s);
        for e in &r.events {
            if let Ev::Warning(w) = e {
                assert!(w.starts_with("agentspace: "), "{w:?}");
                assert!(w.ends_with('.'), "{w:?}");
                assert!(!w.contains('\n'), "{w:?}");
            }
        }
    }
}

#[test]
fn a_push_never_prints_a_detail_line_or_anything_else_on_its_own() {
    // Only Say's three calls exist; the push uses progress and warning, never detail with the
    // content, so a person's lines are exactly the ones the spec lists.
    let env = Env::new();
    let r = env.run(&env.scenario());
    assert!(
        r.events
            .iter()
            .all(|e| !matches!(e, Ev::Detail(d) if d.contains("LOCALSWIM")))
    );
}

#[test]
fn the_stamp_file_alone_is_written_and_no_temporary_file_is_left() {
    let env = Env::new();
    let r = env.run(&env.scenario());
    let names: Vec<&str> = r.home_files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        {
            let mut n = names.clone();
            n.sort();
            n
        },
        vec![".agentspace/", ".agentspace/localswim-pushed.json"],
        "{names:?}"
    );
}

#[test]
fn the_stamps_folder_is_created_when_it_is_missing_and_kept_when_it_is_there() {
    let env = Env::new();
    // Missing: the standard run starts with an empty home.
    let r = env.run(&env.scenario());
    assert_eq!(r.stamp_after, Some(stamp_bytes(STAMP_LA)));
    // Present, with an unrelated file in it, which is left alone.
    let mut s = env.scenario();
    s.keep_home = true;
    std::fs::write(env.stamp_dir().join("other.json"), b"{}").unwrap();
    s.stored_stamp = Slot::Bytes(stamp_bytes(STAMP_LONDON));
    let r = env.run(&s);
    assert_eq!(r.stamp_after, Some(stamp_bytes(STAMP_LA)));
    assert!(
        r.home_files
            .iter()
            .any(|(n, b)| n == ".agentspace/other.json" && b == b"{}")
    );
}
