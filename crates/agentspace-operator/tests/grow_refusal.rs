//! `grow`'s refusal of a `-GiB` of 0 or less (D61, #0164; docs/operator.md: Command line, Usage
//! errors, `grow`'s `-GiB` check, grow). It is a usage error: exactly the message on stderr,
//! nothing on stdout, exit 1, and no AWS call at all, no ssh, nothing written in the home, `-Host`'s
//! key, certificate and alias included. It comes after the other usage errors and the host checks,
//! and before the home, the system folder and the host registry.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/cli.rs"]
mod cli;
#[path = "common/rig.rs"]
mod rig;

use rig::{ID, OTHER_ID, Rig, VOL, aws_args};

/// The values operator.md names for the refusal, and the other ways to write a value of 0 or less.
const REFUSED: [&str; 9] = [
    "0",
    "-0",
    "+0",
    "-1",
    "-2147483648",
    "000",
    "-007",
    "0000000000000000000000000000",
    "-100",
];

/// A rig whose answers would let `grow` go on if it ever asked: a running box with a volume of 8.
fn rig_with_a_box() -> Rig {
    let rig = Rig::standard();
    rig.write_box_id(ID);
    rig
}

/// Asserts the refusal for `args`: the message alone, and the stand-ins never run.
fn assert_refused(rig: &Rig, args: &[&str], arg: &str) {
    let out = rig.run(args);
    rig::assert_fails_with(&out, &cli::gib_not_positive(arg), &format!("{args:?}"));
    assert_eq!(out.err().matches('\n').count(), 1, "{args:?}: one line");
    let calls = rig.calls();
    assert!(
        calls.is_empty(),
        "{args:?}: no AWS call, no ssh, no program: {} call(s) recorded",
        calls.len()
    );
    assert!(
        rig.calls_of("ec2-modify-volume").is_empty(),
        "{args:?}: no ec2 modify-volume"
    );
}

#[test]
fn grow_given_a_gib_of_zero_or_less_is_refused_and_makes_no_aws_call() {
    for gib in REFUSED {
        let rig = rig_with_a_box();
        assert_refused(&rig, &["grow", "-GiB", gib], gib);
    }
}

#[test]
fn the_refusal_names_the_value_as_given_whatever_the_parameter_s_case() {
    for (name, gib) in [
        ("-gib", "0"),
        ("-GIB", "-1"),
        ("-GiB", "+0"),
        ("-gIb", "-0"),
    ] {
        let rig = rig_with_a_box();
        assert_refused(&rig, &["grow", name, gib], gib);
    }
    let rig = rig_with_a_box();
    assert_refused(&rig, &["GROW", "-GiB", "0"], "0");
    let rig = rig_with_a_box();
    assert_refused(&rig, &["-GiB", "-5", "grow"], "-5");
}

#[test]
fn nothing_is_read_made_or_written_before_the_refusal() {
    // No registry exists: nothing is read for it, so nothing is created.
    let rig = Rig::standard();
    assert_refused(&rig, &["grow", "-GiB", "0"], "0");
    assert!(
        rig.tree(&rig.profile).is_empty(),
        "nothing is written in the home: {:?}",
        rig.tree(&rig.profile)
    );
    assert!(rig.hosts_bytes().is_none());
    assert!(
        rig.tree(&rig.local_app_data).is_empty() && rig.tree(&rig.temp).is_empty(),
        "no other folder of the home gets anything either"
    );

    // A registry that is there is left as it was, byte for byte.
    let rig = rig_with_a_box();
    let before = rig.hosts_bytes();
    let tree = rig.tree(&rig.profile);
    assert_refused(&rig, &["grow", "-GiB", "-1"], "-1");
    assert_eq!(rig.hosts_bytes(), before);
    assert_eq!(rig.tree(&rig.profile), tree);
}

#[test]
fn the_refusal_is_found_before_the_host_registry_is_read() {
    // A registry that is not valid would fail every host command with its own message; the
    // refusal comes first. And `-Host nobody` would fail with the project's missing host.
    let rig = Rig::standard();
    rig.write_hosts_bytes(b"{ this is not json");
    assert_refused(&rig, &["grow", "-GiB", "0"], "0");
    assert_eq!(
        rig.hosts_bytes().as_deref(),
        Some(&b"{ this is not json"[..])
    );

    let rig = rig_with_a_box();
    assert_refused(&rig, &["grow", "-Host", "nobody", "-GiB", "0"], "0");
    let rig = Rig::standard();
    assert_refused(&rig, &["grow", "-Host", "nobody", "-GiB", "-3"], "-3");
}

#[test]
fn with_host_no_key_certificate_or_alias_is_written() {
    let rig = Rig::standard();
    rig.write_box_and("agentspace", "ready", ID, Some(OTHER_ID));
    let before = rig.hosts_bytes();
    for gib in ["0", "-1", "+0", "-2147483648"] {
        assert_refused(&rig, &["grow", "-Host", "agentspace", "-GiB", gib], gib);
        assert!(!rig.signed_anything(), "-GiB {gib}: nothing signed");
        assert!(!rig.host_dir("agentspace").exists(), "-GiB {gib}");
        assert!(!rig.host_alias_path("agentspace").exists(), "-GiB {gib}");
        assert_eq!(rig.hosts_bytes(), before, "-GiB {gib}");
    }
    // -Host swimcoach names the box and is refused the same way.
    assert_refused(&rig, &["grow", "-Host", "swimcoach", "-GiB", "0"], "0");
}

