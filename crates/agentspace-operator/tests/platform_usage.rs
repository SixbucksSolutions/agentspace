//! v0.12's `-Platform` on the command line: its form and normalization through the lease CLI's
//! parser, every usage error it adds in the order operator.md gives them, and the three host checks
//! (1a, 3a and 4a) that sit right after the `-Host` checks they mirror (docs/operator.md, Command
//! line; From the command line; Messages). Every refusal here fails before the home and the system
//! folder are read, starts no program and writes nothing, which `assert_failure` shows by running
//! the program in a throwaway home whose seam variables name nothing.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;

/// The commands that take `-Platform` beside `-Host` (docs/operator.md, Command line, the
/// parameter table): every command but `ca` and `guard`.
const TAKES_PLATFORM: [&str; 8] = [
    "deploy",
    "up",
    "sync",
    "bootstrap",
    "provision",
    "down",
    "status",
    "grow",
];

// ---- A value the lease CLI's parser accepts, and agentspace does not support ----

#[test]
fn an_unsupported_platform_prints_the_lease_clis_message_with_the_normalized_value() {
    // The parser accepts each and normalizes it; none of the normalized values is one of the
    // three exactly (pins P2, P9 and P21).
    for (given, normalized) in [
        ("windows/arm64", "windows/arm64"),
        ("linux/386", "linux/386"),
        ("linux/amd64/v3", "linux/amd64/v3"),
        ("darwin/arm64", "darwin/arm64"),
        ("linux/arm64/v9", "linux/arm64/v9"),
        // The value is reported as the parser normalizes it: case, whitespace, `x86-64`.
        (" Windows/ARM64 ", "windows/arm64"),
        ("LINUX/X86-64/V3", "linux/amd64/v3"),
        ("linux/x86_64/v3", "linux/amd64/v3"),
        ("Darwin/AARCH64", "darwin/arm64"),
    ] {
        assert_failure(
            &["status", "-Host", "x", "-Platform", given],
            &platform_unsupported(normalized),
        );
    }
}

#[test]
fn every_command_that_takes_platform_refuses_an_unsupported_one_with_the_same_message() {
    for command in TAKES_PLATFORM {
        assert_failure(
            &[command, "-Host", "x", "-Platform", "windows/arm64"],
            &platform_unsupported("windows/arm64"),
        );
    }
}

// ---- A value the parser refuses ----

#[test]
fn a_platform_that_is_not_an_os_arch_pair_is_reported_as_given() {
    for bad in ["linux", "linux-amd64", "", "-All", "-Quiet", "x", "linux/"] {
        assert_failure(
            &["status", "-Host", "x", "-Platform", bad],
            &platform_not_a_pair(bad),
        );
    }
}

#[test]
fn a_refused_platform_is_reported_as_given_with_its_whitespace_and_control_characters() {
    // `<arg>` is the argument as given, so ` Linux ` keeps its spaces; each control character
    // is written as `?`, so the message stays one line (pin P54).
    assert_failure(
        &["status", "-Host", "x", "-Platform", " Linux "],
        &platform_not_a_pair(" Linux "),
    );
    assert_failure(
        &["status", "-Host", "x", "-Platform", "Linux"],
        &platform_not_a_pair("Linux"),
    );
    assert_failure(
        &["status", "-Host", "x", "-Platform", "a\nb"],
        &platform_not_a_pair("a?b"),
    );
    assert_failure(
        &["status", "-Host", "x", "-Platform", "\t"],
        &platform_not_a_pair("?"),
    );
}

#[test]
fn every_command_that_takes_platform_refuses_a_malformed_one_with_the_same_message() {
    for command in TAKES_PLATFORM {
        assert_failure(
            &[command, "-Host", "x", "-Platform", "linux-amd64"],
            &platform_not_a_pair("linux-amd64"),
        );
    }
}

