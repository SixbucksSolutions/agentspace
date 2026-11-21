//! `up`, `down`, `status` and `grow` for a host, with and without `-Host`, through the test seam's
//! stand-ins (docs/operator.md: Which host a command acts on, From the command line; down,
//! status, up, grow; Calls by command; How the operator reaches a host). Without `-Host` every
//! command acts on the box exactly as in v0.06, and `-Host swimcoach` is the same as no `-Host`.
//!
//! The rig's `SystemRoot` holds nothing, so the signing that every command given `-Host` does for
//! a host that is not the box fails at its first check, `ssh-keygen.exe` missing, after the work
//! the command does itself; that failure is what these tests read to show the order. The cases
//! where signing succeeds, against a real `ssh-keygen.exe` and a throwaway CA, are
//! `host_signing.rs`'s.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use rig::*;

/// The registry of the box (`ID`) and the project `agentspace`, in `state`, with `OTHER_ID`.
fn two_hosts(state: &str) -> Rig {
    let rig = Rig::standard();
    rig.write_box_and("agentspace", state, ID, Some(OTHER_ID));
    rig
}

/// The CA check's first message in the rig's empty `SystemRoot` (operator.md, ca, step 1).
fn keygen_missing(rig: &Rig) -> String {
    format!(
        "agentspace: {} is missing; install the Windows OpenSSH client.",
        rig.system_root
            .join("System32")
            .join("OpenSSH")
            .join("ssh-keygen.exe")
            .display()
    )
}

fn status_lines(id: &str) -> String {
    format!(
        "Instance  {id}\nState     running\nType      m9g.xlarge\nDisk      8 GiB gp3 ({VOL})\nSSM       online\n"
    )
}

fn growing(from: u32, to: u32) -> String {
    format!(
        "Growing {VOL} from {from} to {to} GiB (EBS allows one change per volume every 6 hours)..."
    )
}

fn describe_args(id: &str) -> Vec<String> {
    aws_args("ec2", "describe-instances", &["--instance-ids", id])
}

// ---- down ----

#[test]
fn down_for_a_host_stops_that_hosts_instance_whatever_its_state_and_signs_after_the_stop() {
    for state in ["creating", "ready", "failed"] {
        let rig = two_hosts(state);
        let out = rig.run(&["down", "-Host", "agentspace"]);
        assert_eq!(
            rig.keys(),
            ["ec2-stop-instances"],
            "{state}: the one call, and no stack call, since the record holds the ID"
        );
        assert_eq!(
            rig.calls()[0].args,
            aws_args("ec2", "stop-instances", &["--instance-ids", OTHER_ID]),
            "{state}: the host's instance, never the box's"
        );
        assert_eq!(
            out.err_lines(),
            [format!("Stopping {OTHER_ID}."), keygen_missing(&rig)],
            "{state}: the stop is requested first; the signing that fails after it fails the command"
        );
        assert_eq!(out.code, Some(1), "{state}");
        assert_eq!(out.out(), "", "{state}");
    }
}

#[test]
fn down_for_a_host_with_quiet_stops_it_and_still_prints_the_failure() {
    let rig = two_hosts("ready");
    let out = rig.run(&["down", "-Host", "agentspace", "-Quiet"]);
    assert_eq!(rig.keys(), ["ec2-stop-instances"]);
    assert_eq!(out.err_lines(), [keygen_missing(&rig)]);
    assert_eq!(out.code, Some(1));
}

#[test]
fn down_without_a_host_is_v006s_down_and_never_touches_the_hosts_instance() {
    let rig = two_hosts("ready");
    let out = rig.run(&["down"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), format!("Stopping {ID}.\n"));
    assert_eq!(
        rig.calls()[0].args,
        aws_args("ec2", "stop-instances", &["--instance-ids", ID])
    );
    assert!(!rig.signed_anything(), "the box needs no key or alias");
}

// ---- status ----

