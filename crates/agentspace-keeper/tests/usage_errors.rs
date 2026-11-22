//! The keeper's usage errors, in their order (docs/keeper.md, Command line: Usage errors).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every case runs the built
//! binary in a throwaway home. **A usage error MUST write three lines to stderr, log nothing, and
//! exit 2.** The first that fails of the checks 1 to 6 is the one reported.

mod common;

use common::*;

fn expect_usage(b: &Bin, args: &[&str], message: &str) {
    let r = b.run(args);
    assert_eq!(
        r.status,
        Some(2),
        "Exit code for {args:?}; stderr was {:?}",
        r.stderr
    );
    assert_eq!(
        r.stdout, "",
        "A usage error writes nothing to stdout ({args:?})"
    );
    assert_eq!(r.stderr, usage_error(message), "Stderr for {args:?}");
    assert!(!b.log_exists(), "A usage error logs nothing ({args:?})");
    assert!(
        b.state_is_empty(),
        "A usage error leaves the state folder empty ({args:?})"
    );
}

#[test]
fn the_usage_text_is_the_two_lines_of_the_doc() {
    // Pin the constant itself against the doc's text, so a typo in the helper cannot hide a typo
    // in the program.
    assert_eq!(
        usage_error("m."),
        "agentspace-keeper: m.\nUsage: agentspace-keeper [-SshExe <full path>] [-Now <YYYY-MM-DDTHH:MM:SSZ>]\n       agentspace-keeper -Watch [-SshExe <full path>] [-IntervalSeconds <n>] [-MaxPasses <n>] [-BackoffBaseSeconds <n>] [-BackoffMaxSeconds <n>] [-ReleaseGraceSeconds <n>]\n"
    );
    assert_eq!(usage_error("m.").lines().count(), 3);
}

// ---- Check 1: names, repeats and missing values, in one scan ----

#[test]
fn an_unknown_argument_is_named_as_the_caller_spelled_it() {
    let b = Bin::new();
    for raw in [
        "-Bogus", "-bogus", "-BOGUS", "bogus", "--Watch", "-Watch=1", "-Now:x",
    ] {
        expect_usage(&b, &[raw], &format!("Unknown argument '{raw}'."));
    }
}

#[test]
fn the_help_spellings_are_unknown_arguments() {
    let b = Bin::new();
    for raw in ["-h", "--help", "-?", "-help", "/?"] {
        expect_usage(&b, &[raw], &format!("Unknown argument '{raw}'."));
    }
}

#[test]
fn there_are_no_positional_arguments() {
    let b = Bin::new();
    let ssh = b.ssh.to_string_lossy().into_owned();
    expect_usage(&b, &["extra"], "Unknown argument 'extra'.");
    // A value is skipped, so the second word is the first thing read as a name.
    expect_usage(&b, &["-SshExe", &ssh, "extra"], "Unknown argument 'extra'.");
}

#[test]
fn names_are_matched_without_regard_to_case_and_reported_as_the_page_spells_them() {
    let b = Bin::new();
    expect_usage(&b, &["-WATCH", "-watch"], "-Watch is given twice.");
    expect_usage(
        &b,
        &["-sshexe", "a", "-SSHEXE", "b"],
        "-SshExe is given twice.",
    );
    expect_usage(&b, &["-now", "a", "-NOW", "b"], "-Now is given twice.");
    expect_usage(&b, &["-sshexe"], "-SshExe needs a value.");
    expect_usage(
        &b,
        &["-watch", "-intervalseconds", "0"],
        "-IntervalSeconds '0' is not a whole number from 1 to 86400.",
    );
    expect_usage(
        &b,
        &["-intervalseconds", "5"],
        "-IntervalSeconds is for -Watch.",
    );
}

#[test]
fn a_name_given_twice_is_refused_watch_included() {
    let b = Bin::new();
    expect_usage(&b, &["-Watch", "-Watch"], "-Watch is given twice.");
    expect_usage(
        &b,
        &["-SshExe", "a", "-SshExe", "b"],
        "-SshExe is given twice.",
    );
    expect_usage(&b, &["-Now", NOW, "-Now", NOW], "-Now is given twice.");
    for name in [
        "-IntervalSeconds",
        "-MaxPasses",
        "-BackoffBaseSeconds",
        "-BackoffMaxSeconds",
        "-ReleaseGraceSeconds",
        "-FastFailureSeconds",
        "-GuardIntervalSeconds",
    ] {
        expect_usage(
            &b,
            &["-Watch", name, "5", name, "5"],
            &format!("{name} is given twice."),
        );
    }
}

