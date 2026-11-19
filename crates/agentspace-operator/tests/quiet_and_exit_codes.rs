//! `-Quiet`, the streams and the exit codes of the command line (docs/operator.md, Streams, Exit
//! codes, Messages).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;

/// A corpus of command lines that covers help, every kind of failure and every accepted shape.
fn corpus() -> Vec<Vec<&'static str>> {
    vec![
        vec![],
        vec!["-h"],
        vec!["--help"],
        vec!["up", "-h"],
        vec!["bogus"],
        vec!["bogus", "-h"],
        vec!["up", "status"],
        vec!["-x"],
        vec!["-Quiet", "-Quiet"],
        vec!["-GiB"],
        vec!["-GiB", "abc", "status"],
        vec!["status", "-GiB", "abc"],
        vec!["status", "-Alias", "x"],
        vec!["up", "-Alias", "a b"],
        vec!["up", "-Alias", ""],
        vec!["deploy"],
        vec!["up"],
        vec!["sync"],
        vec!["ca"],
        vec!["down"],
        vec!["status"],
        vec!["grow", "-GiB", "-3"],
        vec!["-Quiet", "status"],
        vec!["-Quiet", "bogus"],
        vec!["-Quiet", "-h"],
    ]
}

#[test]
fn the_exit_code_is_zero_or_one_and_nothing_else() {
    for args in corpus() {
        let r = run(&args);
        assert!(
            matches!(r.code, Some(0) | Some(1)),
            "{args:?}: {:?}",
            r.code
        );
    }
}

#[test]
fn zero_is_help_and_only_help_in_this_story() {
    for args in corpus() {
        let r = run(&args);
        let asks_help = args
            .iter()
            .any(|a| a.eq_ignore_ascii_case("-h") || a.eq_ignore_ascii_case("--help"));
        assert_eq!(r.code == Some(0), asks_help, "{args:?}");
    }
}

#[test]
fn a_failure_writes_nothing_to_stdout() {
    for args in corpus() {
        let r = run(&args);
        if r.code == Some(1) {
            assert_eq!(r.out(), "", "{args:?}");
        }
    }
}

#[test]
fn a_failure_prints_exactly_one_agentspace_line_and_help_prints_none() {
    for args in corpus() {
        let r = run(&args);
        let err = r.err();
        match r.code {
            Some(1) => {
                assert!(err.starts_with("agentspace: "), "{args:?}: {err:?}");
                assert_eq!(err.lines().count(), 1, "{args:?}: {err:?}");
                assert!(
                    err.ends_with('\n') && !err.contains('\r'),
                    "{args:?}: {err:?}"
                );
            }
            _ => assert_eq!(err, "", "{args:?}"),
        }
    }
}

#[test]
fn a_failure_never_shows_a_runtimes_error_record() {
    for args in corpus() {
        let r = run(&args);
        let err = r.err();
        for text in [
            "panicked",
            "stack backtrace",
            "RUST_BACKTRACE",
            "thread 'main'",
            "Error {",
            "Os {",
            "kind:",
        ] {
            assert!(!err.contains(text), "{args:?}: {err:?}");
        }
    }
}

#[test]
fn every_message_starts_with_agentspace_and_a_capital_after_it() {
    // Messages: a failure starts `agentspace: ` then the text the page gives, which starts with a
    // capital or a parameter's name (`-GiB`, `-Alias`, `-Quiet`).
    for args in corpus() {
        let r = run(&args);
        if r.code == Some(1) {
            let rest = r.err().strip_prefix("agentspace: ").unwrap().to_string();
            let first = rest.chars().next().unwrap();
            assert!(
                first.is_ascii_uppercase() || first == '-' || first.is_ascii_lowercase(),
                "{args:?}: {rest:?}"
            );
        }
    }
}

#[test]
fn quiet_never_suppresses_a_failure() {
    for line in [
        "-Quiet",
        "-Quiet bogus",
        "-Quiet up status",
        "-Quiet -x",
        "-Quiet -GiB abc status",
        "-Quiet status -Alias x",
        "up -Quiet -Alias bad!",
        "-Quiet -GiB",
    ] {
        let plain: Vec<&str> = words(line)
            .into_iter()
            .filter(|w| !w.eq_ignore_ascii_case("-Quiet"))
            .collect();
        let quiet = run(&words(line));
        let loud = run(&plain);
        assert_eq!(quiet.code, Some(1), "{line}");
        assert_eq!(quiet.out(), "", "{line}");
        assert_eq!(quiet.err(), loud.err(), "{line}: -Quiet changes no failure");
        assert!(!quiet.err().is_empty(), "{line}");
    }
}

#[test]
fn quiet_does_not_suppress_help() {
    let r = run(&["-Quiet", "up", "-h"]);
    assert_eq!(r.code, Some(0));
    assert_eq!(r.out(), help_text("up"));
    assert_eq!(r.err(), "");
}

#[test]
fn quiet_is_accepted_by_every_command_and_in_any_case() {
    for command in COMMANDS {
        assert_reaches_command(&[command, "-Quiet"], command);
        assert_reaches_command(&["-quiet", command], command);
        assert_reaches_command(&[command, "-QUIET"], command);
    }
}

#[test]
fn quiet_given_twice_is_a_usage_error_even_under_help_free_lines() {
    assert_failure(&["-Quiet", "status", "-Quiet"], &given_twice("-Quiet"));
}

#[test]
fn nothing_is_read_from_stdin_and_nothing_prompts() {
    // The harness gives a null stdin; a prompt would end the run in a different message.
    for args in [vec![], vec!["bogus"], vec!["status"], vec!["up"]] {
        let r = run(&args);
        assert_eq!(r.code, Some(1));
        assert!(r.err().starts_with("agentspace: "), "{:?}", r.err());
    }
}