#[test]
fn the_refusal_needs_neither_the_home_nor_the_system_folder() {
    // As every usage error does (operator.md: `grow -GiB 0` with `USERPROFILE` unset gives the
    // refusal). Nothing is started, so the missing variables are never reached.
    let rig = rig_with_a_box();
    for name in ["USERPROFILE", "SystemRoot"] {
        let out = rig.op(&["grow", "-GiB", "0"]).env_remove(name).run();
        rig::assert_fails_with(&out, &cli::gib_not_positive("0"), name);
        let out = rig.op(&["grow", "-GiB", "-1"]).env(name, "").run();
        rig::assert_fails_with(&out, &cli::gib_not_positive("-1"), name);
    }
    assert!(rig.calls().is_empty());
}

#[test]
fn quiet_does_not_hide_the_refusal() {
    let rig = rig_with_a_box();
    let out = rig.run(&["grow", "-GiB", "0", "-Quiet"]);
    rig::assert_fails_with(&out, &cli::gib_not_positive("0"), "-Quiet");
    let out = rig.run(&["-Quiet", "grow", "-GiB", "-1"]);
    rig::assert_fails_with(&out, &cli::gib_not_positive("-1"), "-Quiet first");
    assert!(rig.calls().is_empty());
}

#[test]
fn the_refusal_comes_after_every_other_check_and_before_the_home() {
    // operator.md, Command line: help, then the arguments, then the two `-Alias` checks, then the
    // six host checks, then `grow`'s `-GiB` check.
    let rig = rig_with_a_box();
    let cases: [(&[&str], &str); 8] = [
        (&["grow", "-Alias", "x", "-GiB", "0"], cli::ALIAS_APPLIES),
        (&["grow", "-All", "-GiB", "0"], cli::ALL_APPLIES),
        (
            &["grow", "-InstanceType", "m9g.xlarge", "-GiB", "0"],
            cli::INSTANCE_TYPE_APPLIES,
        ),
        (
            &["grow", "-Host", "y", "-All", "-GiB", "0"],
            cli::ALL_APPLIES,
        ),
        (
            &["grow", "-Alias", "bad!", "-Host", "y", "-GiB", "-1"],
            cli::ALIAS_APPLIES,
        ),
        (
            &["-GiB", "0", "grow", "-Quiet", "-Quiet"],
            "agentspace: -Quiet is given more than once.",
        ),
        (
            &["grow", "-GiB", "0", "-x"],
            "agentspace: Unknown parameter '-x'.",
        ),
        (
            &["grow", "-GiB", "0", "extra"],
            "agentspace: Unexpected argument 'extra'.",
        ),
    ];
    for (args, message) in cases {
        let out = rig.run(args);
        rig::assert_fails_with(&out, message, &format!("{args:?}"));
        assert!(rig.calls().is_empty(), "{args:?}");
    }
    // The form of the value comes first: it is read with the arguments.
    let out = rig.run(&["grow", "-GiB", "abc"]);
    rig::assert_fails_with(&out, &cli::not_a_number("abc"), "a value that is no number");
    // Help wins over the refusal.
    let out = rig.run(&["grow", "-GiB", "0", "-h"]);
    assert_eq!(out.code, Some(0));
    assert_eq!(out.out(), cli::help_text("grow"));
    assert_eq!(out.err(), "");
    assert!(rig.calls().is_empty());
}

#[test]
fn only_grow_refuses_a_gib_of_zero_or_less() {
    // operator.md: every other command checks the form alone and ignores the value, so each of
    // these reaches its command and makes its calls.
    let rig = rig_with_a_box();
    rig.instance("stopped");
    let out = rig.run(&["status", "-GiB", "0"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(rig.keys()[0], "ec2-describe-instances");

    let rig = rig_with_a_box();
    let out = rig.run(&["down", "-GiB", "-5"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(rig.keys(), ["ec2-stop-instances"]);

    let rig = rig_with_a_box();
    let out = rig.run(&["status", "-GiB", "-2147483648", "-Host", "swimcoach"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert!(!rig.calls().is_empty());
}

#[test]
fn a_gib_of_one_is_not_refused_and_asks_ec2_for_the_size_plus_one() {
    let rig = rig_with_a_box();
    rig.instance("stopped");
    let out = rig.run(&["grow", "-GiB", "1"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        rig.calls_of("ec2-modify-volume")[0].args,
        aws_args("ec2", "modify-volume", &["--volume-id", VOL, "--size", "9"])
    );
}

#[test]
fn the_default_of_two_passes() {
    let rig = rig_with_a_box();
    rig.instance("stopped");
    let out = rig.run(&["grow"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        rig.calls_of("ec2-modify-volume")[0].args,
        aws_args(
            "ec2",
            "modify-volume",
            &["--volume-id", VOL, "--size", "10"]
        )
    );
}