#[test]
fn a_refused_platform_is_found_while_the_arguments_are_read_so_before_every_host_check() {
    // Without -Host the line is check 4a's, but the value's own error is found first, in the
    // left-to-right reading.
    assert_failure(
        &["status", "-Platform", "linux"],
        &platform_not_a_pair("linux"),
    );
    assert_failure(
        &["status", "-Platform", "windows/arm64"],
        &platform_unsupported("windows/arm64"),
    );
    assert_failure(&["ca", "-Platform", "linux"], &platform_not_a_pair("linux"));
    assert_failure(
        &["status", "-All", "-Host", "x", "-Platform", "linux"],
        &platform_not_a_pair("linux"),
    );
}

// ---- Check 4a, 1a and 3a ----

#[test]
fn platform_without_host_is_refused_by_check_4a_for_every_command_that_takes_it() {
    for command in [
        "deploy",
        "up",
        "sync",
        "bootstrap",
        "down",
        "status",
        "grow",
    ] {
        for platform in ["linux/amd64", "linux/arm64", "windows/amd64"] {
            assert_failure(&[command, "-Platform", platform], PLATFORM_NEEDS_HOST);
        }
    }
    // The page's text, with `<project>` written as those nine characters.
    assert_eq!(
        PLATFORM_NEEDS_HOST,
        "agentspace: -Platform needs -Host <project>."
    );
}

#[test]
fn provision_without_host_is_still_check_4s_message_with_or_without_platform() {
    assert_failure(&["provision"], PROVISION_NEEDS_HOST);
    assert_failure(
        &["provision", "-Platform", "linux/amd64"],
        PROVISION_NEEDS_HOST,
    );
}

#[test]
fn platform_with_ca_or_guard_is_refused_by_check_1a_in_check_1s_form() {
    assert_failure(&["ca", "-Platform", "linux/amd64"], PLATFORM_NOT_FOR_CA);
    assert_failure(&["ca", "-Platform", "linux/arm64"], PLATFORM_NOT_FOR_CA);
    assert_failure(
        &["guard", "-Platform", "linux/amd64"],
        PLATFORM_NOT_FOR_GUARD,
    );
    assert_failure(
        &["guard", "-Platform", "windows/amd64"],
        PLATFORM_NOT_FOR_GUARD,
    );
}

#[test]
fn check_1_comes_before_check_1a_and_leaves_its_message_unchanged() {
    // `guard -Host x -Platform linux/amd64` reports check 1's message (the page's example).
    assert_failure(
        &["guard", "-Host", "x", "-Platform", "linux/amd64"],
        HOST_NOT_FOR_GUARD,
    );
    assert_failure(
        &["ca", "-Host", "x", "-Platform", "linux/amd64"],
        HOST_NOT_FOR_CA,
    );
    // The order of the two parameters on the line makes no difference to the check.
    assert_failure(
        &["ca", "-Platform", "linux/amd64", "-Host", "x"],
        HOST_NOT_FOR_CA,
    );
    // `ca` and `guard` with no `-Host` keep every message they had.
    assert_failure(&["ca", "-All"], ALL_APPLIES);
    assert_failure(&["guard", "-All"], ALL_APPLIES);
}

#[test]
fn platform_with_all_is_refused_by_check_3a_for_status_down_and_sync() {
    for command in ["status", "down", "sync"] {
        assert_failure(
            &[command, "-All", "-Platform", "linux/amd64"],
            PLATFORM_AND_ALL,
        );
        assert_failure(
            &[command, "-Platform", "linux/arm64", "-All"],
            PLATFORM_AND_ALL,
        );
    }
}

#[test]
fn check_2_comes_before_check_3a_and_check_3_before_it() {
    // `-All` with a command that does not take it: check 2.
    for command in ["deploy", "up", "bootstrap", "provision", "grow"] {
        assert_failure(&[command, "-All", "-Platform", "linux/amd64"], ALL_APPLIES);
    }
    // `-Host` and `-All` together: check 3, whatever `-Platform` is beside them.
    assert_failure(
        &["status", "-Host", "x", "-All", "-Platform", "linux/amd64"],
        HOST_AND_ALL,
    );
    assert_failure(
        &["down", "-Platform", "linux/amd64", "-All", "-Host", "x"],
        HOST_AND_ALL,
    );
}