#[test]
fn a_name_with_no_value_is_refused() {
    let b = Bin::new();
    expect_usage(&b, &["-SshExe"], "-SshExe needs a value.");
    expect_usage(&b, &["-Now"], "-Now needs a value.");
    for name in [
        "-IntervalSeconds",
        "-MaxPasses",
        "-BackoffBaseSeconds",
        "-BackoffMaxSeconds",
        "-ReleaseGraceSeconds",
        "-FastFailureSeconds",
        "-GuardIntervalSeconds",
    ] {
        expect_usage(&b, &["-Watch", name], &format!("{name} needs a value."));
    }
}

#[test]
fn an_empty_blank_or_dash_led_next_argument_is_no_value() {
    let b = Bin::new();
    expect_usage(&b, &["-SshExe", ""], "-SshExe needs a value.");
    expect_usage(&b, &["-SshExe", "   "], "-SshExe needs a value.");
    expect_usage(&b, &["-SshExe", "\t"], "-SshExe needs a value.");
    expect_usage(&b, &["-SshExe", "-Now"], "-SshExe needs a value.");
    expect_usage(&b, &["-Now", "-Watch"], "-Now needs a value.");
    expect_usage(
        &b,
        &["-Watch", "-MaxPasses", "-5"],
        "-MaxPasses needs a value.",
    );
    expect_usage(&b, &["-Now", "-"], "-Now needs a value.");
}

#[test]
fn at_each_argument_the_name_is_checked_then_the_repeat_then_the_value() {
    let b = Bin::new();
    // Unknown comes before everything, even with no value after it.
    expect_usage(&b, &["-Bogus"], "Unknown argument '-Bogus'.");
    // A repeat comes before a missing value.
    expect_usage(&b, &["-Now", NOW, "-Now"], "-Now is given twice.");
    expect_usage(&b, &["-SshExe", "a", "-SshExe"], "-SshExe is given twice.");
    // -Watch has no value to miss.
    expect_usage(
        &b,
        &["-Watch", "-Watch", "-Bogus"],
        "-Watch is given twice.",
    );
}

#[test]
fn the_scan_ends_at_the_first_failure_and_check_one_ends_before_check_two() {
    let b = Bin::new();
    expect_usage(
        &b,
        &["-IntervalSeconds", "abc", "-Bogus"],
        "Unknown argument '-Bogus'.",
    );
    expect_usage(
        &b,
        &["-Watch", "-MaxPasses", "abc", "-Bogus"],
        "Unknown argument '-Bogus'.",
    );
    expect_usage(
        &b,
        &["-Bogus", "-Watch", "-Watch"],
        "Unknown argument '-Bogus'.",
    );
    expect_usage(&b, &["-Now", "-Bogus"], "-Now needs a value.");
}

#[test]
fn a_value_is_skipped_and_never_read_as_a_name() {
    let b = Bin::new();
    // "Bogus" is the value of -Now, so the first thing read as a name afterwards is -Other.
    expect_usage(
        &b,
        &["-Now", "Bogus", "-Other"],
        "Unknown argument '-Other'.",
    );
    // And a value that looks like a known name's word is still just a value: this is a bad time,
    // reported by check 6, not an unknown or repeated -Watch.
    let ssh = b.ssh.to_string_lossy().into_owned();
    expect_usage(
        &b,
        &["-SshExe", &ssh, "-Now", "Watch"],
        "-Now 'Watch' is not a time in the form YYYY-MM-DDTHH:MM:SSZ.",
    );
}

// ---- Check 2: the numbers ----

const WATCH_ONLY: [&str; 7] = [
    "-IntervalSeconds",
    "-MaxPasses",
    "-BackoffBaseSeconds",
    "-BackoffMaxSeconds",
    "-ReleaseGraceSeconds",
    "-FastFailureSeconds",
    "-GuardIntervalSeconds",
];

#[test]
fn a_watch_only_number_without_watch_is_refused_whatever_its_value() {
    let b = Bin::new();
    for name in WATCH_ONLY {
        expect_usage(&b, &[name, "5"], &format!("{name} is for -Watch."));
        expect_usage(&b, &[name, "abc"], &format!("{name} is for -Watch."));
        expect_usage(&b, &[name, "0"], &format!("{name} is for -Watch."));
    }
}

