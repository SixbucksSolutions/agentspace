//! Every usage error of the command line, and the order the checks run (docs/operator.md,
//! Command line). A command line that passes every check reaches the command, which is a stub
//! in v0.06-T01-S01; `reaches_the_stub` below says so for each accepted line, and is the one
//! assertion that T03's replacement of the stubs changes.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;

/// An accepted command line gets past every usage check and the `-Alias` checks: it reaches the
/// command, whatever the command then does. Today that is the stub's one line.
fn reaches_the_stub(line: &str, command: &str) {
    assert_reaches_command(&words(line), command);
}

// ---- The messages, one by one ----

#[test]
fn no_command_given() {
    assert_failure::<&str>(&[], &no_command());
    assert_line_fails("-Quiet", &no_command());
    assert_line_fails("-GiB 3", &no_command());
    assert_line_fails("-Alias devbox -Quiet", &no_command());
}

#[test]
fn it_never_prompts_for_a_missing_command() {
    // Stdin is null in the harness; the run ends at once with the message, so nothing waited.
    let r = run::<&str>(&[]);
    assert_eq!(r.code, Some(1));
    assert_eq!(r.out(), "");
}

#[test]
fn an_unknown_command_is_named_as_given() {
    assert_line_fails("bogus", &unknown_command("bogus"));
    assert_line_fails("Bogus", &unknown_command("Bogus"));
    assert_failure(&[""], &unknown_command(""));
    assert_failure(&["up "], &unknown_command("up "));
    assert_failure(&[" up"], &unknown_command(" up"));
}

#[test]
fn commands_are_whole_never_abbreviations() {
    for abbreviation in [
        "st",
        "stat",
        "statu",
        "d",
        "dep",
        "u",
        "s",
        "gro",
        "do",
        "c",
        "ups",
        "statuss",
        "growth",
        "ca1",
        "b",
        "boot",
        "bootstra",
        "bootstrapp",
        "p",
        "prov",
        "provisio",
        "provisions",
    ] {
        assert_failure(&[abbreviation], &unknown_command(abbreviation));
    }
}

#[test]
fn a_second_positional_argument_is_unexpected() {
    assert_line_fails("up status", &unexpected("status"));
    assert_line_fails("status up", &unexpected("up"));
    assert_line_fails("grow bogus", &unexpected("bogus"));
    assert_line_fails("ca extra more", &unexpected("extra"));
    assert_line_fails("up UP", &unexpected("UP"));
}

#[test]
fn an_unknown_parameter_starts_with_a_dash() {
    assert_line_fails("-x", &unknown_parameter("-x"));
    assert_line_fails("up -x", &unknown_parameter("-x"));
    assert_line_fails("up --verbosely", &unknown_parameter("--verbosely"));
    assert_line_fails("up -", &unknown_parameter("-"));
    assert_line_fails("up -5", &unknown_parameter("-5"));
    assert_line_fails("status -Quietly", &unknown_parameter("-Quietly"));
    assert_line_fails("status -GiBx 4", &unknown_parameter("-GiBx"));
    assert_line_fails("up -Aliases x", &unknown_parameter("-Aliases"));
}

#[test]
fn a_parameter_given_twice_names_the_parameter_as_the_page_spells_it() {
    assert_line_fails("status -Quiet -Quiet", &given_twice("-Quiet"));
    assert_line_fails("status -quiet -QUIET", &given_twice("-Quiet"));
    assert_line_fails("grow -GiB 1 -GiB 2", &given_twice("-GiB"));
    assert_line_fails("grow -gib 1 -GIB 2", &given_twice("-GiB"));
    assert_line_fails("up -Alias a -Alias b", &given_twice("-Alias"));
    // At each argument a repeated name is found before a missing value (architect's ruling).
    assert_line_fails("status -GiB 1 -GiB", &given_twice("-GiB"));
    assert_line_fails("up -Alias a -alias", &given_twice("-Alias"));
    assert_line_fails("up -Alias a -Alias bad!", &given_twice("-Alias"));
    // And before the repeated name's own value is checked.
    assert_line_fails("status -GiB 1 -GiB abc", &given_twice("-GiB"));
    // The first -GiB's own value is checked as soon as it is read.
    assert_line_fails("status -GiB abc -GiB 1", &not_a_number("abc"));
    assert_line_fails("up -alias a -ALIAS a", &given_twice("-Alias"));
    // Before or after the command, and with the command between.
    assert_line_fails("-Quiet up -Quiet", &given_twice("-Quiet"));
}