#[test]
fn status_for_a_host_reports_that_hosts_instance_and_then_signs() {
    let rig = two_hosts("ready");
    let out = rig.run(&["status", "-Host", "agentspace"]);
    assert_eq!(
        rig.keys(),
        [
            "ec2-describe-instances",
            "ec2-describe-volumes",
            "ssm-describe-instance-information"
        ]
    );
    assert_eq!(rig.calls()[0].args, describe_args(OTHER_ID));
    assert_eq!(
        rig.calls()[2].args,
        aws_args(
            "ssm",
            "describe-instance-information",
            &["--filters", &format!("Key=InstanceIds,Values={OTHER_ID}")]
        )
    );
    let expected = format!("{}{}\n", status_lines(OTHER_ID), keygen_missing(&rig));
    assert_eq!(
        out.err(),
        expected,
        "the five lines print first, and the signing's failure fails status after its report"
    );
    assert_eq!(out.code, Some(1));
    assert_eq!(out.out(), "");
}

#[test]
fn status_for_a_host_prints_its_report_whatever_quiet_says() {
    let rig = two_hosts("ready");
    let out = rig.run(&["status", "-Host", "agentspace", "-Quiet"]);
    assert_eq!(
        out.err(),
        format!("{}{}\n", status_lines(OTHER_ID), keygen_missing(&rig))
    );
}

#[test]
fn status_without_a_host_is_v006s_status() {
    let rig = two_hosts("ready");
    let out = rig.run(&["status"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), status_lines(ID), "five lines and no Host line");
    assert!(!rig.signed_anything());
}

// ---- up ----

#[test]
fn up_for_a_host_starts_and_waits_for_that_hosts_instance_then_warns_of_a_signing_failure() {
    let rig = two_hosts("ready");
    let out = rig.run(&["up", "-Host", "agentspace"]);
    assert_eq!(
        rig.keys(),
        ["ec2-describe-instances", "ec2-describe-instances"]
    );
    for c in rig.calls() {
        assert_eq!(c.args, describe_args(OTHER_ID));
    }
    let message = keygen_missing(&rig);
    let cause = message
        .strip_prefix("agentspace: ")
        .and_then(|m| m.strip_suffix('.'))
        .expect("a message that starts agentspace: and ends in a period");
    assert_eq!(
        out.err_lines(),
        [
            format!("Ready: {OTHER_ID}"),
            format!(
                "agentspace: Could not send the localswim token, because {cause}; the board will not work on the host this session."
            )
        ],
        "a signing failure is a push failure: it names its cause and fails nothing"
    );
    assert_eq!(
        out.code,
        Some(0),
        "up's exit code is 0 once the host is ready"
    );
    assert_eq!(out.out(), "");
    assert!(
        rig.calls_of("ssh").is_empty(),
        "the push is skipped when signing failed"
    );
}

#[test]
fn up_for_a_host_with_quiet_hides_the_signing_warning() {
    let rig = two_hosts("ready");
    let out = rig.run(&["up", "-Host", "agentspace", "-Quiet"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), "");
}

#[test]
fn up_for_a_stopped_host_starts_that_instance() {
    let rig = two_hosts("ready");
    rig.instance("stopped");
    let out = rig.run(&["up", "-Host", "agentspace"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let lines = out.err_lines();
    assert_eq!(lines[0], format!("Starting {OTHER_ID}..."));
    assert_eq!(lines[1], format!("Ready: {OTHER_ID}"));
    assert_eq!(
        rig.calls_of("ec2-start-instances")[0].args,
        aws_args("ec2", "start-instances", &["--instance-ids", OTHER_ID])
    );
}

#[test]
fn a_nested_up_for_a_host_signs_nothing_and_warns_of_nothing() {
    // `up` run by the operator's own ssh is nested: it never pushes, and so never first makes the
    // operator's key, certificate and alias (operator.md, up, step 3).
    let rig = two_hosts("ready");
    let out = rig
        .op(&["up", "-Host", "agentspace"])
        .env("AGENTSPACE_NESTED", "1")
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), format!("Ready: {OTHER_ID}\n"));
    assert!(!rig.signed_anything());
}

#[test]
fn up_without_a_host_is_v006s_up() {
    let rig = two_hosts("ready");
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), format!("Ready: {ID}\n"));
    for c in rig.calls() {
        assert_eq!(c.args, describe_args(ID));
    }
    assert!(!rig.signed_anything());
}

