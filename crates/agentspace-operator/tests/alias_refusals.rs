//! The two `-Alias` refusals, and that nothing runs before them (docs/operator.md, Command line:
//! "`-Alias` is checked before anything but help and the usage errors runs"). Mirrors
//! conformance/operator/test-alias-refusals.ps1's values.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;

#[test]
fn alias_is_refused_with_any_command_but_up() {
    for command in [
        "deploy",
        "sync",
        "bootstrap",
        "provision",
        "ca",
        "down",
        "status",
        "grow",
    ] {
        assert_failure(&[command, "-Alias", "devbox"], ALIAS_APPLIES);
        // Whatever the value is, the first check runs first.
        assert_failure(&[command, "-Alias", "bad!"], ALIAS_APPLIES);
        assert_failure(&[command, "-Alias", ""], ALIAS_APPLIES);
        // The parameter before the command, and in other cases.
        assert_failure(&["-ALIAS", "devbox", command], ALIAS_APPLIES);
        let upper = command.to_uppercase();
        assert_failure(
            &[upper.as_str(), "-alias", "devbox", "-Quiet"],
            ALIAS_APPLIES,
        );
    }
}

#[test]
fn up_refuses_an_alias_that_is_not_letters_digits_dot_underscore_dash() {
    // The values of conformance/operator/test-alias-refusals.ps1, and the ones its note leaves out.
    let bad = [
        "a;b",
        "a b",
        "a&b",
        "",
        "a|b",
        "a\"b",
        "-Fx",
        "a/b",
        "a%b",
        "-",
        "--",
        "a\\b",
        "a$b",
        "a`b",
        "a'b",
        "a(b",
        "a*b",
        "a:b",
        "a@b",
        "a=b",
        "a,b",
        "a<b",
        "a>b",
        "a?b",
        "a#b",
        "a~b",
        "a!b",
        "a\tb",
        " a",
        "a ",
        "\u{e9}",
        "caf\u{e9}",
        "\u{212a}",
        "\u{130}",
        "\u{ff21}",
        "\u{661}",
        "-h1",
        "-a",
        "-.",
        "-_",
    ];
    for alias in bad {
        assert_failure(&["up", "-Alias", alias], ALIAS_INVALID);
    }
}

#[test]
fn the_pattern_ends_with_z_so_a_trailing_line_feed_is_refused() {
    // `\z`, unlike `$`, never matches before a trailing newline.
    for alias in ["a\n", "devbox\n", "a\r\n", "\n", "a\nb", "\na", "a\r"] {
        assert_failure(&["up", "-Alias", alias], ALIAS_INVALID);
    }
}

#[test]
fn an_empty_alias_fails() {
    assert_failure(&["up", "-Alias", ""], ALIAS_INVALID);
}

#[test]
fn an_alias_may_not_start_with_a_dash_because_ssh_would_read_it_as_an_option() {
    for alias in ["-Fx", "-oProxyCommand=x", "-a", "-1", "-.", "-a-b"] {
        assert_failure(&["up", "-Alias", alias], ALIAS_INVALID);
    }
}

#[test]
fn the_pattern_is_case_sensitive_and_ascii_only_so_every_good_alias_passes() {
    let long = "a".repeat(200);
    for alias in [
        "agentspace",
        "devbox",
        "DevBox",
        "A",
        "a",
        "0",
        "9",
        "_",
        ".",
        "a.b",
        "a_b",
        "a-b",
        "a-",
        "a.",
        "a_",
        ".a",
        "_a",
        "0a",
        "a1.b2_c3-d4",
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "abcdefghijklmnopqrstuvwxyz",
        "0123456789",
        long.as_str(),
    ] {
        assert_reaches_command(&["up", "-Alias", alias], "up");
    }
}