#[test]
fn a_parameter_with_no_value_as_the_last_argument() {
    assert_line_fails("grow -GiB", &needs_value("-GiB"));
    assert_line_fails("up -Alias", &needs_value("-Alias"));
    assert_line_fails("-gib", &needs_value("-GiB"));
    assert_line_fails("up -ALIAS", &needs_value("-Alias"));
    assert_line_fails("up -Quiet -Alias", &needs_value("-Alias"));
    assert_line_fails("-Quiet -GiB", &needs_value("-GiB"));
}

#[test]
fn a_bad_gib_value_names_the_value_as_given() {
    for bad in [
        "abc",
        "12abc",
        "+",
        "-",
        "--1",
        "+-1",
        "2147483648",
        "-2147483649",
        "99999999999999999999",
        "1_0",
        "1,0",
        "\u{0661}\u{0662}",
    ] {
        assert_failure(&["grow", "-GiB", bad], &not_a_number(bad));
        assert_failure(&["status", "-gib", bad], &not_a_number(bad));
    }
}

// ---- -GiB's form and range ----

#[test]
fn gib_takes_a_decimal_integer_in_the_32_bit_range() {
    for good in [
        "0",
        "1",
        "2",
        "10",
        "-1",
        "+5",
        "-0",
        "+0",
        "007",
        "2147483647",
        "-2147483648",
        "+2147483647",
        "0000000000000000000000000001",
    ] {
        reaches_the_stub(&format!("grow -GiB {good}"), "grow");
    }
}

#[test]
fn every_command_checks_gib_s_form() {
    // `status -GiB abc` is a usage error, and every command but grow then ignores the value.
    for command in COMMANDS {
        assert_line_fails(&format!("{command} -GiB abc"), &not_a_number("abc"));
        let line = format!("{} -GiB 7", command_args(command).join(" "));
        reaches_the_stub(&line, command);
    }
}

#[test]
fn gib_s_range_is_not_checked_further() {
    // 0 and below reach AWS, which refuses them (grow, step 2); the operator MUST NOT.
    reaches_the_stub("grow -GiB 0", "grow");
    reaches_the_stub("grow -GiB -2147483648", "grow");
    reaches_the_stub("status -GiB -100", "status");
}

// ---- A value is taken whatever it looks like ----

#[test]
fn the_argument_after_gib_is_its_value_whatever_it_looks_like() {
    reaches_the_stub("grow -GiB -1", "grow");
    reaches_the_stub("grow -gib +1", "grow");
    // A parameter's name, a command, a flag: each is the value, and so a bad number.
    assert_line_fails("grow -GiB -Quiet", &not_a_number("-Quiet"));
    assert_line_fails("grow -GiB -x", &not_a_number("-x"));
    assert_line_fails("grow -GiB -Alias", &not_a_number("-Alias"));
    assert_line_fails("grow -GiB status", &not_a_number("status"));
    assert_line_fails("-GiB grow", &not_a_number("grow"));
}

#[test]
fn the_argument_after_alias_is_its_value_whatever_it_looks_like() {
    // `-Alias -Fx` gives -Alias the value -Fx (operator.md): an alias refusal, not an unknown
    // parameter, and -Quiet as a value is no switch.
    assert_line_fails("up -Alias -Fx", ALIAS_INVALID);
    assert_line_fails("up -Alias -Quiet", ALIAS_INVALID);
    // -GiB is the value here, so the 4 after it is a second positional argument.
    assert_line_fails("up -Alias -GiB 4", &unexpected("4"));
    assert_line_fails("up -Alias -x", ALIAS_INVALID);
    // A command's name as the value is a valid alias, and is no command.
    reaches_the_stub("up -Alias status", "up");
    reaches_the_stub("up -Alias up", "up");
    reaches_the_stub("up -Alias grow", "up");
}

#[test]
fn a_value_never_names_the_command() {
    assert_line_fails("-Alias up", &no_command());
    assert_line_fails("-GiB 5 -Alias status", &no_command());
    assert_line_fails("-GiB -1", &no_command());
    // And a command after it is the command: here status, which -Alias does not apply to.
    assert_line_fails("-Alias up status", ALIAS_APPLIES);
    // `status` is the alias here, and up the command.
    reaches_the_stub("-Alias status up", "up");
}

#[test]
fn a_value_that_is_a_quiet_switch_does_not_count_as_a_second_quiet() {
    // `-Alias -quiet` gives -Alias the value -quiet, so -Quiet is given once and the alias is
    // refused.
    assert_line_fails("-Quiet up -Alias -quiet", ALIAS_INVALID);
}

// ---- The order the checks run ----