#[test]
fn without_watch_the_first_watch_only_number_in_the_tables_order_is_reported() {
    let b = Bin::new();
    expect_usage(
        &b,
        &["-MaxPasses", "abc", "-IntervalSeconds", "0"],
        "-IntervalSeconds is for -Watch.",
    );
    expect_usage(
        &b,
        &["-FastFailureSeconds", "1", "-ReleaseGraceSeconds", "1"],
        "-ReleaseGraceSeconds is for -Watch.",
    );
    expect_usage(
        &b,
        &["-BackoffMaxSeconds", "1", "-BackoffBaseSeconds", "1"],
        "-BackoffBaseSeconds is for -Watch.",
    );
}

#[test]
fn with_watch_a_number_that_is_not_a_whole_number_from_1_to_86400_is_refused() {
    let b = Bin::new();
    let bad = [
        "0",
        "86401",
        "abc",
        "1.5",
        "+5",
        "1e3",
        "0x10",
        "1,000",
        " 5",
        "5 ",
        "5 5",
        "1_0",
        "99999999999999999999",
        "00",
        "000000",
        "8640000000000000000000",
    ];
    for name in WATCH_ONLY {
        for value in bad {
            expect_usage(
                &b,
                &["-Watch", name, value],
                &format!("{name} '{value}' is not a whole number from 1 to 86400."),
            );
        }
    }
}

#[test]
fn decimal_digits_only_means_ascii_digits() {
    // "Written in decimal digits only: no sign, no blanks, no separators" (keeper.md, Command
    // line) is read as the ASCII digits 0 to 9; a digit of another script is not one.
    let b = Bin::new();
    expect_usage(
        &b,
        &["-Watch", "-MaxPasses", "\u{663}"],
        "-MaxPasses '\u{663}' is not a whole number from 1 to 86400.",
    );
}

#[test]
fn with_watch_the_numbers_are_checked_in_the_tables_order_not_the_callers() {
    let b = Bin::new();
    expect_usage(
        &b,
        &["-Watch", "-MaxPasses", "abc", "-IntervalSeconds", "0"],
        "-IntervalSeconds '0' is not a whole number from 1 to 86400.",
    );
    expect_usage(
        &b,
        &[
            "-Watch",
            "-FastFailureSeconds",
            "0",
            "-ReleaseGraceSeconds",
            "0",
        ],
        "-ReleaseGraceSeconds '0' is not a whole number from 1 to 86400.",
    );
    expect_usage(
        &b,
        &[
            "-Watch",
            "-ReleaseGraceSeconds",
            "0",
            "-BackoffMaxSeconds",
            "0",
            "-BackoffBaseSeconds",
            "0",
        ],
        "-BackoffBaseSeconds '0' is not a whole number from 1 to 86400.",
    );
}

#[test]
fn a_number_is_checked_before_the_other_checks_of_the_rest() {
    // A bad number (check 2) is reported before -Now with -Watch (3), the backoff order (4), the
    // ssh path (5) and the time (6).
    let b = Bin::new();
    expect_usage(
        &b,
        &[
            "-Watch",
            "-Now",
            "x",
            "-SshExe",
            "ssh.exe",
            "-IntervalSeconds",
            "0",
        ],
        "-IntervalSeconds '0' is not a whole number from 1 to 86400.",
    );
}

#[test]
fn every_good_number_passes_check_two_and_the_ends_and_leading_zeros_are_allowed() {
    // With -Watch and -Now, check 3 is the next to fail, so reaching it shows that check 2 took
    // every number.
    let b = Bin::new();
    expect_usage(
        &b,
        &[
            "-Watch",
            "-IntervalSeconds",
            "0030",
            "-MaxPasses",
            "1",
            "-BackoffBaseSeconds",
            "86400",
            "-BackoffMaxSeconds",
            "86400",
            "-ReleaseGraceSeconds",
            "00001",
            "-FastFailureSeconds",
            "000086400",
            "-GuardIntervalSeconds",
            "00086400",
            "-Now",
            NOW,
        ],
        "-Now is not allowed with -Watch.",
    );
}

