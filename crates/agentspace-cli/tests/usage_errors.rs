//! The command line's rules and every usage error (lease-api.md, Command lines and Messages): the
//! tokens, ASCII case-insensitive names, the two forms of a flag's value, which error is reported
//! when several apply, and the five cases that carry `field`.
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Each case runs through the
//! built binary, which MUST end with exit 2, nothing on stdout and one error document on stderr, and
//! through `execute_in` in a throwaway world, which MUST return exit 2. The messages are the wording of the Messages
//! table, which an implementation SHOULD use; the code, the exit code and `field` are MUST.

mod common;

use common::*;

#[test]
fn no_command_and_no_subcommand_are_usage_errors_without_field() {
    check_usage_cases(vec![
        case(&[], None, MSG_NO_COMMAND),
        case(&["lease"], None, MSG_NO_SUBCOMMAND),
        case(&["LEASE"], None, MSG_NO_SUBCOMMAND),
    ]);
}

#[test]
fn an_unknown_command_or_subcommand_is_reported_as_soon_as_it_is_read() {
    check_usage_cases(vec![
        case(&["frob"], None, &msg_unknown_command("frob")),
        case(&["Frob"], None, &msg_unknown_command("Frob")),
        case(&["frob", "--help"], None, &msg_unknown_command("frob")),
        case(
            &["frob", "--bogus", "lease"],
            None,
            &msg_unknown_command("frob"),
        ),
        case(&["lease", "frob"], None, &msg_unknown_subcommand("frob")),
        case(&["lease", "FROB"], None, &msg_unknown_subcommand("FROB")),
        case(&["LEASE", "FROB"], None, &msg_unknown_subcommand("FROB")),
        case(
            &["lease", "frob", "--help"],
            None,
            &msg_unknown_subcommand("frob"),
        ),
        case(
            &["lease", "frob", "--ttl"],
            None,
            &msg_unknown_subcommand("frob"),
        ),
        // An option before the command never takes a value, so the next word is the command.
        case(
            &["--project", "x", "lease", "create"],
            None,
            &msg_unknown_command("x"),
        ),
        // An unknown command is reported even when an unknown option comes before it.
        case(&["--bogus", "frob"], None, &msg_unknown_command("frob")),
        case(
            &["--bogus", "frob", "--help"],
            None,
            &msg_unknown_command("frob"),
        ),
        case(
            &["lease", "--bogus", "frob"],
            None,
            &msg_unknown_subcommand("frob"),
        ),
        case(
            &["--bogus", "lease", "--bogus2", "frob"],
            None,
            &msg_unknown_subcommand("frob"),
        ),
        // The command and the subcommand are ASCII letters only; an empty argument is an argument.
        case(&[""], None, &msg_unknown_command("")),
        case(&["lease", ""], None, &msg_unknown_subcommand("")),
        case(&["lease1"], None, &msg_unknown_command("lease1")),
        case(
            &["lease", "creat3"],
            None,
            &msg_unknown_subcommand("creat3"),
        ),
        case(
            &["lease", "cre-ate"],
            None,
            &msg_unknown_subcommand("cre-ate"),
        ),
        case(
            &["lease create"],
            None,
            &msg_unknown_command("lease create"),
        ),
        case(
            &["lease", "créate"],
            None,
            &msg_unknown_subcommand("créate"),
        ),
        // Only `lease` is a command in v0, and only the five are subcommands.
        case(&["create"], None, &msg_unknown_command("create")),
        case(&["lease", "lease"], None, &msg_unknown_subcommand("lease")),
        case(
            &["lease", "delete"],
            None,
            &msg_unknown_subcommand("delete"),
        ),
        case(&["lease", "help"], None, &msg_unknown_subcommand("help")),
        case(&["help"], None, &msg_unknown_command("help")),
    ]);
}