#[test]
fn checks_run_left_to_right_and_the_first_failure_ends_the_command() {
    // Each pair is the same two problems in both orders; the one on the left wins.
    assert_line_fails("bogus -x", &unknown_command("bogus"));
    assert_line_fails("-x bogus", &unknown_parameter("-x"));
    assert_line_fails("bogus extra", &unknown_command("bogus"));
    assert_line_fails("status bogus -x", &unexpected("bogus"));
    assert_line_fails("status -x bogus", &unknown_parameter("-x"));
    assert_line_fails("-x -GiB abc", &unknown_parameter("-x"));
    assert_line_fails("-GiB abc -x", &not_a_number("abc"));
    assert_line_fails("-Quiet -Quiet -x", &given_twice("-Quiet"));
    assert_line_fails("-x -Quiet -Quiet", &unknown_parameter("-x"));
    assert_line_fails("bogus -GiB abc", &unknown_command("bogus"));
    assert_line_fails("-GiB abc bogus", &not_a_number("abc"));
    assert_line_fails("bogus -Quiet -Quiet", &unknown_command("bogus"));
    assert_line_fails("status extra -GiB abc", &unexpected("extra"));
    assert_line_fails("-GiB abc extra status", &not_a_number("abc"));
    assert_line_fails("status -Quiet -Quiet -GiB abc", &given_twice("-Quiet"));
    assert_line_fails("status -GiB abc -Quiet -Quiet", &not_a_number("abc"));
}

#[test]
fn a_missing_command_is_found_only_once_the_whole_line_is_read() {
    assert_line_fails("-GiB abc", &not_a_number("abc"));
    assert_line_fails("-x", &unknown_parameter("-x"));
    assert_line_fails("-Quiet -Quiet", &given_twice("-Quiet"));
    assert_line_fails("-GiB 3 -Alias", &needs_value("-Alias"));
    assert_line_fails("-Quiet -GiB", &needs_value("-GiB"));
    // With nothing else wrong, the missing command is what is reported.
    assert_line_fails("-GiB 3 -Alias devbox -Quiet", &no_command());
}

#[test]
fn usage_errors_come_before_the_alias_checks() {
    // operator.md's own example.
    assert_line_fails("status -GiB abc -Alias bad!", &not_a_number("abc"));
    assert_line_fails("up -Alias bad! -GiB abc", &not_a_number("abc"));
    assert_line_fails("up -Alias bad! -x", &unknown_parameter("-x"));
    assert_line_fails("up -Alias bad! extra", &unexpected("extra"));
    assert_line_fails("bogus -Alias bad!", &unknown_command("bogus"));
    assert_line_fails("status bogus -Alias x", &unexpected("bogus"));
    assert_line_fails("-Alias bad!", &no_command());
    assert_line_fails("-Alias x -Quiet", &no_command());
    assert_line_fails("up -Alias a -Alias bad!", &given_twice("-Alias"));
}

#[test]
fn the_two_alias_checks_run_in_their_order() {
    // The first check, "applies to up alone", wins over the second, the pattern.
    assert_line_fails("status -Alias bad!", ALIAS_APPLIES);
    assert_line_fails("grow -Alias -Fx", ALIAS_APPLIES);
    assert_failure(&["deploy", "-Alias", ""], ALIAS_APPLIES);
    // Then the pattern, for up.
    assert_line_fails("up -Alias bad!", ALIAS_INVALID);
}

#[test]
fn help_is_checked_before_every_usage_error() {
    // The full treatment is in help.rs; this is the order's first link.
    let r = run(&["bogus", "-x", "-GiB", "abc", "-Quiet", "-Quiet", "-h"]);
    assert_eq!(r.code, Some(0));
    assert_eq!(r.err(), "");
    assert_eq!(r.out(), help_text("top"));
}

// ---- Case ----

#[test]
fn commands_are_matched_without_regard_to_case() {
    for command in COMMANDS {
        let upper = command.to_uppercase();
        let mixed: String = command
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i % 2 == 0 {
                    c.to_ascii_uppercase()
                } else {
                    c
                }
            })
            .collect();
        reaches_the_stub(&command_args(&upper).join(" "), command);
        reaches_the_stub(&command_args(&mixed).join(" "), command);
    }
}

#[test]
fn parameter_names_are_matched_without_regard_to_case() {
    reaches_the_stub("grow -gib 4", "grow");
    reaches_the_stub("grow -GIB 4", "grow");
    reaches_the_stub("grow -gIb 4", "grow");
    reaches_the_stub("up -alias devbox", "up");
    reaches_the_stub("up -ALIAS devbox", "up");
    reaches_the_stub("up -quiet", "up");
    reaches_the_stub("up -QUIET", "up");
    reaches_the_stub("-quiet up -alias devbox", "up");
    // The messages name the parameter as the page spells it, whatever case was given.
    assert_line_fails("status -gib abc", &not_a_number("abc"));
    assert_line_fails("status -gib", &needs_value("-GiB"));
    assert_line_fails("status -alias", &needs_value("-Alias"));
    assert_line_fails("status -alias x", ALIAS_APPLIES);
}