// ---- The hidden -GuardIntervalSeconds (keeper.md, Command line; G15) ----

#[test]
fn guard_interval_seconds_without_watch_is_refused_and_starts_no_guard() {
    // G15. Nothing is logged, and no stand-in guard ran, since a usage error ends the program
    // before any pass and before any start.
    let b = Bin::new();
    expect_usage(
        &b,
        &["-GuardIntervalSeconds", "5"],
        "-GuardIntervalSeconds is for -Watch.",
    );
    assert_eq!(guard_runs_in(&b.standin_dir()).len(), 0);
}

#[test]
fn a_guard_interval_of_zero_with_watch_is_refused_and_starts_no_guard() {
    // G15.
    let b = Bin::new();
    expect_usage(
        &b,
        &["-Watch", "-GuardIntervalSeconds", "0"],
        "-GuardIntervalSeconds '0' is not a whole number from 1 to 86400.",
    );
    assert_eq!(guard_runs_in(&b.standin_dir()).len(), 0);
}

#[test]
fn a_guard_interval_above_a_day_is_refused() {
    let b = Bin::new();
    expect_usage(
        &b,
        &["-Watch", "-GuardIntervalSeconds", "86401"],
        "-GuardIntervalSeconds '86401' is not a whole number from 1 to 86400.",
    );
}

#[test]
fn the_guard_interval_is_the_last_number_check_two_takes() {
    // The table's order: -FastFailureSeconds comes before -GuardIntervalSeconds, whatever order
    // the caller wrote them in.
    let b = Bin::new();
    expect_usage(
        &b,
        &[
            "-Watch",
            "-GuardIntervalSeconds",
            "0",
            "-FastFailureSeconds",
            "0",
        ],
        "-FastFailureSeconds '0' is not a whole number from 1 to 86400.",
    );
    // Without -Watch the same order gives the first of them that was given.
    expect_usage(
        &b,
        &["-GuardIntervalSeconds", "1", "-FastFailureSeconds", "1"],
        "-FastFailureSeconds is for -Watch.",
    );
}

#[test]
fn the_guard_interval_is_not_in_the_usage_text() {
    // Hidden, for checks only (keeper.md, Command line).
    assert!(!USAGE.contains("GuardInterval"));
    let b = Bin::new();
    let r = b.run(&["-Bogus"]);
    assert_eq!(r.status, Some(2));
    assert!(!r.stderr.contains("GuardInterval"));
}

// ---- Check 3: -Now with -Watch ----

#[test]
fn now_with_watch_is_refused_whatever_the_time_is() {
    let b = Bin::new();
    expect_usage(
        &b,
        &["-Watch", "-Now", NOW],
        "-Now is not allowed with -Watch.",
    );
    expect_usage(
        &b,
        &["-Now", NOW, "-Watch"],
        "-Now is not allowed with -Watch.",
    );
    // Check 3 comes before check 6, which would call this a bad time.
    expect_usage(
        &b,
        &["-Watch", "-Now", "x"],
        "-Now is not allowed with -Watch.",
    );
}

#[test]
fn now_with_watch_comes_before_the_backoff_order_and_the_ssh_path() {
    let b = Bin::new();
    expect_usage(
        &b,
        &[
            "-Watch",
            "-Now",
            NOW,
            "-BackoffBaseSeconds",
            "10",
            "-BackoffMaxSeconds",
            "5",
            "-SshExe",
            "ssh.exe",
        ],
        "-Now is not allowed with -Watch.",
    );
}

// ---- Check 4: the backoff order ----

#[test]
fn a_backoff_max_less_than_the_base_is_refused() {
    let b = Bin::new();
    let msg = "-BackoffMaxSeconds is less than -BackoffBaseSeconds.";
    expect_usage(
        &b,
        &[
            "-Watch",
            "-BackoffBaseSeconds",
            "20",
            "-BackoffMaxSeconds",
            "10",
        ],
        msg,
    );
    // Defaults count: the base is 30 and the max is 300.
    expect_usage(&b, &["-Watch", "-BackoffMaxSeconds", "29"], msg);
    expect_usage(&b, &["-Watch", "-BackoffBaseSeconds", "301"], msg);
}