#[test]
fn the_pages_examples_of_the_new_checks_are_each_reported_by_their_own() {
    // operator.md, Command line, under the host checks.
    assert_failure(
        &[
            "status",
            "-All",
            "-Platform",
            "linux/amd64",
            "-InstanceType",
            "m8a.large",
        ],
        PLATFORM_AND_ALL,
    );
    assert_failure(
        &["provision", "-Platform", "linux/amd64"],
        PROVISION_NEEDS_HOST,
    );
    assert_failure(
        &[
            "status",
            "-Platform",
            "linux/amd64",
            "-InstanceType",
            "m8a.large",
        ],
        PLATFORM_NEEDS_HOST,
    );
    assert_failure(
        &["guard", "-Host", "x", "-Platform", "linux/amd64"],
        HOST_NOT_FOR_GUARD,
    );
}

#[test]
fn check_4a_comes_before_check_5() {
    for command in ["status", "up", "down", "grow", "sync", "bootstrap"] {
        assert_failure(
            &[
                command,
                "-Platform",
                "linux/amd64",
                "-InstanceType",
                "m8a.xlarge",
            ],
            PLATFORM_NEEDS_HOST,
        );
    }
    // With a host, check 5 speaks.
    assert_failure(
        &[
            "status",
            "-Host",
            "x",
            "-Platform",
            "linux/amd64",
            "-InstanceType",
            "m8a.xlarge",
        ],
        INSTANCE_TYPE_APPLIES,
    );
}

#[test]
fn the_alias_checks_come_before_the_platform_checks() {
    assert_failure(
        &["status", "-Alias", "x", "-Platform", "linux/amd64"],
        ALIAS_APPLIES,
    );
    assert_failure(
        &["up", "-Alias", "bad!", "-Platform", "linux/amd64"],
        ALIAS_INVALID,
    );
}

// ---- The reading of -Platform's own argument ----

#[test]
fn platform_as_the_last_argument_needs_a_value_in_hosts_words() {
    assert_line_fails("status -Host x -Platform", &needs_value("-Platform"));
    assert_line_fails("status -Host x -PLATFORM", &needs_value("-Platform"));
    assert_line_fails("status -platform", &needs_value("-Platform"));
    assert_line_fails("-Platform", &needs_value("-Platform"));
    assert_line_fails("deploy -Quiet -Platform", &needs_value("-Platform"));
}

#[test]
fn platform_given_twice_names_the_parameter_as_the_page_spells_it() {
    assert_line_fails(
        "status -Host x -Platform linux/amd64 -Platform linux/arm64",
        &given_twice("-Platform"),
    );
    assert_line_fails(
        "status -Host x -platform linux/amd64 -PLATFORM linux/amd64",
        &given_twice("-Platform"),
    );
    // A repeated name is found before a missing value, and before the repeat's own value is
    // checked; the first one's value is checked as soon as it is read.
    assert_line_fails(
        "status -Host x -Platform linux/amd64 -Platform",
        &given_twice("-Platform"),
    );
    assert_line_fails(
        "status -Host x -Platform linux/amd64 -Platform bad",
        &given_twice("-Platform"),
    );
    assert_line_fails(
        "status -Host x -Platform bad -Platform linux/amd64",
        &platform_not_a_pair("bad"),
    );
}