#[test]
fn up_with_a_host_takes_an_alias_too_and_the_alias_checks_still_run_first() {
    // `-Alias` and `-Host` MAY be given together (operator.md, up, step 3). The alias is checked
    // before the host checks and before the registry is looked up.
    assert_failure(&["up", "-Host", "x", "-Alias", "bad!"], ALIAS_INVALID);
    assert_failure(&["up", "-Alias", "bad!", "-All"], ALIAS_INVALID);
    assert_failure(
        &["status", "-Alias", "x", "-Host", "y", "-All"],
        ALIAS_APPLIES,
    );
    assert_failure(&["ca", "-Alias", "x", "-Host", "y"], ALIAS_APPLIES);
    assert_failure(&["provision", "-Alias", "x"], ALIAS_APPLIES);
    assert_failure(&["provision", "-Alias", "bad!"], ALIAS_APPLIES);
    // A host name that is a valid alias too: the alias passes, and the host is then looked up.
    assert_failure(
        &["up", "-Host", "x", "-Alias", "custom.alias"],
        &no_host("x"),
    );
}

#[test]
fn the_default_alias_passes_the_same_check() {
    // `up` with no -Alias uses `agentspace`, which matches, and so reaches the command.
    assert_reaches_command(&["up"], "up");
}

#[test]
fn the_alias_value_is_not_the_command_nor_a_switch() {
    // `-Alias -Fx` is a value (operator.md); a refusal, and no unknown-parameter message.
    assert_failure(&["up", "-Alias", "-Fx"], ALIAS_INVALID);
    assert_failure(&["up", "-Alias", "-Quiet"], ALIAS_INVALID);
    assert_failure(&["-Alias", "-Fx", "up"], ALIAS_INVALID);
}

#[test]
fn a_refusal_exits_one_with_stdout_empty_and_one_line_on_stderr() {
    for args in [
        vec!["up", "-Alias", "a b"],
        vec!["status", "-Alias", "x"],
        vec!["up", "-Alias", ""],
    ] {
        let r = run(&args);
        assert_eq!(r.code, Some(1), "{args:?}");
        assert_eq!(r.out(), "", "{args:?}");
        let err = r.err();
        assert!(err.starts_with("agentspace: -Alias "), "{err:?}");
        assert_eq!(err.matches('\n').count(), 1, "{err:?}");
    }
}

#[test]
fn a_refusal_even_with_quiet_still_prints() {
    assert_failure(&["up", "-Quiet", "-Alias", "a b"], ALIAS_INVALID);
    assert_failure(&["-Quiet", "status", "-Alias", "x"], ALIAS_APPLIES);
}

#[test]
fn a_refusal_comes_before_anything_runs_not_even_a_program_start() {
    // Every seam variable names a path that does not exist, so a program the operator reached for
    // would fail with "<variable> names <value>, which cannot be run." in place of the refusal.
    let home = Home::new();
    for args in [
        vec!["up", "-Alias", "bad!"],
        vec!["sync", "-Alias", "x"],
        vec!["deploy", "-Alias", "x"],
        vec!["grow", "-Alias", "x"],
    ] {
        let r = home.run(&args);
        let expected = if args[0] == "up" {
            ALIAS_INVALID
        } else {
            ALIAS_APPLIES
        };
        assert_failure_run(&format!("{args:?}"), &r, expected);
        assert!(!r.err().contains("cannot be run"), "{:?}", r.err());
    }
}

#[cfg(unix)]
#[test]
fn a_refusal_starts_no_aws_ssh_or_scp_not_even_one_that_would_succeed() {
    use std::os::unix::fs::PermissionsExt;
    let home = Home::new();
    let marker = home.path().join("started");
    let tripwire = home.path().join("tripwire.sh");
    std::fs::write(
        &tripwire,
        format!(
            "#!/bin/sh\necho \"$0 $*\" >> '{}'\nexit 0\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&tripwire, std::fs::Permissions::from_mode(0o755)).unwrap();
    for args in [
        vec!["up", "-Alias", "bad!"],
        vec!["up", "-Alias", ""],
        vec!["status", "-Alias", "x"],
        vec!["deploy", "-Alias", "x"],
    ] {
        let mut c = home.command(&args);
        c.env("AGENTSPACE_AWS_CMD", &tripwire)
            .env("AGENTSPACE_SSH_CMD", &tripwire)
            .env("AGENTSPACE_SCP_CMD", &tripwire);
        let out = c.output().expect("the program starts");
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert!(
            !marker.exists(),
            "{args:?}: a seam program was started: {:?}",
            std::fs::read_to_string(&marker)
        );
    }
}