#[test]
fn the_backoff_order_comes_before_the_ssh_path_and_an_equal_pair_passes_it() {
    let b = Bin::new();
    expect_usage(
        &b,
        &["-Watch", "-BackoffMaxSeconds", "10", "-SshExe", "ssh.exe"],
        "-BackoffMaxSeconds is less than -BackoffBaseSeconds.",
    );
    // Equal is not less, so check 5 is the next to fail.
    expect_usage(
        &b,
        &[
            "-Watch",
            "-BackoffBaseSeconds",
            "10",
            "-BackoffMaxSeconds",
            "10",
            "-SshExe",
            "ssh.exe",
        ],
        "SshExe 'ssh.exe' is not a full path.",
    );
    expect_usage(
        &b,
        &["-Watch", "-BackoffMaxSeconds", "30", "-SshExe", "ssh.exe"],
        "SshExe 'ssh.exe' is not a full path.",
    );
}

// ---- Check 5: -SshExe ----

#[test]
fn a_relative_ssh_exe_is_not_a_full_path() {
    let b = Bin::new();
    for p in [
        "ssh.exe",
        "bin/ssh",
        "bin\\ssh.exe",
        ".",
        "..",
        "./ssh",
        "ssh",
    ] {
        expect_usage(
            &b,
            &["-SshExe", p],
            &format!("SshExe '{p}' is not a full path."),
        );
    }
}

#[test]
fn a_relative_path_is_not_a_full_path_before_it_is_anything_else() {
    // The order of the table's rows: not a full path, then a quote, then a batch's percent.
    let b = Bin::new();
    expect_usage(
        &b,
        &["-SshExe", "a\"b"],
        "SshExe 'a\"b' is not a full path.",
    );
    expect_usage(
        &b,
        &["-SshExe", "50%/ssh.cmd"],
        "SshExe '50%/ssh.cmd' is not a full path.",
    );
}

#[test]
fn the_default_ssh_exe_is_checked_like_a_value_that_was_given() {
    // SystemRoot is the only source of the default (D35), and with it missing or empty the default
    // is the text after it, which is not a full path.
    // The default is the value of `SystemRoot`, then `\System32\OpenSSH\ssh.exe`, and is checked
    // like a value that was given (keeper.md, Command line), so with no value the message names
    // that text with nothing before it.
    let mut b = Bin::new();
    b.unset("SystemRoot");
    expect_usage(
        &b,
        &[],
        "SshExe '\\System32\\OpenSSH\\ssh.exe' is not a full path.",
    );
    b.set("SystemRoot", "");
    expect_usage(
        &b,
        &[],
        "SshExe '\\System32\\OpenSSH\\ssh.exe' is not a full path.",
    );
}

#[test]
fn a_default_ssh_exe_that_is_a_full_path_passes_check_five() {
    // With SystemRoot a full path the default is one too, so check 6 is the next to fail.
    let b = Bin::new();
    expect_usage(
        &b,
        &["-Now", "x"],
        "-Now 'x' is not a time in the form YYYY-MM-DDTHH:MM:SSZ.",
    );
}

#[test]
fn a_full_ssh_exe_holding_a_quote_is_refused() {
    let b = Bin::new();
    let with_quote = b.full(&["q\"x", "ssh"]);
    let p = with_quote.to_string_lossy().into_owned();
    expect_usage(
        &b,
        &["-SshExe", &p],
        &format!("SshExe '{p}' holds a quote."),
    );
    // A batch's path with a quote is a quote first.
    let batch = b.full(&["q\"x", "50%", "ssh.cmd"]);
    let p = batch.to_string_lossy().into_owned();
    expect_usage(
        &b,
        &["-SshExe", &p],
        &format!("SshExe '{p}' holds a quote."),
    );
}

#[test]
fn a_batch_whose_path_holds_a_percent_sign_is_refused() {
    let b = Bin::new();
    for name in ["ssh.cmd", "ssh.bat", "ssh.CMD", "ssh.Bat"] {
        let path = b.full(&["50%", name]);
        let p = path.to_string_lossy().into_owned();
        expect_usage(
            &b,
            &["-SshExe", &p],
            &format!("SshExe '{p}' is a batch whose path holds a percent sign."),
        );
    }
    // The percent may be in the file's own name.
    let path = b.full(&["bin", "ssh%x.cmd"]);
    let p = path.to_string_lossy().into_owned();
    expect_usage(
        &b,
        &["-SshExe", &p],
        &format!("SshExe '{p}' is a batch whose path holds a percent sign."),
    );
}

