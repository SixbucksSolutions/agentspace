//! `guard`'s command line: what it accepts, every usage error operator.md gives for it, its help
//! and its place in the top level's (docs/operator.md: Command line, Usage errors, Help, the host
//! checks, guard). A command line that passes every check reaches the guard's real run, which in an
//! empty throwaway home stops at its step 3, the missing lease state file: `reaches_the_guard`
//! below says so for each accepted line, through `assert_reaches_guard` in `common/cli.rs`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;

/// An accepted command line gets past every usage check and reaches the guard's run, which makes no
/// AWS call and starts no program: the harness's seam variables name nothing, so a call or a
/// program would give another message. It leaves the lock and the log in the home, and no more.
fn reaches_the_guard(line: &str) {
    assert_reaches_guard(&words(line));
}

// ---- What it accepts ----

#[test]
fn guard_and_guard_quiet_are_accepted() {
    // operator.md, Command line: `guard` is one of the ten commands; its line is `guard [-Quiet]`.
    reaches_the_guard("guard");
    reaches_the_guard("guard -Quiet");
    reaches_the_guard("-Quiet guard");
}

#[test]
fn guard_is_matched_without_regard_to_case_and_never_as_an_abbreviation() {
    for spelling in ["GUARD", "Guard", "gUaRd", "GuArD"] {
        reaches_the_guard(spelling);
        reaches_the_guard(&format!("{spelling} -quiet"));
    }
    for abbreviation in ["g", "gu", "gua", "guar", "guards", "guard1", "guar d"] {
        assert_failure(&[abbreviation], &unknown_command(abbreviation));
    }
}

#[test]
fn guard_s_gib_is_checked_for_form_and_ignored() {
    // operator.md, Command line, -GiB: every other command checks the form alone and ignores the
    // value, so `guard -GiB 5` is accepted, and so is `guard -GiB 0`.
    reaches_the_guard("guard -GiB 5");
    reaches_the_guard("guard -GiB 0");
    reaches_the_guard("guard -GiB -5");
    reaches_the_guard("-GiB 7 guard -Quiet");
    assert_line_fails("guard -GiB abc", &not_a_number("abc"));
    assert_line_fails("guard -GiB 2147483648", &not_a_number("2147483648"));
    assert_line_fails("guard -GiB", &needs_value("-GiB"));
    assert_line_fails("guard -GiB 1 -GiB 2", &given_twice("-GiB"));
}

// ---- The messages that name guard ----

#[test]
fn the_two_command_list_messages_name_guard() {
    // operator.md, Command line, Usage errors: the text, byte for byte.
    assert_failure::<&str>(
        &[],
        "agentspace: No command given. The commands are deploy, up, sync, bootstrap, provision, ca, down, status, grow and guard.",
    );
    assert_failure(
        &["bogus"],
        "agentspace: Unknown command 'bogus'. The commands are deploy, up, sync, bootstrap, provision, ca, down, status, grow and guard.",
    );
}

// ---- Check 1: -Host ----

#[test]
fn guard_given_host_fails_with_its_own_name_in_the_message() {
    // operator.md, the host checks, check 1: `<command>` is `ca` or `guard`.
    assert_eq!(
        HOST_NOT_FOR_GUARD,
        "agentspace: -Host does not apply to guard."
    );
    assert_line_fails("guard -Host x", HOST_NOT_FOR_GUARD);
    assert_line_fails("guard -host swimcoach", HOST_NOT_FOR_GUARD);
    assert_line_fails("-Host x guard", HOST_NOT_FOR_GUARD);
    assert_line_fails("GUARD -Quiet -Host Y", HOST_NOT_FOR_GUARD);
    // A whitespace-only value is no project, whatever command it is given to.
    assert_failure(&["guard", "-Host", " "], &not_a_project(" "));
}

#[test]
fn cas_message_is_unchanged_byte_for_byte() {
    assert_eq!(HOST_NOT_FOR_CA, "agentspace: -Host does not apply to ca.");
    assert_line_fails("ca -Host y", HOST_NOT_FOR_CA);
    assert_line_fails("CA -Host y", HOST_NOT_FOR_CA);
}

#[test]
fn check_1_comes_before_check_3_for_guard() {
    // operator.md: `guard -Host x -All` reports the `-Host` message, check 1 coming before check 3,
    // and so does `-All` first.
    assert_line_fails("guard -Host x -All", HOST_NOT_FOR_GUARD);
    assert_line_fails("guard -All -Host x", HOST_NOT_FOR_GUARD);
    assert_line_fails("-All -Host x guard", HOST_NOT_FOR_GUARD);
}