#[test]
fn matching_is_ascii_only_so_other_scripts_that_fold_to_ascii_never_match() {
    // U+017F, the long s, upper-cases to `S`, so a Unicode-aware match would take these for ASCII.
    check_usage_cases(vec![
        case(&["lea\u{17f}e"], None, &msg_unknown_command("lea\u{17f}e")),
        case(
            &["lease", "crea\u{17f}e"],
            None,
            &msg_unknown_subcommand("crea\u{17f}e"),
        ),
        case(
            &["lease", "create", "--project", "x", "--\u{17f}ize", "m"],
            None,
            &msg_unknown_option_sub("--\u{17f}ize", "create"),
        ),
        // U+0131 is the dotless i, which upper-cases to `I`.
        case(
            &["lease", "l\u{131}st"],
            None,
            &msg_unknown_subcommand("l\u{131}st"),
        ),
        case(
            &["lease", "list", "--\u{131}"],
            None,
            &msg_unknown_option_sub("--\u{131}", "list"),
        ),
    ]);
}

#[test]
fn an_unknown_option_is_reported_at_its_own_level() {
    check_usage_cases(vec![
        case(&["--bogus"], None, &msg_unknown_option_top("--bogus")),
        case(&["--help=yes"], None, &msg_unknown_option_top("--help=yes")),
        case(&["-"], None, &msg_unknown_option_top("-")),
        case(&["--"], None, &msg_unknown_option_top("--")),
        case(&["-x"], None, &msg_unknown_option_top("-x")),
        case(&["--=x"], None, &msg_unknown_option_top("--=x")),
        case(
            &["--bogus=a b"],
            None,
            &msg_unknown_option_top("--bogus=a b"),
        ),
        case(
            &["lease", "--bogus"],
            None,
            &msg_unknown_option_lease("--bogus"),
        ),
        case(&["lease", "-p"], None, &msg_unknown_option_lease("-p")),
        case(&["lease", "--"], None, &msg_unknown_option_lease("--")),
        case(
            &["lease", "create", "--project", "x", "--bogus"],
            None,
            &msg_unknown_option_sub("--bogus", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "-ttl", "8h"],
            None,
            &msg_unknown_option_sub("-ttl", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "-p", "y"],
            None,
            &msg_unknown_option_sub("-p", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--ttl:"],
            None,
            &msg_unknown_option_sub("--ttl:", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--ttl-x", "8h"],
            None,
            &msg_unknown_option_sub("--ttl-x", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "-"],
            None,
            &msg_unknown_option_sub("-", "create"),
        ),
        // A bare `--` does not end the options.
        case(
            &["lease", "create", "--project", "x", "--"],
            None,
            &msg_unknown_option_sub("--", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--", "--ttl", "8h"],
            None,
            &msg_unknown_option_sub("--", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--help=yes"],
            None,
            &msg_unknown_option_sub("--help=yes", "create"),
        ),
        // The whole argument is quoted, a value after `=` included.
        case(
            &["lease", "create", "--project", "x", "--bogus=1"],
            None,
            &msg_unknown_option_sub("--bogus=1", "create"),
        ),
        // An unknown option never takes a value, so the next word is not read as one.
        case(
            &["lease", "create", "--bogus", "x"],
            None,
            &msg_unknown_option_sub("--bogus", "create"),
        ),
        case(
            &["lease", "list", "--bogus"],
            None,
            &msg_unknown_option_sub("--bogus", "list"),
        ),
    ]);
}

#[test]
fn a_flag_that_another_subcommand_has_is_an_unknown_option() {
    check_usage_cases(vec![
        case(
            &["lease", "get", ID, "--ttl", "8h"],
            None,
            &msg_unknown_option_sub("--ttl", "get"),
        ),
        case(
            &["lease", "get", "--ttl", "8h"],
            None,
            &msg_unknown_option_sub("--ttl", "get"),
        ),
        case(
            &["lease", "get", ID, "--TTL=8h"],
            None,
            &msg_unknown_option_sub("--TTL=8h", "get"),
        ),
        case(
            &["lease", "get", "--project", "x"],
            None,
            &msg_unknown_option_sub("--project", "get"),
        ),
        case(
            &["lease", "release", ID, "--ttl", "8h"],
            None,
            &msg_unknown_option_sub("--ttl", "release"),
        ),
        case(
            &["lease", "renew", ID, "--state", "ready"],
            None,
            &msg_unknown_option_sub("--state", "renew"),
        ),
        case(
            &["lease", "renew", ID, "--project", "x"],
            None,
            &msg_unknown_option_sub("--project", "renew"),
        ),
        case(
            &["lease", "list", "--ttl", "8h"],
            None,
            &msg_unknown_option_sub("--ttl", "list"),
        ),
        case(
            &["lease", "create", "--project", "x", "--state", "ready"],
            None,
            &msg_unknown_option_sub("--state", "create"),
        ),
    ]);
}

#[test]
fn an_unexpected_positional_argument_is_a_usage_error_without_field() {
    check_usage_cases(vec![
        case(
            &["lease", "create", "--project", "x", "extra"],
            None,
            &msg_unexpected("extra", "create"),
        ),
        case(
            &["lease", "create", "extra"],
            None,
            &msg_unexpected("extra", "create"),
        ),
        case(
            &["lease", "create", ""],
            None,
            &msg_unexpected("", "create"),
        ),
        case(&["lease", "get", ID, ID], None, &msg_unexpected(ID, "get")),
        case(&["lease", "get", ID, ""], None, &msg_unexpected("", "get")),
        case(&["lease", "get", "", ""], None, &msg_unexpected("", "get")),
        case(
            &["lease", "renew", ID, "extra"],
            None,
            &msg_unexpected("extra", "renew"),
        ),
        case(
            &["lease", "renew", "a", "--ttl", "8h", "b"],
            None,
            &msg_unexpected("b", "renew"),
        ),
        case(
            &["lease", "release", ID, ID],
            None,
            &msg_unexpected(ID, "release"),
        ),
        case(
            &["lease", "list", "extra"],
            None,
            &msg_unexpected("extra", "list"),
        ),
        case(
            &["lease", "list", "--state", "ready", "extra"],
            None,
            &msg_unexpected("extra", "list"),
        ),
        // The subcommand is quoted in lowercase, whatever case it was typed in.
        case(
            &["LEASE", "GET", ID, "y"],
            None,
            &msg_unexpected("y", "get"),
        ),
    ]);
}

#[test]
fn a_missing_lease_id_or_project_carries_field_id_or_project() {
    check_usage_cases(vec![
        case(&["lease", "create"], Some("project"), MSG_NO_PROJECT),
        case(
            &["lease", "create", "--ttl", "8h"],
            Some("project"),
            MSG_NO_PROJECT,
        ),
        case(
            &[
                "lease",
                "create",
                "--size",
                "m",
                "--platform",
                "linux/arm64",
            ],
            Some("project"),
            MSG_NO_PROJECT,
        ),
        case(&["lease", "get"], Some("id"), &msg_no_id("get")),
        case(&["lease", "renew"], Some("id"), &msg_no_id("renew")),
        case(
            &["lease", "renew", "--ttl", "8h"],
            Some("id"),
            &msg_no_id("renew"),
        ),
        case(
            &["lease", "renew", "--ttl=8h"],
            Some("id"),
            &msg_no_id("renew"),
        ),
        case(&["lease", "release"], Some("id"), &msg_no_id("release")),
        case(&["LEASE", "GET"], Some("id"), &msg_no_id("get")),
        case(&["Lease", "Get"], Some("id"), &msg_no_id("get")),
    ]);
}

#[test]
fn a_flag_with_no_value_carries_the_flag_as_field() {
    check_usage_cases(vec![
        case(
            &["lease", "create", "--project"],
            Some("project"),
            &msg_needs_value("project", "create"),
        ),
        case(
            &["lease", "create", "--project", "--size", "m"],
            Some("project"),
            &msg_needs_value("project", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--ttl"],
            Some("ttl"),
            &msg_needs_value("ttl", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--ttl", "--size", "m"],
            Some("ttl"),
            &msg_needs_value("ttl", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--platform"],
            Some("platform"),
            &msg_needs_value("platform", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--profile"],
            Some("profile"),
            &msg_needs_value("profile", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--size"],
            Some("size"),
            &msg_needs_value("size", "create"),
        ),
        // The flag's name is lowercase in the message, whatever case it was typed in.
        case(
            &["lease", "create", "--project", "x", "--TTL"],
            Some("ttl"),
            &msg_needs_value("ttl", "create"),
        ),
        case(
            &["lease", "renew", ID, "--ttl"],
            Some("ttl"),
            &msg_needs_value("ttl", "renew"),
        ),
        case(
            &["lease", "renew", ID, "--ttl", "--ttl", "8h"],
            Some("ttl"),
            &msg_needs_value("ttl", "renew"),
        ),
        case(
            &["lease", "list", "--project"],
            Some("project"),
            &msg_needs_value("project", "list"),
        ),
        case(
            &["lease", "list", "--state"],
            Some("state"),
            &msg_needs_value("state", "list"),
        ),
        case(
            &["lease", "list", "--state", "ready", "--state"],
            Some("state"),
            &msg_needs_value("state", "list"),
        ),
        case(
            &["lease", "list", "--state", "--project", "x"],
            Some("state"),
            &msg_needs_value("state", "list"),
        ),
        // A repeated flag whose later occurrence has no value is reported as that, not as twice.
        case(
            &["lease", "create", "--project", "x", "--project"],
            Some("project"),
            &msg_needs_value("project", "create"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--ttl",
                "8h",
                "--ttl",
                "--size",
                "m",
            ],
            Some("ttl"),
            &msg_needs_value("ttl", "create"),
        ),
    ]);
}

#[test]
fn a_flag_with_an_empty_value_carries_the_flag_as_field() {
    check_usage_cases(vec![
        case(
            &["lease", "create", "--project="],
            Some("project"),
            &msg_empty_value("project", "create"),
        ),
        case(
            &["lease", "create", "--project", ""],
            Some("project"),
            &msg_empty_value("project", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--ttl="],
            Some("ttl"),
            &msg_empty_value("ttl", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--ttl", ""],
            Some("ttl"),
            &msg_empty_value("ttl", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--platform="],
            Some("platform"),
            &msg_empty_value("platform", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--profile", ""],
            Some("profile"),
            &msg_empty_value("profile", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--size="],
            Some("size"),
            &msg_empty_value("size", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--TTL="],
            Some("ttl"),
            &msg_empty_value("ttl", "create"),
        ),
        case(
            &["lease", "renew", ID, "--ttl="],
            Some("ttl"),
            &msg_empty_value("ttl", "renew"),
        ),
        case(
            &["lease", "list", "--project="],
            Some("project"),
            &msg_empty_value("project", "list"),
        ),
        case(
            &["lease", "list", "--state", ""],
            Some("state"),
            &msg_empty_value("state", "list"),
        ),
        case(
            &["lease", "list", "--state="],
            Some("state"),
            &msg_empty_value("state", "list"),
        ),
        // A repeated flag whose later occurrence has an empty value is reported as that.
        case(
            &["lease", "create", "--project", "x", "--project="],
            Some("project"),
            &msg_empty_value("project", "create"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--ttl",
                "8h",
                "--ttl",
                "",
            ],
            Some("ttl"),
            &msg_empty_value("ttl", "create"),
        ),
        case(
            &["lease", "list", "--state", "ready", "--state="],
            Some("state"),
            &msg_empty_value("state", "list"),
        ),
    ]);
}

#[test]
fn a_flag_given_twice_carries_the_flag_as_field_whatever_case_each_is_in() {
    check_usage_cases(vec![
        case(
            &["lease", "create", "--project", "x", "--project", "y"],
            Some("project"),
            &msg_twice("project"),
        ),
        case(
            &["lease", "create", "--project", "x", "--Project=y"],
            Some("project"),
            &msg_twice("project"),
        ),
        case(
            &["lease", "create", "--project=x", "--project=x"],
            Some("project"),
            &msg_twice("project"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--ttl",
                "8h",
                "--TTL",
                "9h",
            ],
            Some("ttl"),
            &msg_twice("ttl"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--platform",
                "a/b",
                "--platform",
                "a/b",
            ],
            Some("platform"),
            &msg_twice("platform"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--profile",
                "a",
                "--PROFILE",
                "a",
            ],
            Some("profile"),
            &msg_twice("profile"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--size",
                "m",
                "--size",
                "m",
            ],
            Some("size"),
            &msg_twice("size"),
        ),
        case(
            &["lease", "renew", ID, "--ttl", "8h", "--ttl", "9h"],
            Some("ttl"),
            &msg_twice("ttl"),
        ),
        case(
            &["lease", "list", "--project", "a", "--project", "b"],
            Some("project"),
            &msg_twice("project"),
        ),
        case(
            &["lease", "list", "--project", "a", "--PROJECT=a"],
            Some("project"),
            &msg_twice("project"),
        ),
    ]);
}

#[test]
fn the_first_usage_error_in_line_order_is_the_one_reported() {
    check_usage_cases(vec![
        case(
            &["lease", "create", "--project", "x", "--ttl", "8", "--bogus"],
            None,
            &msg_unknown_option_sub("--bogus", "create"),
        ),
        case(
            &["lease", "create", "--ttl", "8", "--project"],
            Some("project"),
            &msg_needs_value("project", "create"),
        ),
        case(
            &["lease", "create", "--bogus", "--ttl"],
            None,
            &msg_unknown_option_sub("--bogus", "create"),
        ),
        case(
            &["lease", "create", "--ttl", "--bogus"],
            Some("ttl"),
            &msg_needs_value("ttl", "create"),
        ),
        case(
            &["lease", "get", "not-a-uuid", "--ttl", "8h"],
            None,
            &msg_unknown_option_sub("--ttl", "get"),
        ),
        case(
            &["lease", "get", "not-a-uuid", "extra"],
            None,
            &msg_unexpected("extra", "get"),
        ),
        case(
            &["lease", "list", "--project", "-x-", "--bogus"],
            None,
            &msg_unknown_option_sub("--bogus", "list"),
        ),
        // Twice is reported where the second one is read, so before a later unexpected argument.
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--project",
                "y",
                "extra",
            ],
            Some("project"),
            &msg_twice("project"),
        ),
        case(
            &[
                "lease",
                "create",
                "extra",
                "--project",
                "x",
                "--project",
                "y",
            ],
            None,
            &msg_unexpected("extra", "create"),
        ),
        // A usage error of any kind comes before a missing project or ID.
        case(
            &["lease", "create", "--bogus"],
            None,
            &msg_unknown_option_sub("--bogus", "create"),
        ),
        case(
            &["lease", "create", "extra"],
            None,
            &msg_unexpected("extra", "create"),
        ),
        case(
            &["lease", "create", "--ttl"],
            Some("ttl"),
            &msg_needs_value("ttl", "create"),
        ),
        case(
            &["lease", "create", "--ttl="],
            Some("ttl"),
            &msg_empty_value("ttl", "create"),
        ),
        case(
            &["lease", "get", "--bogus"],
            None,
            &msg_unknown_option_sub("--bogus", "get"),
        ),
        case(
            &["lease", "renew", "--ttl"],
            Some("ttl"),
            &msg_needs_value("ttl", "renew"),
        ),
        case(
            &["lease", "renew", "--ttl", "8h", "--ttl", "9h"],
            Some("ttl"),
            &msg_twice("ttl"),
        ),
        // The first of two flag errors in line order.
        case(
            &["lease", "create", "--size", "--ttl="],
            Some("size"),
            &msg_needs_value("size", "create"),
        ),
        case(
            &["lease", "create", "--ttl=", "--size"],
            Some("ttl"),
            &msg_empty_value("ttl", "create"),
        ),
        // A usage error comes before any bad value, wherever each is on the line.
        case(
            &[
                "lease",
                "create",
                "--project",
                "!",
                "--ttl",
                "8",
                "--ttl",
                "9",
            ],
            Some("ttl"),
            &msg_twice("ttl"),
        ),
        case(
            &["lease", "get", "not-a-uuid", "--bogus"],
            None,
            &msg_unknown_option_sub("--bogus", "get"),
        ),
    ]);
}

#[test]
fn only_the_five_usage_cases_carry_field() {
    // Each of these is a usage error the table says carries no field.
    check_usage_cases(vec![
        case(&[], None, MSG_NO_COMMAND),
        case(&["lease"], None, MSG_NO_SUBCOMMAND),
        case(&["frob"], None, &msg_unknown_command("frob")),
        case(&["lease", "frob"], None, &msg_unknown_subcommand("frob")),
        case(&["--bogus"], None, &msg_unknown_option_top("--bogus")),
        case(
            &["lease", "--bogus"],
            None,
            &msg_unknown_option_lease("--bogus"),
        ),
        case(
            &["lease", "list", "--bogus"],
            None,
            &msg_unknown_option_sub("--bogus", "list"),
        ),
        case(
            &["lease", "get", ID, "--ttl", "8h"],
            None,
            &msg_unknown_option_sub("--ttl", "get"),
        ),
        case(
            &["lease", "list", "extra"],
            None,
            &msg_unexpected("extra", "list"),
        ),
        case(&["lease", "get", ID, ID], None, &msg_unexpected(ID, "get")),
    ]);
}

// ---- Lines with no usage error: they reach the commands ----------------------------------------

/// A line with no usage error reaches its command, and gets that command's own answer, which MAY
/// be an error about a value (`invalid_value`, `out_of_range`) but is never a usage error. The run
/// through the binary and the run in process, each in a throwaway home with no driver, agree.
fn assert_reaches(line: &[&str], _sub: &str) {
    let line = args(line);
    let run = run_bin(&line);
    let p = reaches_command_problems(&run);
    assert!(p.is_empty(), "{}: {}", show(&line), p.join("; "));
    assert_eq!(
        in_process(&line),
        run.status.expect("An exit code"),
        "{} in process",
        show(&line)
    );
}

/// A line whose values are all valid gets the answer of an empty throwaway home with no driver:
/// `create` fails `internal` ("No lease driver is available"), `list` prints the empty list, and
/// `get`, `renew` and `release` are `not_found` (`id` is the lease ID, normalized).
fn assert_answer(line: &[&str], sub: &str, id: &str) {
    let line = args(line);
    let run = run_bin(&line);
    let p = answer_problems(&run, sub, id);
    assert!(p.is_empty(), "{}: {}", show(&line), p.join("; "));
    assert_eq!(
        in_process(&line),
        answer_exit(sub),
        "{} in process",
        show(&line)
    );
}

#[test]
fn every_subcommand_with_a_valid_line_gets_its_commands_answer() {
    assert_answer(&["lease", "create", "--project", "x"], "create", ID);
    assert_answer(&["lease", "get", ID], "get", ID);
    assert_answer(&["lease", "renew", ID], "renew", ID);
    assert_answer(&["lease", "renew", ID, "--ttl", "8h"], "renew", ID);
    assert_answer(&["lease", "release", ID], "release", ID);
    assert_answer(&["lease", "list"], "list", ID);
    assert_answer(&["lease", "list", "--project", "x"], "list", ID);
    assert_answer(&["lease", "list", "--state", "ready"], "list", ID);
}

#[test]
fn a_command_subcommand_and_flag_match_without_regard_to_ascii_case() {
    assert_reaches(&["Lease", "CREATE", "--Project", "x"], "create");
    assert_reaches(&["LEASE", "create", "--PROJECT=x", "--TtL", "8h"], "create");
    assert_reaches(
        &[
            "lease",
            "Create",
            "--pRoJeCt",
            "x",
            "--PLATFORM",
            "a/b",
            "--Profile=p",
            "--SIZE",
            "m",
        ],
        "create",
    );
    assert_reaches(&["LEASE", "GET", ID], "get");
    assert_reaches(&["lease", "Renew", ID, "--TTL", "8h"], "renew");
    assert_reaches(&["lease", "RELEASE", ID], "release");
    assert_reaches(
        &["LEASE", "LIST", "--STATE", "READY", "--Project=x"],
        "list",
    );
}

#[test]
fn a_flags_value_takes_the_equals_form_and_the_next_argument_form_alike() {
    assert_reaches(&["lease", "create", "--project=x", "--ttl=8h"], "create");
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl", "8h"],
        "create",
    );
    // Everything after the first `=` is the value, even one that starts with `-` or holds `=`.
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl=a=b"],
        "create",
    );
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl=--bogus"],
        "create",
    );
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl=-5m"],
        "create",
    );
    assert_reaches(
        &["lease", "create", "--project=--size", "--ttl", "8h"],
        "create",
    );
    // A single dash starts a value, and so does a lone `-`.
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl", "-5m"],
        "create",
    );
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl", "-"],
        "create",
    );
    assert_reaches(&["lease", "create", "--project", "-x-"], "create");
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl", "-hx"],
        "create",
    );
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl", "-help"],
        "create",
    );
    // A value MAY hold spaces when the shell passes it as one argument.
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl", "1h 30m"],
        "create",
    );
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl=1h 30m"],
        "create",
    );
}

#[test]
fn a_value_of_only_whitespace_is_not_an_empty_value() {
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl", " "],
        "create",
    );
    assert_reaches(&["lease", "create", "--project", "x", "--ttl= "], "create");
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl", "\t"],
        "create",
    );
    assert_reaches(&["lease", "list", "--state", " "], "list");
    assert_reaches(&["lease", "list", "--state=\u{3000}"], "list");
}

#[test]
fn the_two_value_forms_agree_for_a_value_with_a_line_feed() {
    // The prototype read `--project=<a value with a line feed>` as an unknown option (#0083).
    assert_reaches(&["lease", "create", "--project=a\nb"], "create");
    assert_reaches(&["lease", "create", "--project", "a\nb"], "create");
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl=8h\n"],
        "create",
    );
    assert_reaches(
        &["lease", "create", "--project", "x", "--ttl", "8h\n"],
        "create",
    );
    assert_reaches(&["lease", "create", "--project=\n"], "create");
    assert_reaches(
        &["lease", "create", "--project", "x", "--size=\r\n"],
        "create",
    );
}

