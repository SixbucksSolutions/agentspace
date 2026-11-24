//! The command line's v0.07 additions: `-Host`, `-All` and `-InstanceType`, their usage errors in
//! the order operator.md gives them, and the five host checks that run after the `-Alias` checks
//! (docs/operator.md, Command line). Every one of these fails before the home and the system folder
//! are read, starts no program and writes nothing, which `assert_failure` shows by running the
//! program in a throwaway home whose seam variables name nothing.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;

// ---- -Host's form ----

#[test]
fn a_host_that_is_not_a_project_name_names_the_value_as_given() {
    let thirty_three = "a".repeat(33);
    for bad in [
        "",
        "-",
        "--x",
        "-abc",
        "abc-",
        "a_b",
        "a.b",
        "a/b",
        "a b",
        "a\u{e9}b",
        "\u{e9}",
        "a!",
        "x*",
        thirty_three.as_str(),
        "agentspace-host.agentspace",
    ] {
        assert_failure(&["status", "-Host", bad], &not_a_project(bad));
        assert_failure(&["up", "-host", bad], &not_a_project(bad));
    }
}

#[test]
fn a_host_made_of_whitespace_alone_is_not_a_project_name() {
    // Surrounding whitespace is trimmed (lease-api.md, Project), and nothing is left.
    assert_failure(&["status", "-Host", " "], &not_a_project(" "));
    assert_failure(&["status", "-Host", "   "], &not_a_project("   "));
}

#[test]
fn a_host_accepts_the_lease_clis_forms_in_any_case_with_surrounding_whitespace() {
    // A line that passes every check reaches the registry lookup, which fails in the throwaway
    // home for a project that is not the box's: the first message that is not a usage error.
    let thirty_two = "a".repeat(32);
    for good in [
        "a",
        "A",
        "9",
        "x-y",
        "x--y",
        "Agentspace",
        "AGENTSPACE",
        " agentspace ",
        "  a1-B2  ",
        thirty_two.as_str(),
    ] {
        let normalized = good.trim().to_lowercase();
        assert_failure(&["status", "-Host", good], &no_host(&normalized));
    }
    // And the box's own project is found in the seed, so it reaches the command itself.
    assert_reaches_command(&["status", "-Host", "swimcoach"], "status");
    assert_reaches_command(&["down", "-Host", " SwimCoach "], "down");
}

#[test]
fn a_host_value_with_a_control_character_is_one_line_with_each_written_as_a_question_mark() {
    for (given, shown) in [
        ("a\nb", "a?b"),
        ("a\tb", "a?b"),
        ("\n", "?"),
        ("x\r\ny", "x??y"),
    ] {
        assert_failure(&["status", "-Host", given], &not_a_project(shown));
    }
}

#[test]
fn the_argument_after_host_is_its_value_whatever_it_looks_like() {
    assert_line_fails("status -Host -All", &not_a_project("-All"));
    assert_line_fails("status -Host -Quiet", &not_a_project("-Quiet"));
    assert_line_fails("status -Host -x", &not_a_project("-x"));
    assert_line_fails("status -Host up", &no_host("up"));
    // `up` is the value, so no command is given.
    assert_line_fails("-Host up", &no_command());
    assert_line_fails("-Host x -All", &no_command());
}

// ---- -InstanceType's form ----

#[test]
fn an_instance_type_that_is_not_of_the_form_names_the_value_as_given() {
    for bad in [
        "",
        "m9g",
        "m9g.",
        ".xlarge",
        "M9g.xlarge",
        "m9g.XLarge",
        "m9g.x.large",
        "-m9g.xlarge",
        "m9g.-xlarge",
        "m9g xlarge",
        "m9g.xlarge ",
        " m9g.xlarge",
        "m9g_2.xlarge",
        "m9g.xl_arge",
        "m9g\u{e9}.xlarge",
        "m9g.xlarge\n",
        "m9g\n.xlarge",
        "m9g..xlarge",
    ] {
        let shown: String = bad
            .chars()
            .map(|c| if c.is_control() { '?' } else { c })
            .collect();
        assert_failure(
            &["deploy", "-Host", "x", "-InstanceType", bad],
            &not_an_instance_type(&shown),
        );
    }
}