#[test]
fn a_bad_host_value_is_a_usage_error_found_before_the_checks() {
    // The arguments are read first, so the `-Host` value's own message wins over check 1.
    assert_line_fails("guard -Host bad!", &not_a_project("bad!"));
    assert_line_fails("guard -Host", &needs_value("-Host"));
    assert_line_fails("guard -Host x -Host y", &given_twice("-Host"));
}

// ---- Check 2: -All ----

#[test]
fn guard_given_all_fails_with_the_all_message() {
    // operator.md, check 2: `-All` applies to status, down and sync alone. `guard` is not `-All`.
    assert_line_fails("guard -All", ALL_APPLIES);
    assert_line_fails("-all guard", ALL_APPLIES);
    assert_line_fails("guard -ALL -Quiet", ALL_APPLIES);
    assert_line_fails("guard -All -All", &given_twice("-All"));
}

// ---- The -Alias and -InstanceType refusals ----

#[test]
fn guard_given_alias_fails_before_every_host_check() {
    // The two `-Alias` checks run before the six host checks.
    assert_line_fails("guard -Alias x", ALIAS_APPLIES);
    assert_line_fails("guard -Alias bad!", ALIAS_APPLIES);
    assert_failure(&["guard", "-Alias", ""], ALIAS_APPLIES);
    assert_line_fails("guard -Alias x -Host y", ALIAS_APPLIES);
    assert_line_fails("guard -Alias x -All", ALIAS_APPLIES);
    assert_line_fails("guard -Host y -Alias x", ALIAS_APPLIES);
}

#[test]
fn guard_given_instance_type_fails_with_check_5() {
    assert_line_fails("guard -InstanceType m9g.xlarge", INSTANCE_TYPE_APPLIES);
    assert_line_fails("-InstanceType m9g.xlarge guard", INSTANCE_TYPE_APPLIES);
    // The earlier checks come first: 1 and 2.
    assert_line_fails("guard -Host y -InstanceType m9g.xlarge", HOST_NOT_FOR_GUARD);
    assert_line_fails("guard -All -InstanceType m9g.xlarge", ALL_APPLIES);
    // And the value's form is a usage error of the arguments, before every check.
    assert_line_fails("guard -InstanceType bad", &not_an_instance_type("bad"));
}

// ---- Stray arguments, unknown parameters, repeats ----

#[test]
fn guard_refuses_a_stray_argument_and_an_unknown_parameter() {
    assert_line_fails("guard extra", &unexpected("extra"));
    assert_line_fails("guard status", &unexpected("status"));
    assert_line_fails("status guard", &unexpected("guard"));
    assert_line_fails("guard guard", &unexpected("guard"));
    assert_line_fails("guard -Foo", &unknown_parameter("-Foo"));
    assert_line_fails("guard --verbosely", &unknown_parameter("--verbosely"));
    assert_line_fails("guard -", &unknown_parameter("-"));
    assert_line_fails("guard -Quiet extra", &unexpected("extra"));
    assert_line_fails("guard -Quietly", &unknown_parameter("-Quietly"));
}

#[test]
fn guard_given_quiet_twice_fails() {
    assert_line_fails("guard -Quiet -Quiet", &given_twice("-Quiet"));
    assert_line_fails("-quiet guard -QUIET", &given_twice("-Quiet"));
}

#[test]
fn guard_s_usage_errors_run_left_to_right_and_the_first_ends_the_command() {
    assert_line_fails("guard -x extra", &unknown_parameter("-x"));
    assert_line_fails("guard extra -x", &unexpected("extra"));
    assert_line_fails("guard -Host x extra", &unexpected("extra"));
    assert_line_fails("guard -GiB abc -Host x", &not_a_number("abc"));
}

#[test]
fn every_guard_usage_error_is_one_agentspace_line_and_writes_nothing() {
    for line in [
        "guard -Host x",
        "guard -All",
        "guard -Alias x",
        "guard -InstanceType m9g.xlarge",
        "guard extra",
        "guard -Foo",
        "guard -GiB abc",
        "guard -Quiet -Quiet",
        "guard -Host x -All",
    ] {
        let r = run(&words(line));
        assert_eq!(r.code, Some(1), "{line}");
        assert_eq!(r.out(), "", "{line}");
        let err = r.err();
        assert!(err.starts_with("agentspace: "), "{line}: {err:?}");
        assert_eq!(err.matches('\n').count(), 1, "{line}: {err:?}");
        assert!(err.ends_with('\n'), "{line}: {err:?}");
        assert!(r.leftovers.is_empty(), "{line}: {:?}", r.leftovers);
    }
}