#[test]
fn the_lease_id_is_positional_anywhere_among_the_flags_and_an_empty_one_counts() {
    assert_reaches(&["lease", "renew", "--ttl", "8h", ID], "renew");
    assert_reaches(&["lease", "renew", "--ttl=8h", ID], "renew");
    assert_reaches(&["lease", "renew", ID, "--ttl", "8h"], "renew");
    assert_reaches(&["lease", "get", ""], "get");
    assert_reaches(&["lease", "get", "not-a-uuid"], "get");
    assert_reaches(&["lease", "release", "/x"], "release");
    assert_reaches(&["lease", "release", " "], "release");
    assert_reaches(&["lease", "renew", "", "--ttl", "8h"], "renew");
}

#[test]
fn list_state_may_be_repeated_and_no_other_flag_may() {
    assert_reaches(
        &["lease", "list", "--state", "ready", "--state", "pending"],
        "list",
    );
    assert_reaches(
        &["lease", "list", "--state=ready", "--STATE", "pending"],
        "list",
    );
    assert_reaches(&["lease", "list", "--state", "ready,pending"], "list");
    assert_reaches(&["lease", "list", "--state", "ready,,pending"], "list");
    assert_reaches(
        &[
            "lease",
            "list",
            "--project",
            "x",
            "--state",
            "a",
            "--state",
            "b",
        ],
        "list",
    );
}

#[test]
fn a_help_like_value_after_an_equals_is_a_value_and_does_not_end_the_line() {
    assert_reaches(&["lease", "create", "--project=-h"], "create");
    assert_reaches(&["lease", "create", "--project=--help"], "create");
    assert_reaches(&["lease", "list", "--state=--help"], "list");
    assert_reaches(&["lease", "renew", ID, "--ttl=--HELP"], "renew");
}

#[test]
fn a_valid_line_never_prints_help_and_a_failed_one_prints_nothing_on_stdout() {
    for line in [
        args(&["lease", "create", "--project", "x"]),
        args(&["lease", "get", ID]),
        args(&["lease", "renew", ID]),
        args(&["lease", "release", ID]),
    ] {
        let run = run_bin(&line);
        assert!(run.stdout.is_empty(), "{}: nothing on stdout", show(&line));
    }
    // A list succeeds in the throwaway home: one JSON document, never help.
    let run = run_bin(&args(&["lease", "list"]));
    assert_eq!(run.stdout, b"{\"leases\":[]}\n");
}