#[test]
fn an_instance_type_of_the_form_passes_the_form_check() {
    // The first message that is not a usage error is the host check or the lookup.
    for good in [
        "m9g.xlarge",
        "m9g.2xlarge",
        "m7i.metal-24xl",
        "t3.micro",
        "c7g.12xlarge",
        "a.b",
        "0.0",
        "a-b.c-d",
        "m6g.metal",
    ] {
        assert_failure(
            &["deploy", "-Host", "x", "-InstanceType", good],
            &no_host("x"),
        );
    }
}

#[test]
fn the_form_is_case_sensitive_and_the_value_is_not_checked_against_ec2() {
    // "Whether EC2 offers the type is not checked" (operator.md): a type that does not exist
    // passes the form and reaches the lookup.
    assert_failure(
        &["deploy", "-Host", "x", "-InstanceType", "zz9.nosuchsize"],
        &no_host("x"),
    );
    assert_failure(
        &["deploy", "-Host", "x", "-InstanceType", "M9G.xlarge"],
        &not_an_instance_type("M9G.xlarge"),
    );
}

#[test]
fn the_argument_after_instance_type_is_its_value_whatever_it_looks_like() {
    assert_line_fails(
        "deploy -Host x -InstanceType -Quiet",
        &not_an_instance_type("-Quiet"),
    );
    assert_line_fails(
        "deploy -Host x -InstanceType -Host y",
        &not_an_instance_type("-Host"),
    );
    // `y` is then a second positional argument.
    assert_line_fails("deploy -InstanceType -All", &not_an_instance_type("-All"));
}

// ---- Given twice, missing values, unknown parameters ----

#[test]
fn a_new_parameter_given_twice_names_the_parameter_as_the_page_spells_it() {
    assert_line_fails("status -Host a -Host b", &given_twice("-Host"));
    assert_line_fails("status -host a -HOST b", &given_twice("-Host"));
    assert_line_fails("status -All -All", &given_twice("-All"));
    assert_line_fails("status -all -ALL", &given_twice("-All"));
    assert_line_fails(
        "deploy -InstanceType m9g.xlarge -InstanceType m9g.xlarge",
        &given_twice("-InstanceType"),
    );
    assert_line_fails(
        "deploy -instancetype m9g.xlarge -INSTANCETYPE t3.micro",
        &given_twice("-InstanceType"),
    );
    // A repeated name is found before a missing value, and before the repeat's own value is
    // checked; the first one's value is checked as soon as it is read.
    assert_line_fails("status -Host a -Host", &given_twice("-Host"));
    assert_line_fails("status -Host a -Host bad!", &given_twice("-Host"));
    assert_line_fails("status -Host bad! -Host a", &not_a_project("bad!"));
    assert_line_fails(
        "deploy -InstanceType a.b -InstanceType",
        &given_twice("-InstanceType"),
    );
    assert_line_fails(
        "deploy -InstanceType bad -InstanceType a.b",
        &not_an_instance_type("bad"),
    );
    // `-All` and `-Quiet` are switches of their own: neither counts as the other.
    assert_line_fails("status -All -Quiet -All", &given_twice("-All"));
    assert_line_fails("status -Quiet -All -Quiet", &given_twice("-Quiet"));
}

#[test]
fn a_new_parameter_with_no_value_as_the_last_argument() {
    assert_line_fails("status -Host", &needs_value("-Host"));
    assert_line_fails("status -HOST", &needs_value("-Host"));
    assert_line_fails("-host", &needs_value("-Host"));
    assert_line_fails("deploy -InstanceType", &needs_value("-InstanceType"));
    assert_line_fails("deploy -instancetype", &needs_value("-InstanceType"));
    assert_line_fails(
        "deploy -Host x -InstanceType",
        &needs_value("-InstanceType"),
    );
    assert_line_fails("deploy -Quiet -Host", &needs_value("-Host"));
}

