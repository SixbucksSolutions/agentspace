//! The seven help blocks, byte for byte, and where help is asked for (lease-api.md, Commands and
//! Command lines, "Which error is reported", rule 1).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. The blocks are read from
//! docs/lease-api.md at test time (D38), so a change to the doc and to the program MUST go together.

mod common;

use common::*;

#[test]
fn the_doc_holds_seven_help_blocks_and_they_are_read_whole() {
    let blocks = help_blocks();
    let names: Vec<&str> = blocks.keys().map(String::as_str).collect();
    let mut want: Vec<&str> = LEVELS.iter().map(|(name, _)| *name).collect();
    want.sort_unstable();
    assert_eq!(names, want, "The levels found in docs/lease-api.md");
    for (name, block) in &blocks {
        assert!(
            block.ends_with('\n'),
            "{name}: every line ends in a line feed"
        );
        assert!(!block.contains('\r'), "{name}: no carriage return");
        assert!(
            !block.contains("\n\n\n"),
            "{name}: the block ends where its fence does"
        );
        assert!(
            block.lines().count() >= 4,
            "{name}: the block is not truncated"
        );
    }
    // The pattern MUST be able to find something (Terry's rule on audits): spot-check known lines.
    assert!(blocks["agentspace"].starts_with("Usage: agentspace <command> [<subcommand>]"));
    assert!(
        blocks["agentspace lease create"].contains("\n  -h, --help            Print this help")
    );
    assert!(
        blocks["agentspace lease list"]
            .ends_with("unsatisfiable.\n  -h, --help        Print this help and exit\n")
    );
}

fn check_help(line: &[&str], level: &str) -> Option<String> {
    let line = args(line);
    let run = run_bin(&line);
    let mut p = help_problems(&run, level);
    let code = in_process(&line);
    if code != 0 {
        p.push(format!("In process: exit {code}, not 0"));
    }
    if p.is_empty() {
        None
    } else {
        Some(format!("{}: {}", show(&line), p.join("; ")))
    }
}

fn assert_all(failures: Vec<Option<String>>) {
    let failures: Vec<String> = failures.into_iter().flatten().collect();
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

#[test]
fn every_level_prints_its_block_for_dash_h_and_dash_dash_help() {
    let mut failures = Vec::new();
    for (level, words) in LEVELS {
        for flag in ["--help", "-h", "-H", "--HELP", "--Help"] {
            let mut line: Vec<&str> = words.to_vec();
            line.push(flag);
            failures.push(check_help(&line, level));
        }
    }
    assert_all(failures);
}

#[test]
fn help_is_found_whatever_case_the_command_and_subcommand_are_in() {
    assert_all(vec![
        check_help(&["Lease", "-h"], "agentspace lease"),
        check_help(&["LEASE", "--help"], "agentspace lease"),
        check_help(&["lease", "CREATE", "--help"], "agentspace lease create"),
        check_help(&["LEASE", "CREATE", "-H"], "agentspace lease create"),
        check_help(&["Lease", "Get", "-h"], "agentspace lease get"),
        check_help(&["lease", "reNew", "--help"], "agentspace lease renew"),
        check_help(&["lease", "RELEASE", "--help"], "agentspace lease release"),
        check_help(&["lease", "List", "--help"], "agentspace lease list"),
    ]);
}

#[test]
fn help_in_a_flags_value_position_is_a_request_for_help() {
    assert_all(vec![
        check_help(
            &["lease", "create", "--ttl", "--help"],
            "agentspace lease create",
        ),
        check_help(
            &["lease", "create", "--ttl", "-H"],
            "agentspace lease create",
        ),
        check_help(
            &["lease", "create", "--ttl", "-h"],
            "agentspace lease create",
        ),
        check_help(
            &["lease", "create", "--project", "--HELP"],
            "agentspace lease create",
        ),
        check_help(
            &["lease", "renew", ID, "--ttl", "-H"],
            "agentspace lease renew",
        ),
        check_help(
            &["lease", "renew", "--ttl", "--help", ID],
            "agentspace lease renew",
        ),
        check_help(
            &["lease", "list", "--state", "ready", "--state", "--HELP"],
            "agentspace lease list",
        ),
        check_help(
            &["lease", "list", "--project", "-h"],
            "agentspace lease list",
        ),
    ]);
}

#[test]
fn help_wins_over_every_other_error_and_nothing_else_is_done() {
    assert_all(vec![
        // A bad value after it is never checked.
        check_help(
            &["lease", "create", "--help", "--ttl", "99d"],
            "agentspace lease create",
        ),
        // An unknown option, and a flag with no value, before it.
        check_help(
            &["lease", "create", "--bogus", "--project", "-h"],
            "agentspace lease create",
        ),
        // A flag given twice before it.
        check_help(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--project",
                "y",
                "--help",
            ],
            "agentspace lease create",
        ),
        // An unexpected second ID before it.
        check_help(&["lease", "get", ID, ID, "--help"], "agentspace lease get"),
        check_help(
            &["lease", "get", ID, "--ttl", "8h", "-h"],
            "agentspace lease get",
        ),
        check_help(&["lease", "get", "--ttl", "--help"], "agentspace lease get"),
        check_help(
            &["lease", "release", ID, "extra", "--help"],
            "agentspace lease release",
        ),
        // A missing project or ID.
        check_help(&["lease", "create", "-h"], "agentspace lease create"),
        check_help(&["lease", "get", "--HELP"], "agentspace lease get"),
        // Unknown options at the lease and top levels.
        check_help(&["Lease", "--bogus", "--Help"], "agentspace lease"),
        check_help(&["--bogus", "--help"], "agentspace"),
        check_help(&["--bogus", "-h"], "agentspace"),
        check_help(&["-", "--help"], "agentspace"),
        check_help(&["--", "--help"], "agentspace"),
        check_help(&["--help=yes", "--help"], "agentspace"),
        check_help(&["--help", "--help=yes"], "agentspace"),
    ]);
}