#[test]
fn up_with_a_host_and_an_alias_acts_on_the_host_and_still_checks_the_alias() {
    // `-Alias` and `-Host` MAY be given together: `-Host` picks the host and its instance ID.
    let rig = two_hosts("ready");
    let out = rig.run(&["up", "-Host", "agentspace", "-Alias", "custom.alias"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    for c in rig.calls() {
        assert_eq!(c.args, describe_args(OTHER_ID));
    }
}

// ---- grow ----

#[test]
fn grow_for_a_running_host_changes_its_volume_and_signs_before_its_online_step() {
    let rig = two_hosts("ready");
    let out = rig.run(&["grow", "-Host", "agentspace", "-GiB", "4"]);
    assert_eq!(
        rig.keys(),
        [
            "ec2-describe-instances",
            "ec2-describe-volumes",
            "ec2-modify-volume"
        ],
        "the volume's change in step 2 stands, and no ssh is started once signing has failed"
    );
    assert_eq!(rig.calls()[0].args, describe_args(OTHER_ID));
    assert_eq!(
        rig.calls()[2].args,
        aws_args(
            "ec2",
            "modify-volume",
            &["--volume-id", VOL, "--size", "12"]
        )
    );
    let lines = out.err_lines();
    assert_eq!(lines[0], growing(8, 12));
    assert_eq!(
        lines.last().map(String::as_str),
        Some(keygen_missing(&rig).as_str())
    );
    assert!(
        !lines
            .iter()
            .any(|l| l == "Growing the filesystem online..."),
        "the online step comes after the signing: {lines:?}"
    );
    assert_eq!(out.code, Some(1));
}

#[test]
fn grow_for_a_stopped_host_still_signs_after_the_volume_change() {
    // operator.md, grow: "When the host is not running, `grow -Host` still writes its alias and
    // signs a fresh certificate, after step 2".
    let rig = two_hosts("ready");
    rig.instance("stopped");
    let out = rig.run(&["grow", "-Host", "agentspace"]);
    assert_eq!(
        rig.keys(),
        [
            "ec2-describe-instances",
            "ec2-describe-volumes",
            "ec2-modify-volume"
        ]
    );
    assert_eq!(
        out.err_lines().last().map(String::as_str),
        Some(keygen_missing(&rig).as_str()),
        "{:?}",
        out.err()
    );
    assert_eq!(out.code, Some(1));
}

#[test]
fn grow_without_a_host_is_v006s_grow() {
    let rig = two_hosts("ready");
    rig.instance("stopped");
    let out = rig.run(&["grow"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err_lines(),
        [
            growing(8, 10),
            "The filesystem grows on its own at next boot.".to_string()
        ]
    );
    assert_eq!(rig.calls()[0].args, describe_args(ID));
    assert!(!rig.signed_anything());
}

// ---- -Host swimcoach is the box ----

#[test]
fn host_swimcoach_is_the_same_as_no_host_for_every_command() {
    let lines: [Vec<&str>; 4] = [
        vec!["up"],
        vec!["down"],
        vec!["status"],
        vec!["grow", "-GiB", "3"],
    ];
    for plain in lines {
        let mut named = plain.clone();
        named.push("-Host");
        named.push("swimcoach");
        let one = two_hosts("ready");
        let two = two_hosts("ready");
        if plain[0] == "grow" {
            one.instance("stopped");
            two.instance("stopped");
        }
        let a = one.run(&plain);
        let b = two.run(&named);
        assert_eq!(a.code, Some(0), "{plain:?}: {:?}", a.err());
        assert_eq!(b.code, a.code, "{named:?}");
        assert_eq!(b.err(), a.err(), "{named:?}: the same messages");
        assert_eq!(b.out(), a.out(), "{named:?}");
        let calls = |rig: &Rig| -> Vec<(String, Vec<String>)> {
            rig.calls().into_iter().map(|c| (c.key, c.args)).collect()
        };
        assert_eq!(calls(&two), calls(&one), "{named:?}: the same calls");
        assert!(
            !two.signed_anything() && !two.host_dir("swimcoach").exists(),
            "{named:?}: the box needs no key, certificate or alias, and has no folder"
        );
        assert_eq!(
            two.hosts_bytes(),
            one.hosts_bytes(),
            "{named:?}: the same registry"
        );
    }
}

#[test]
fn host_names_are_normalized_as_the_lease_cli_normalizes_a_project() {
    // `-Host Agentspace` names the project `agentspace`, and surrounding whitespace is trimmed.
    for given in ["Agentspace", "AGENTSPACE", " agentspace ", "  AgentSpace"] {
        let rig = two_hosts("ready");
        let out = rig.run(&["down", "-Host", given]);
        assert_eq!(rig.keys(), ["ec2-stop-instances"], "{given:?}");
        assert_eq!(
            rig.calls()[0].args,
            aws_args("ec2", "stop-instances", &["--instance-ids", OTHER_ID]),
            "{given:?}"
        );
        assert_eq!(
            out.err_lines()[0],
            format!("Stopping {OTHER_ID}."),
            "{given:?}"
        );
    }
    for given in ["SwimCoach", " swimcoach", "SWIMCOACH "] {
        let rig = two_hosts("ready");
        let out = rig.run(&["down", "-Host", given]);
        assert_eq!(out.code, Some(0), "{given:?}: {:?}", out.err());
        assert_eq!(out.err(), format!("Stopping {ID}.\n"), "{given:?}");
    }
}

// ---- A project with no record ----

#[test]
fn a_project_with_no_record_fails_every_command_but_provision_before_anything_runs() {
    for command in [
        "up",
        "down",
        "status",
        "grow",
        "deploy",
        "sync",
        "bootstrap",
    ] {
        let rig = two_hosts("ready");
        let before = rig.hosts_bytes();
        let out = rig.run(&[command, "-Host", "nosuch"]);
        assert_fails_with(
            &out,
            "agentspace: Project 'nosuch' has no host; provision -Host nosuch makes one.",
            command,
        );
        assert_eq!(out.err().lines().count(), 1, "{command}");
        assert!(
            rig.calls().is_empty(),
            "{command}: no program started and no AWS call made"
        );
        assert_eq!(rig.hosts_bytes(), before, "{command}: no record added");
        assert!(rig.tmp_files().is_empty(), "{command}");
        assert!(!rig.signed_anything(), "{command}");
    }
}

#[test]
fn the_missing_project_is_named_as_normalized_and_a_missing_registry_has_only_the_box() {
    let rig = Rig::standard();
    let out = rig.run(&["status", "-Host", " NoSuch "]);
    assert_fails_with(
        &out,
        "agentspace: Project 'nosuch' has no host; provision -Host nosuch makes one.",
        "normalized",
    );
    assert_eq!(rig.hosts_bytes(), None, "a lookup writes nothing");

    // The seed's one record is the box's: `agentspace` has none until provision adds it.
    let rig = Rig::standard();
    let out = rig.run(&["down", "-Host", "agentspace"]);
    assert_fails_with(
        &out,
        "agentspace: Project 'agentspace' has no host; provision -Host agentspace makes one.",
        "agentspace with no registry",
    );
    assert!(rig.calls().is_empty());
}

#[test]
fn the_lookup_comes_after_the_alias_checks_and_before_any_program() {
    let rig = two_hosts("ready");
    // An alias refusal wins over a missing project.
    let out = rig.run(&["up", "-Host", "nosuch", "-Alias", "bad!"]);
    assert_fails_with(
        &out,
        "agentspace: -Alias MUST be an SSH host alias of letters, digits, '.', '_' and '-' that does not start with '-'.",
        "alias first",
    );
    // A project with a record is found whatever its state.
    for state in ["creating", "ready", "failed"] {
        let rig = two_hosts(state);
        let out = rig.run(&["down", "-Host", "agentspace", "-Quiet"]);
        assert!(
            !out.err().contains("has no host"),
            "{state}: {:?}",
            out.err()
        );
    }
}