#[test]
fn all_takes_no_value_so_what_follows_it_is_the_command() {
    assert_line_fails("-All up", ALL_APPLIES);
    assert_line_fails("-All status -Host x", HOST_AND_ALL);
    assert_line_fails("-All -Host x", &no_command());
}

#[test]
fn an_unknown_parameter_close_to_a_new_one_is_unknown() {
    assert_line_fails("status -Hosts x", &unknown_parameter("-Hosts"));
    assert_line_fails("status -Alls", &unknown_parameter("-Alls"));
    assert_line_fails(
        "deploy -InstanceTypes x",
        &unknown_parameter("-InstanceTypes"),
    );
    assert_line_fails(
        "deploy -Instance m9g.xlarge",
        &unknown_parameter("-Instance"),
    );
    assert_line_fails("status -Hos x", &unknown_parameter("-Hos"));
}

#[test]
fn usage_errors_found_while_reading_come_before_every_host_check() {
    // A line with a host check's problem and an earlier usage error: the usage error speaks.
    assert_line_fails("ca -Host y -GiB abc", &not_a_number("abc"));
    assert_line_fails("ca -Host y -x", &unknown_parameter("-x"));
    assert_line_fails("up -All -Host bad!", &not_a_project("bad!"));
    assert_line_fails("provision -Alias", &needs_value("-Alias"));
    assert_line_fails("grow -InstanceType bad", &not_an_instance_type("bad"));
    assert_line_fails("up -All -All", &given_twice("-All"));
    assert_line_fails("status -Host y -All extra", &unexpected("extra"));
    assert_line_fails("bogus -Host y -All", &unknown_command("bogus"));
}

#[test]
fn the_command_list_in_the_missing_and_unknown_command_messages_names_all_ten() {
    // v0.08 adds `guard` (operator.md, Usage errors; guard_command_line.rs has the same two).
    assert_failure::<&str>(
        &[],
        "agentspace: No command given. The commands are deploy, up, sync, bootstrap, provision, ca, down, status, grow and guard.",
    );
    assert_failure(
        &["bogus"],
        "agentspace: Unknown command 'bogus'. The commands are deploy, up, sync, bootstrap, provision, ca, down, status, grow and guard.",
    );
}

// ---- The five host checks, in their order ----

#[test]
fn check_1_host_given_with_ca() {
    assert_line_fails("ca -Host y", HOST_NOT_FOR_CA);
    assert_line_fails("ca -host swimcoach", HOST_NOT_FOR_CA);
    assert_line_fails("-Host y ca", HOST_NOT_FOR_CA);
    assert_line_fails("CA -Quiet -Host Y", HOST_NOT_FOR_CA);
}

#[test]
fn check_2_all_given_with_a_command_that_does_not_take_it() {
    for command in [
        "deploy",
        "up",
        "bootstrap",
        "provision",
        "ca",
        "grow",
        "guard",
    ] {
        assert_line_fails(&format!("{command} -All"), ALL_APPLIES);
        assert_line_fails(&format!("-all {command}"), ALL_APPLIES);
    }
}

#[test]
fn all_is_accepted_by_status_down_and_sync_and_reaches_the_registry() {
    // In the throwaway home no registry exists, so -All is the box alone, whose first AWS call
    // fails with the seam's own line after the Host line. What matters here: it passed the checks.
    for command in ["status", "down"] {
        let home = Home::new();
        let r = home.run(&[command, "-All"]);
        assert_eq!(r.code, Some(1), "{command}: {:?}", r.err());
        assert_eq!(r.out(), "", "{command}");
        let err = r.err();
        let lines: Vec<&str> = err.lines().collect();
        assert_eq!(
            lines[0], "Host      swimcoach (agentbox), ready",
            "{command}"
        );
        assert!(
            lines[1].starts_with("agentspace: AGENTSPACE_AWS_CMD names"),
            "{command}: {err:?}"
        );
        assert_eq!(
            lines.last(),
            Some(&"agentspace: 1 of 1 hosts failed."),
            "{command}: {err:?}"
        );
    }
    let r = run(&["sync", "-All"]);
    assert_eq!(r.code, Some(1));
    assert_eq!(
        r.err().lines().next(),
        Some("Host      swimcoach (agentbox), ready")
    );
}