#[test]
fn the_argument_after_platform_is_its_value_whatever_it_looks_like() {
    assert_line_fails(
        "status -Host x -Platform -All",
        &platform_not_a_pair("-All"),
    );
    assert_line_fails(
        "status -Host x -Platform -Quiet",
        &platform_not_a_pair("-Quiet"),
    );
    assert_line_fails(
        "status -Host x -Platform -Host",
        &platform_not_a_pair("-Host"),
    );
    // `up` is the value, so no command is given.
    assert_line_fails("-Platform up", &no_command());
    assert_line_fails("-Platform linux/amd64 -Host x", &no_command());
    // And a command after the value is the command.
    assert_line_fails("-Platform linux/amd64 status", PLATFORM_NEEDS_HOST);
}

#[test]
fn help_is_never_a_platforms_value_and_wins_over_every_platform_error() {
    // The argument after `-Platform` is its value unless it is `-h` or `--help`.
    assert_line_help("status -Host x -Platform -h", "status");
    assert_line_help("status -Host x -Platform --help", "status");
    assert_line_help("-Platform -h status", "status");
    assert_line_help("-Platform -h", "top");
    // Help wins over every usage error `-Platform` can add.
    assert_line_help("status -Platform linux -h", "status");
    assert_line_help("status -Platform windows/arm64 -h", "status");
    assert_line_help("status -All -Platform linux/amd64 -h", "status");
    assert_line_help("ca -Platform linux/amd64 -h", "ca");
    assert_line_help("guard -Platform linux/amd64 --help", "guard");
    assert_line_help("up -Platform linux/amd64 -Platform linux/arm64 -h", "up");
    // Which argument is the command, for help: a `-Platform` value never names it.
    assert_line_help("-Platform up -h", "top");
}

#[test]
fn the_usage_errors_are_found_in_the_left_to_right_reading() {
    assert_line_fails(
        "status -Host bad! -Platform linux/amd64",
        &not_a_project("bad!"),
    );
    assert_line_fails(
        "status -Platform linux -Host bad!",
        &platform_not_a_pair("linux"),
    );
    assert_line_fails("status -GiB abc -Platform linux", &not_a_number("abc"));
    assert_line_fails(
        "status -Platform linux -GiB abc",
        &platform_not_a_pair("linux"),
    );
    assert_line_fails(
        "status -Host x -Platform linux/amd64 extra",
        &unexpected("extra"),
    );
    assert_line_fails(
        "status -Host x -Platform linux/amd64 -Foo",
        &unknown_parameter("-Foo"),
    );
    assert_line_fails(
        "bogus -Host x -Platform linux/amd64",
        &unknown_command("bogus"),
    );
}

#[test]
fn grow_refuses_a_gib_of_zero_or_less_after_the_platform_checks_and_before_the_registry() {
    // `grow -Host nobody -GiB 0` reports the `-GiB` message and not the missing host; the same
    // holds with `-Platform`, whose host would be the missing `nobody.linux-amd64`.
    assert_failure(
        &[
            "grow",
            "-Host",
            "nobody",
            "-Platform",
            "linux/amd64",
            "-GiB",
            "0",
        ],
        &gib_not_positive("0"),
    );
    // The platform checks come first.
    assert_failure(
        &["grow", "-Platform", "linux/amd64", "-GiB", "0"],
        PLATFORM_NEEDS_HOST,
    );
    assert_failure(
        &[
            "grow",
            "-Host",
            "x",
            "-Platform",
            "linux/amd64",
            "-All",
            "-GiB",
            "0",
        ],
        ALL_APPLIES,
    );
}

// ---- A platform that is accepted ----

#[test]
fn each_form_the_lease_cli_takes_names_the_platform_it_normalizes_to() {
    // A line that passes every check reaches the registry lookup, which fails in the throwaway home
    // for a project that is not the box's, and names the host by its normalized platform. For
    // `linux/arm64` the message is v0.07's, unchanged.
    for given in [
        "linux/amd64",
        " Linux/AMD64 ",
        "LINUX/AMD64",
        "linux/x86_64",
        "LINUX/X86-64",
        "linux/X86_64",
    ] {
        assert_failure(
            &["status", "-Host", "x", "-Platform", given],
            &no_host_on("x", "linux/amd64"),
        );
    }
    for given in [
        "linux/arm64",
        "linux/aarch64",
        "linux/aarch64/v8",
        "LINUX/ARM64/V8",
        " linux/arm64 ",
    ] {
        assert_failure(&["status", "-Host", "x", "-Platform", given], &no_host("x"));
    }
    for given in ["windows/amd64", "Windows/X86_64", " WINDOWS/AMD64 "] {
        assert_failure(
            &["status", "-Host", "x", "-Platform", given],
            &no_host_on("x", "windows/amd64"),
        );
    }
}