#[test]
fn parameters_and_the_command_may_stand_in_any_order() {
    reaches_the_stub("-Quiet grow -GiB 3", "grow");
    reaches_the_stub("-GiB 3 grow -Quiet", "grow");
    reaches_the_stub("-Alias devbox up", "up");
    reaches_the_stub("-Quiet -Alias devbox -GiB 4 up", "up");
    reaches_the_stub("up -Quiet -GiB 4 -Alias devbox", "up");
}

// ---- Control characters are written `?` ----

#[test]
fn a_control_character_in_the_argument_is_written_as_a_question_mark() {
    for (given, shown) in [
        ("a\nb", "a?b"),
        ("a\tb", "a?b"),
        ("a\rb", "a?b"),
        ("a\x1bb", "a?b"),
        ("a\x01b", "a?b"),
        ("\n", "?"),
        ("x\n\n\ny", "x???y"),
        ("a\r\nb", "a??b"),
    ] {
        assert_failure(&[given], &unknown_command(shown)); // an unknown command
        assert_failure(&["status", given], &unexpected(shown)); // an unexpected argument
        let dashed = format!("-{given}");
        let dashed_shown = format!("-{shown}");
        assert_failure(&["status", &dashed], &unknown_parameter(&dashed_shown)); // an unknown parameter
        assert_failure(&["grow", "-GiB", given], &not_a_number(shown)); // a bad -GiB value
    }
}

#[test]
fn a_message_with_a_control_character_in_it_is_still_one_line() {
    let r = run(&["a\nb\r\nc"]);
    assert_eq!(r.code, Some(1));
    let err = r.err();
    assert_eq!(err.matches('\n').count(), 1, "{err:?}");
    assert!(err.ends_with("\n"));
    assert!(!err.contains('\r'));
}

#[test]
fn a_control_character_is_one_of_unicode_category_cc() {
    // Category Cc: U+0000 to U+001F, U+007F and U+0080 to U+009F (the dev lead's reading,
    // confirmed by the architect, 2026-11-19). U+0000 cannot be passed in an argument.
    for given in ["a\u{7f}b", "a\u{80}b", "a\u{85}b", "a\u{9f}b"] {
        assert_failure(&[given], &unknown_command("a?b"));
    }
    // U+00A0 (Zs) and U+2028 (Zl) are not Cc, and are kept.
    for given in ["a\u{a0}b", "a\u{2028}b"] {
        assert_failure(&[given], &unknown_command(given));
    }
}

#[test]
fn text_that_is_no_control_character_is_kept() {
    assert_failure(&["caf\u{e9}"], &unknown_command("caf\u{e9}"));
    assert_failure(&["a b"], &unknown_command("a b"));
    assert_failure(&["grow", "-GiB", "\u{4e8c}"], &not_a_number("\u{4e8c}"));
    assert_failure(&["status", "'quoted'"], &unexpected("'quoted'"));
}

#[cfg(unix)]
#[test]
fn an_argument_that_is_not_valid_text_never_panics() {
    use std::os::unix::ffi::OsStrExt;
    let bad = std::ffi::OsStr::from_bytes(b"up\xff\xfe");
    let r = run(&[bad]);
    assert_eq!(r.code, Some(1));
    assert_eq!(r.out(), "");
    let err = r.err();
    assert!(err.starts_with("agentspace: "), "{err:?}");
    assert_eq!(err.matches('\n').count(), 1, "{err:?}");
    assert!(!err.contains("panicked"), "{err:?}");
}

// ---- Only the usage-error text is printed, one line, no runtime text ----

#[test]
fn every_failure_is_one_agentspace_line_and_nothing_else() {
    let lines = [
        "",
        "-Quiet",
        "bogus",
        "up status",
        "-x",
        "up -x",
        "-Quiet -Quiet",
        "grow -GiB",
        "grow -GiB abc",
        "status -Alias x",
        "up -Alias bad!",
        "up -Alias -Fx",
        "-Alias up",
        "grow -GiB 99999999999",
    ];
    for line in lines {
        let r = run(&words(line));
        assert_eq!(r.code, Some(1), "{line:?}");
        assert_eq!(r.out(), "", "{line:?}");
        let err = r.err();
        assert!(err.starts_with("agentspace: "), "{line:?}: {err:?}");
        assert_eq!(err.matches('\n').count(), 1, "{line:?}: {err:?}");
        assert!(err.ends_with('\n'), "{line:?}: {err:?}");
        for runtime in [
            "panicked",
            "RUST_BACKTRACE",
            "Error:",
            "error:",
            "Exception",
            "stack backtrace",
            "thread '",
        ] {
            assert!(!err.contains(runtime), "{line:?}: {err:?}");
        }
    }
}