// ---- Help ----

#[test]
fn guard_help_is_the_block_in_operator_md_byte_for_byte() {
    // The block, with the doc's own words checked so a changed extractor shows.
    let doc = help_text("guard");
    assert!(
        doc.starts_with("Usage: agentspace-operator guard [-Quiet]\n\n"),
        "{doc:?}"
    );
    assert!(
        doc.ends_with("Exit codes: 0 success, or another guard is running; 1 failure.\n"),
        "{doc:?}"
    );
    assert_help(&["guard", "-h"], "guard");
    assert_help(&["guard", "--help"], "guard");
    assert_help(&["-h", "guard"], "guard");
    assert_help(&["--help", "guard"], "guard");
    assert_help(&["GUARD", "-H"], "guard");
    assert_help(&["guard", "--HELP"], "guard");
}

#[test]
fn help_wins_over_every_guard_usage_error_and_host_check() {
    assert_line_help("guard -Host x -h", "guard");
    assert_line_help("guard -All -h", "guard");
    assert_line_help("guard -Alias x --help", "guard");
    assert_line_help("guard -InstanceType bad -h", "guard");
    assert_line_help("guard extra -h", "guard");
    assert_line_help("guard -GiB abc -h", "guard");
    assert_line_help("guard -Quiet -Quiet -h", "guard");
    assert_line_help("-Host x -All guard --help", "guard");
    assert_line_help("guard -Foo -h", "guard");
    assert_line_help("-Quiet guard -h", "guard");
    // Which argument is the command, for help: a value never names it.
    assert_line_help("-Host guard -h", "top");
    assert_line_help("-Alias -h guard", "guard");
}

#[test]
fn guard_help_starts_no_program_and_writes_nothing() {
    let r = run(&["guard", "-h"]);
    assert_eq!(r.err(), "");
    assert!(r.leftovers.is_empty(), "{:?}", r.leftovers);
}

#[test]
fn the_top_levels_help_lists_guard() {
    // operator.md, The top level: `guard` among the commands, in the doc's words.
    let doc = help_text("top");
    assert!(
        doc.contains(
            "  guard      Stop every running host no live lease explains, after a\n             15-minute grace, and send each running host its lease horizon\n"
        ),
        "the extracted block names guard"
    );
    assert!(doc.contains("provision needs it, and ca and guard take none"));
    for spelling in ["-h", "--help"] {
        assert_help(&[spelling], "top");
    }
    // The program prints it, whole, and guard's line is in what it prints.
    let r = run(&["-h"]);
    assert_eq!(r.code, Some(0));
    assert!(
        r.out()
            .contains("\n  guard      Stop every running host no live lease explains, after a\n")
    );
    assert!(
        r.out().contains(
            "             15-minute grace, and send each running host its lease horizon\n"
        )
    );
}

#[test]
fn status_grow_and_sync_help_are_the_blocks_in_operator_md() {
    // The three help texts v0.08 changes besides the top level's and guard's.
    for level in ["status", "grow", "sync"] {
        assert_help(&[level, "-h"], level);
    }
    assert!(help_text("grow").contains("a whole number of 1 or more (default 2)"));
    assert!(help_text("status").contains("its count of live leases"));
}

// ---- Where the home and the system folder come in ----

#[test]
fn guard_needs_the_home_and_the_system_folder_after_its_usage_checks() {
    // After the command line's checks: `guard -Host x` is a usage error even with no home.
    let r = run_with_env(&["guard", "-Host", "x"], |c| {
        c.env_remove("USERPROFILE");
    });
    assert_failure_run("guard -Host x, no home", &r, HOST_NOT_FOR_GUARD);
    let r = run_with_env(&["guard"], |c| {
        c.env_remove("USERPROFILE");
    });
    assert_failure_run("guard, no home", &r, USERPROFILE_UNSET);
    let r = run_with_env(&["guard", "-Quiet"], |c| {
        c.env("SystemRoot", "");
    });
    assert_failure_run("guard, no system folder", &r, SYSTEMROOT_UNSET);
}