#[test]
fn check_3_host_and_all_together() {
    for command in ["status", "down", "sync"] {
        assert_line_fails(&format!("{command} -Host y -All"), HOST_AND_ALL);
        assert_line_fails(&format!("{command} -All -Host y"), HOST_AND_ALL);
    }
}

#[test]
fn check_4_provision_without_a_host() {
    assert_line_fails("provision", PROVISION_NEEDS_HOST);
    assert_line_fails("PROVISION -Quiet", PROVISION_NEEDS_HOST);
    assert_line_fails("-GiB 3 provision", PROVISION_NEEDS_HOST);
    assert_line_fails("provision -GiB 3", PROVISION_NEEDS_HOST);
    // With a host it passes the check, and is dispatched.
    assert_reaches_command(&["provision", "-Host", "x"], "provision");
}

#[test]
fn check_5_instance_type_given_with_a_command_but_deploy() {
    for command in ["up", "sync", "bootstrap", "ca", "down", "status", "grow"] {
        assert_line_fails(
            &format!("{command} -InstanceType m9g.xlarge"),
            INSTANCE_TYPE_APPLIES,
        );
        assert_line_fails(
            &format!("{command} -Host y -InstanceType m9g.xlarge"),
            if command == "ca" {
                HOST_NOT_FOR_CA
            } else {
                INSTANCE_TYPE_APPLIES
            },
        );
    }
    assert_line_fails(
        "provision -Host y -InstanceType m9g.xlarge",
        INSTANCE_TYPE_APPLIES,
    );
}

/// Whether a line gets past the command line: with `USERPROFILE` unset it meets the home, the
/// first thing after the usage errors and the alias and host checks, and no program is started.
fn assert_line_passes_the_command_line(args: &[&str]) {
    let r = run_with_env(args, |c| {
        c.env_remove("USERPROFILE");
    });
    assert_failure_run(&format!("{args:?}"), &r, USERPROFILE_UNSET);
}

#[test]
fn v0_09_deleted_the_sixth_host_check_deploy_takes_instance_type_for_every_host() {
    // D67, D71: `deploy -InstanceType <type>` is allowed for every host, the box included. The
    // box with no `-Host`, and `-Host swimcoach` in every spelling of the box's project, pass the
    // command line, and the next thing that speaks is the home.
    assert_line_passes_the_command_line(&["deploy", "-InstanceType", "m9g.xlarge"]);
    assert_line_passes_the_command_line(&[
        "deploy",
        "-Host",
        "swimcoach",
        "-InstanceType",
        "m9g.xlarge",
    ]);
    for host in ["SwimCoach", " swimcoach", "SWIMCOACH "] {
        assert_line_passes_the_command_line(&[
            "deploy",
            "-Host",
            host,
            "-InstanceType",
            "m9g.xlarge",
        ]);
    }
    // In any order, in any case, and with `-Quiet`.
    assert_line_passes_the_command_line(&["-InstanceType", "m7i.metal-24xl", "deploy"]);
    assert_line_passes_the_command_line(&["-quiet", "DEPLOY", "-instancetype", "m9g.2xlarge"]);
    // A host that is not the box passes as before: the lookup then speaks.
    assert_failure(
        &[
            "deploy",
            "-Host",
            "agentspace",
            "-InstanceType",
            "m9g.xlarge",
        ],
        &no_host("agentspace"),
    );
}