#[test]
fn every_command_but_provision_names_the_hosts_name_and_its_platform_flag_when_it_has_no_host() {
    for command in [
        "up",
        "down",
        "status",
        "grow",
        "deploy",
        "sync",
        "bootstrap",
    ] {
        assert_failure(
            &[command, "-Host", "Example", "-Platform", "linux/amd64"],
            &no_host_on("example", "linux/amd64"),
        );
        assert_failure(
            &[command, "-Host", "Example", "-Platform", "windows/amd64"],
            &no_host_on("example", "windows/amd64"),
        );
    }
}

#[test]
fn the_box_is_swimcoachs_linux_arm64_host_alone() {
    // `-Host swimcoach` and `-Host swimcoach -Platform linux/arm64` reach the box's command; on
    // another platform swimcoach's host is a record like any other's, which a seed holds none of.
    for platform in ["linux/arm64", "linux/aarch64", " Linux/ARM64/v8 "] {
        assert_reaches_command(
            &["status", "-Host", "swimcoach", "-Platform", platform],
            "status",
        );
        assert_reaches_command(
            &["down", "-Host", "SwimCoach", "-Platform", platform],
            "down",
        );
    }
    assert_failure(
        &["status", "-Host", "swimcoach", "-Platform", "linux/amd64"],
        &no_host_on("swimcoach", "linux/amd64"),
    );
    assert_failure(
        &["down", "-Host", "swimcoach", "-Platform", "windows/amd64"],
        &no_host_on("swimcoach", "windows/amd64"),
    );
}

#[test]
fn platform_linux_arm64_reaches_each_command_as_no_platform_does() {
    for command in ["up", "down", "status", "grow"] {
        assert_reaches_command(
            &[command, "-Host", "swimcoach", "-Platform", "linux/arm64"],
            command,
        );
    }
    for command in ["deploy", "sync", "bootstrap"] {
        assert_reaches_command(
            &[command, "-Host", "swimcoach", "-Platform", "linux/arm64"],
            command,
        );
    }
    // `provision` for a project with no record and no project list: v0.07's message, the same
    // with `-Platform linux/arm64` as without it.
    assert_reaches_command(
        &["provision", "-Host", "x", "-Platform", "linux/arm64"],
        "provision",
    );
    assert_reaches_command(
        &["provision", "-Host", "x", "-Platform", "linux/aarch64/v8"],
        "provision",
    );
}

#[test]
fn quiet_and_the_other_parameters_sit_beside_platform_in_any_order() {
    assert_failure(
        &["-Quiet", "status", "-Platform", "linux/amd64", "-Host", "x"],
        &no_host_on("x", "linux/amd64"),
    );
    assert_failure(
        &[
            "grow",
            "-GiB",
            "3",
            "-Platform",
            "linux/amd64",
            "-Host",
            "x",
        ],
        &no_host_on("x", "linux/amd64"),
    );
    assert_failure(
        &[
            "up",
            "-Alias",
            "custom.alias",
            "-Host",
            "x",
            "-Platform",
            "linux/amd64",
        ],
        &no_host_on("x", "linux/amd64"),
    );
    assert_failure(
        &[
            "deploy",
            "-InstanceType",
            "m8a.xlarge",
            "-Platform",
            "linux/amd64",
            "-Host",
            "x",
        ],
        &no_host_on("x", "linux/amd64"),
    );
}