#[test]
fn a_percent_sign_in_an_exe_is_no_usage_error() {
    // The percent matters only to a batch, which cmd would expand. An exe's path passes check 5,
    // so a bad time (check 6) is what is reported.
    let b = Bin::new();
    let path = b.full(&["50%", "ssh.exe"]);
    let p = path.to_string_lossy().into_owned();
    expect_usage(
        &b,
        &["-SshExe", &p, "-Now", "x"],
        "-Now 'x' is not a time in the form YYYY-MM-DDTHH:MM:SSZ.",
    );
}

#[test]
fn whether_ssh_exe_exists_is_not_a_usage_error() {
    let b = Bin::new();
    let path = b.full(&["no-such-folder", "ssh.exe"]);
    let p = path.to_string_lossy().into_owned();
    expect_usage(
        &b,
        &["-SshExe", &p, "-Now", "x"],
        "-Now 'x' is not a time in the form YYYY-MM-DDTHH:MM:SSZ.",
    );
}

#[test]
fn the_ssh_path_is_checked_before_the_time() {
    let b = Bin::new();
    expect_usage(
        &b,
        &["-SshExe", "ssh.exe", "-Now", "x"],
        "SshExe 'ssh.exe' is not a full path.",
    );
}

// ---- Check 6: -Now ----

#[test]
fn a_now_that_is_not_exactly_the_form_or_not_a_real_time_is_refused() {
    let b = Bin::new();
    let ssh = b.ssh.to_string_lossy().into_owned();
    let bad = [
        "x",
        "2026-11-19T12:00:00",
        "2026-11-19 12:00:00Z",
        "2026-11-19T12:00:00z",
        "2026-11-19t12:00:00Z",
        "2026-11-19T12:00:00.000Z",
        "2026-11-19T12:00:00+00:00",
        "2026-11-19T12:00Z",
        "2026-11-19T12:00:00ZZ",
        " 2026-11-19T12:00:00Z",
        "2026-11-19T12:00:00Z ",
        "2026-1-19T12:00:00Z",
        "20261119T120000Z",
        "2026-13-01T00:00:00Z",
        "2026-00-10T00:00:00Z",
        "2026-11-31T00:00:00Z",
        "2026-02-30T00:00:00Z",
        "2026-02-29T00:00:00Z",
        "2026-11-19T24:00:00Z",
        "2026-11-19T12:60:00Z",
        "2026-11-19T12:00:60Z",
        "2026-11-00T12:00:00Z",
    ];
    for v in bad {
        expect_usage(
            &b,
            &["-SshExe", &ssh, "-Now", v],
            &format!("-Now '{v}' is not a time in the form YYYY-MM-DDTHH:MM:SSZ."),
        );
    }
}

#[test]
fn every_usage_error_comes_before_the_home_folder_check() {
    // USERPROFILE is checked after the usage checks (keeper.md, Streams and environment).
    let mut b = Bin::new();
    b.set("USERPROFILE", "");
    expect_usage(&b, &["-Bogus"], "Unknown argument '-Bogus'.");
    b.unset("USERPROFILE");
    expect_usage(
        &b,
        &["-Now", "x", "-SshExe", "ssh.exe"],
        "SshExe 'ssh.exe' is not a full path.",
    );
}

// ---- In process: a usage error calls no trait and writes only to stderr ----

#[test]
fn in_process_a_usage_error_calls_no_trait_logs_nothing_and_exits_2() {
    for (args, message) in [
        (vec!["-Bogus"], "Unknown argument '-Bogus'."),
        (
            vec!["-IntervalSeconds", "5"],
            "-IntervalSeconds is for -Watch.",
        ),
        (
            vec!["-Watch", "-Now", NOW],
            "-Now is not allowed with -Watch.",
        ),
        (
            vec!["-SshExe", "ssh.exe"],
            "SshExe 'ssh.exe' is not a full path.",
        ),
    ] {
        let h = Harness::new();
        let r = h.run(&args);
        assert_eq!(r.exit, 2, "{args:?}");
        assert_eq!(r.stdout, "", "{args:?}");
        assert_eq!(r.stderr, usage_error(message), "{args:?}");
        assert!(
            h.events().is_empty(),
            "A trait was called: {:?}",
            h.events()
        );
        assert!(!h.log_path().exists());
    }
}