#[test]
fn the_checks_run_in_their_order_and_the_first_failure_ends_the_command() {
    // 1 before 2: ca with both.
    assert_line_fails("ca -All -Host y", HOST_NOT_FOR_CA);
    assert_line_fails("ca -Host y -All", HOST_NOT_FOR_CA);
    // 2 before 3: up is no command that takes -All, whatever else is wrong.
    assert_line_fails("up -All -Host y", ALL_APPLIES);
    assert_line_fails("grow -Host y -All", ALL_APPLIES);
    assert_line_fails("provision -All -Host y", ALL_APPLIES);
    // 2 before 4.
    assert_line_fails("provision -All", ALL_APPLIES);
    // 3 before 5.
    assert_line_fails("status -Host y -All -InstanceType m9g.xlarge", HOST_AND_ALL);
    // 2 before 5, for deploy too: `-All` is not `deploy`'s.
    assert_line_fails("deploy -All -InstanceType m9g.xlarge", ALL_APPLIES);
    assert_line_fails("deploy -Host y -All -InstanceType m9g.xlarge", ALL_APPLIES);
    // 4 before 5: provision's -InstanceType is refused only once -Host is there.
    assert_line_fails("provision -InstanceType m9g.xlarge", PROVISION_NEEDS_HOST);
    // 1 before 5.
    assert_line_fails("ca -Host y -InstanceType m9g.xlarge", HOST_NOT_FOR_CA);
    // 5 is the last host check: a command but deploy is refused as another command's, with or
    // without -Host.
    assert_line_fails("grow -InstanceType m9g.xlarge", INSTANCE_TYPE_APPLIES);
    assert_line_fails("ca -InstanceType m9g.xlarge", INSTANCE_TYPE_APPLIES);
    assert_line_fails(
        "status -All -InstanceType m9g.xlarge",
        INSTANCE_TYPE_APPLIES,
    );
}

#[test]
fn the_host_checks_come_after_the_alias_checks() {
    // operator.md's own example.
    assert_line_fails("ca -Alias x -Host y", ALIAS_APPLIES);
    assert_line_fails("status -Alias x -All", ALIAS_APPLIES);
    assert_line_fails("up -Alias bad! -All", ALIAS_INVALID);
    assert_line_fails("up -Alias bad! -InstanceType m9g.xlarge", ALIAS_INVALID);
    assert_line_fails("provision -Alias x", ALIAS_APPLIES);
    assert_line_fails("deploy -Alias x -InstanceType m9g.xlarge", ALIAS_APPLIES);
}

#[test]
fn a_host_check_failure_with_quiet_still_prints() {
    assert_failure(&["-Quiet", "ca", "-Host", "y"], HOST_NOT_FOR_CA);
    assert_failure(&["up", "-All", "-Quiet"], ALL_APPLIES);
    assert_failure(&["provision", "-Quiet"], PROVISION_NEEDS_HOST);
}

#[test]
fn every_host_check_failure_is_one_agentspace_line_and_writes_nothing() {
    for line in [
        "ca -Host y",
        "up -All",
        "status -Host y -All",
        "provision",
        "grow -InstanceType m9g.xlarge",
        "status -All -InstanceType m9g.xlarge",
        "status -Host bad!",
        "deploy -Host y -InstanceType bad",
    ] {
        let r = run(&words(line));
        assert_eq!(r.code, Some(1), "{line}");
        assert_eq!(r.out(), "", "{line}");
        let err = r.err();
        assert!(err.starts_with("agentspace: "), "{line}: {err:?}");
        assert_eq!(err.matches('\n').count(), 1, "{line}: {err:?}");
        assert!(r.leftovers.is_empty(), "{line}: {:?}", r.leftovers);
    }
}

// ---- Lines that pass, in every shape ----

#[test]
fn host_parameters_may_stand_in_any_order_and_in_any_case() {
    for line in [
        "-Host swimcoach status",
        "status -HOST swimcoach",
        "-quiet -host SWIMCOACH down",
        "grow -gib 3 -Host swimcoach",
        "up -Alias custom -Host swimcoach",
        "-Host swimcoach -Alias custom up",
    ] {
        let args = words(line);
        let command = args
            .iter()
            .find(|w| COMMANDS.contains(*w))
            .copied()
            .expect("a command");
        assert_reaches_command(&args, command);
    }
}