#[test]
fn help_prints_the_level_reached_so_far() {
    assert_all(vec![
        check_help(&["--help", "lease"], "agentspace"),
        check_help(&["-h", "lease", "create"], "agentspace"),
        check_help(&["lease", "--help", "create"], "agentspace lease"),
        check_help(
            &["lease", "-H", "create", "--ttl", "99d"],
            "agentspace lease",
        ),
        check_help(
            &["lease", "create", "--help", "extra"],
            "agentspace lease create",
        ),
    ]);
}

#[test]
fn help_before_an_unknown_command_or_subcommand_prints_help() {
    assert_all(vec![
        check_help(&["--help", "frob"], "agentspace"),
        check_help(&["-h", "frob"], "agentspace"),
        check_help(&["lease", "--help", "frob"], "agentspace lease"),
        check_help(&["lease", "-h", "frob", "--bogus"], "agentspace lease"),
        check_help(&["--bogus", "--help", "frob"], "agentspace"),
    ]);
}

#[test]
fn help_after_an_unknown_command_or_subcommand_is_never_read() {
    check_usage_cases(vec![
        case(&["frob", "--help"], None, &msg_unknown_command("frob")),
        case(&["frob", "-h"], None, &msg_unknown_command("frob")),
        case(
            &["--bogus", "frob", "--help"],
            None,
            &msg_unknown_command("frob"),
        ),
        case(
            &["lease", "frob", "--help"],
            None,
            &msg_unknown_subcommand("frob"),
        ),
        case(
            &["lease", "frob", "-H"],
            None,
            &msg_unknown_subcommand("frob"),
        ),
        case(
            &["lease", "--bogus", "frob", "--help"],
            None,
            &msg_unknown_subcommand("frob"),
        ),
    ]);
}

#[test]
fn an_equals_form_makes_help_a_value_and_not_a_request_for_help() {
    // The line has no usage error, so it reaches the command, which checks the value `--help` or
    // `-h` as a TTL or a project name and refuses it: `invalid_value`, never help (exit 0).
    for flag in ["--ttl=--help", "--ttl=-h", "--ttl=-H", "--TTL=--HELP"] {
        let line = args(&["lease", "create", "--project", "x", flag]);
        let run = run_bin(&line);
        let p = failure_problems(&run, "invalid_value", Some("ttl"), None);
        assert!(p.is_empty(), "{}: {}", show(&line), p.join("; "));
        assert_eq!(in_process(&line), 2, "{} in process", show(&line));
    }
    let line = args(&["lease", "list", "--project=--help"]);
    let run = run_bin(&line);
    let p = failure_problems(&run, "invalid_value", Some("project"), None);
    assert!(p.is_empty(), "{}: {}", show(&line), p.join("; "));
    assert_eq!(in_process(&line), 2, "{} in process", show(&line));
}

#[test]
fn help_is_not_a_request_when_it_only_looks_like_one() {
    // Anything else that starts with `-` is an unknown option, `--help=yes` and `-help` among them.
    check_usage_cases(vec![
        case(&["--help=yes"], None, &msg_unknown_option_top("--help=yes")),
        case(&["-help"], None, &msg_unknown_option_top("-help")),
        case(&["-hh"], None, &msg_unknown_option_top("-hh")),
        case(&["-h=1"], None, &msg_unknown_option_top("-h=1")),
        case(&["--h"], None, &msg_unknown_option_top("--h")),
        case(
            &["lease", "--help=yes"],
            None,
            &msg_unknown_option_lease("--help=yes"),
        ),
        case(
            &["lease", "create", "--project", "x", "--help=yes"],
            None,
            &msg_unknown_option_sub("--help=yes", "create"),
        ),
    ]);
}

#[test]
fn help_prints_lf_line_endings_only() {
    for (level, words) in LEVELS {
        let mut line: Vec<&str> = words.to_vec();
        line.push("--help");
        let run = run_str(&line);
        assert!(!run.stdout.contains(&b'\r'), "{level}: no carriage return");
        assert!(run.stdout.ends_with(b"\n"), "{level}: ends in a line feed");
        assert!(
            !run.stdout.starts_with(&[0xEF, 0xBB, 0xBF]),
            "{level}: no byte order mark"
        );
        assert!(std::str::from_utf8(&run.stdout).is_ok(), "{level}: UTF-8");
    }
}
