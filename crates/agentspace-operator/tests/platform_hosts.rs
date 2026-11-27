//! `-Platform` on the commands that act on one host: `down`, `status`, `up`, `grow`, `deploy`,
//! `sync`, `bootstrap` and `provision`, each with and without it, and a project's hosts on two and
//! three platforms each reached by its own (docs/operator.md: Which host a command acts on, From
//! the command line; A host's platform and its names; down, status, up, grow, deploy, provision;
//! Messages). Every case runs the built program against the rig's stand-ins for the AWS CLI, ssh
//! and scp, in a throwaway home with a seeded host registry, and reads which instance, stack,
//! record and folder each step was given.
//!
//! The rig's `SystemRoot` holds nothing, so the signing that every command given `-Host` does for
//! a host that is not the box fails at its first check, `ssh-keygen.exe` missing, after the work
//! the command does itself; that failure is what these tests read to show the order. The cases
//! where signing succeeds are `platform_signing.rs`'s.
//!
//! A case asserts the operator's own output and which host, record and platform each step was
//! given, never a Windows host's own steps, which are the Windows spec's.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use rig::*;

/// One host of `agentspace` on each of the three platforms, after the box, in this file's order.
/// Each has its own instance ID, so a step that named the wrong host shows in the call's argument.
fn three_platforms() -> Rig {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec_on("agentspace", "linux/arm64", "ready", Some(OTHER_ID)),
        host_rec_on("agentspace", "linux/amd64", "ready", Some(THIRD_ID)),
        host_rec_on("agentspace", "windows/amd64", "ready", Some(WINDOWS_ID)),
    ]);
    rig
}

/// The platform of a `-Platform` argument as given and the instance of the host it names. The
/// first two name the same host: `linux/arm64` is the default (D91).
const REACHED: [(Option<&str>, &str); 7] = [
    (None, OTHER_ID),
    (Some("linux/arm64"), OTHER_ID),
    (Some("linux/aarch64"), OTHER_ID),
    (Some("linux/amd64"), THIRD_ID),
    (Some(" Linux/X86_64 "), THIRD_ID),
    (Some("windows/amd64"), WINDOWS_ID),
    (Some("WINDOWS/X86-64"), WINDOWS_ID),
];

/// A command line for `agentspace`'s host, with `-Platform` when `platform` is given.
fn line<'a>(command: &'a [&'a str], platform: Option<&'a str>) -> Vec<&'a str> {
    let mut args: Vec<&str> = command.to_vec();
    args.push("-Host");
    args.push("agentspace");
    if let Some(p) = platform {
        args.push("-Platform");
        args.push(p);
    }
    args
}

/// The no-record message for a host on `linux/arm64`: v0.07's, unchanged (operator.md, From the
/// command line).
fn no_host(project: &str) -> String {
    format!("agentspace: Project '{project}' has no host; provision -Host {project} makes one.")
}

/// The no-record message for a host on another platform (normalized): the host's name in the
/// message, and ` -Platform <platform>` in the command it suggests (pins P12 and P44).
fn no_host_on(project: &str, platform: &str) -> String {
    format!(
        "agentspace: Project '{}' has no host; provision -Host {project} -Platform {platform} makes one.",
        host_name(project, platform)
    )
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

fn describe_args(id: &str) -> Vec<String> {
    aws_args("ec2", "describe-instances", &["--instance-ids", id])
}

/// Whether a recorded call's arguments hold `text` as one of them.
fn names(call: &Call, text: &str) -> bool {
    call.args.iter().any(|a| a == text)
}

/// Whether any AWS call the rig recorded names `id`.
fn any_call_names(rig: &Rig, id: &str) -> bool {
    rig.calls().iter().any(|c| names(c, id))
}

// ---- down ----

#[test]
fn down_stops_the_instance_of_the_host_on_the_platform_it_is_given() {
    for (platform, id) in REACHED {
        let rig = three_platforms();
        let shown = format!("down, -Platform {platform:?}");
        let out = rig.run(&line(&["down"], platform));
        assert_eq!(rig.keys(), ["ec2-stop-instances"], "{shown}: the one call");
        assert_eq!(
            rig.calls()[0].args,
            aws_args("ec2", "stop-instances", &["--instance-ids", id]),
            "{shown}: that host's own instance"
        );
        assert_eq!(
            out.err_lines(),
            [format!("Stopping {id}."), keygen_missing(&rig)],
            "{shown}: the stop is requested first; the signing that fails after it fails the command"
        );
        assert_eq!(out.code, Some(1), "{shown}");
        assert_eq!(out.out(), "", "{shown}");
        for other in [ID, OTHER_ID, THIRD_ID, WINDOWS_ID] {
            if other != id {
                assert!(
                    !any_call_names(&rig, other),
                    "{shown}: no call names another host's instance"
                );
            }
        }
    }
}

#[test]
fn down_with_quiet_and_a_platform_stops_the_host_and_still_prints_the_failure() {
    let rig = three_platforms();
    let out = rig.run(&[
        "down",
        "-Host",
        "agentspace",
        "-Platform",
        "linux/amd64",
        "-Quiet",
    ]);
    assert_eq!(
        rig.calls()[0].args,
        aws_args("ec2", "stop-instances", &["--instance-ids", THIRD_ID])
    );
    assert_eq!(out.err_lines(), [keygen_missing(&rig)]);
    assert_eq!(out.code, Some(1));
}

#[test]
fn down_of_the_box_and_of_swimcoachs_arm64_host_is_the_same_command_whatever_platform_is_beside_it()
{
    for args in [
        vec!["down"],
        vec!["down", "-Host", "swimcoach"],
        vec!["down", "-Host", "swimcoach", "-Platform", "linux/arm64"],
        vec![
            "down",
            "-Host",
            "SwimCoach",
            "-Platform",
            "linux/aarch64/v8",
        ],
    ] {
        let rig = three_platforms();
        let out = rig.run(&args);
        assert_eq!(out.code, Some(0), "{args:?}: {:?}", out.err());
        assert_eq!(out.err(), format!("Stopping {ID}.\n"), "{args:?}");
        assert_eq!(
            rig.calls()[0].args,
            aws_args("ec2", "stop-instances", &["--instance-ids", ID]),
            "{args:?}"
        );
        assert!(!rig.signed_anything(), "{args:?}: the box needs no key");
    }
}

// ---- status ----

#[test]
fn status_reports_the_instance_of_the_host_on_the_platform_it_is_given() {
    for (platform, id) in REACHED {
        let rig = three_platforms();
        let shown = format!("status, -Platform {platform:?}");
        let out = rig.run(&line(&["status"], platform));
        assert_eq!(
            rig.keys(),
            [
                "ec2-describe-instances",
                "ec2-describe-volumes",
                "ssm-describe-instance-information"
            ],
            "{shown}"
        );
        assert_eq!(rig.calls()[0].args, describe_args(id), "{shown}");
        assert_eq!(
            rig.calls()[2].args,
            aws_args(
                "ssm",
                "describe-instance-information",
                &["--filters", &format!("Key=InstanceIds,Values={id}")]
            ),
            "{shown}"
        );
        // A host off `linux/arm64` prints `Platform  <platform>` first, before the five lines; a
        // `linux/arm64` host prints none (pin P57), so its report is v0.11's byte for byte.
        let platform_line = match id {
            THIRD_ID => "Platform  linux/amd64\n",
            WINDOWS_ID => "Platform  windows/amd64\n",
            _ => "",
        };
        assert_eq!(
            out.err(),
            format!(
                "{platform_line}{}{}\n",
                status_lines(id),
                keygen_missing(&rig)
            ),
            "{shown}: the report prints first, and the signing's failure fails status after it"
        );
        assert_eq!(out.code, Some(1), "{shown}");
        assert_eq!(out.out(), "", "{shown}");
    }
}

#[test]
fn the_platform_line_of_a_status_is_part_of_the_report_and_quiet_does_not_hide_it() {
    let rig = three_platforms();
    let out = rig.run(&[
        "status",
        "-Host",
        "agentspace",
        "-Platform",
        "linux/amd64",
        "-Quiet",
    ]);
    assert_eq!(
        out.err(),
        format!(
            "Platform  linux/amd64\n{}{}\n",
            status_lines(THIRD_ID),
            keygen_missing(&rig)
        )
    );
}

#[test]
fn status_for_a_host_on_a_platform_never_reads_the_lease_state_file() {
    // `status -Host <project> -Platform <platform>` prints the five lines alone, as `status -Host`
    // does, and MUST NOT read the lease state file (operator.md, status). A file that is not valid
    // would be said once after the last host's part of a `status -All`; here it is never opened.
    let rig = three_platforms();
    std::fs::create_dir_all(rig.state_dir()).expect("the state folder");
    std::fs::write(rig.state_dir().join("leases.json"), b"not json").expect("a bad file");
    let out = rig.run(&["status", "-Host", "agentspace", "-Platform", "linux/amd64"]);
    let err = out.err();
    assert!(!err.contains("lease state file"), "{err:?}");
    assert!(!err.contains("Leases"), "{err:?}");
    // And the line was run: the platform line and the five lines of the amd64 host's own report.
    assert_eq!(
        err,
        format!(
            "Platform  linux/amd64\n{}{}\n",
            status_lines(THIRD_ID),
            keygen_missing(&rig)
        )
    );
}

// ---- up ----

#[test]
fn up_starts_and_waits_for_the_instance_of_the_host_on_the_platform_it_is_given() {
    for (platform, id) in REACHED {
        let rig = three_platforms();
        let shown = format!("up, -Platform {platform:?}");
        let out = rig.run(&line(&["up"], platform));
        assert_eq!(
            rig.keys(),
            ["ec2-describe-instances", "ec2-describe-instances"],
            "{shown}"
        );
        for c in rig.calls() {
            assert_eq!(c.args, describe_args(id), "{shown}");
        }
        let message = keygen_missing(&rig);
        let cause = message
            .strip_prefix("agentspace: ")
            .and_then(|m| m.strip_suffix('.'))
            .expect("a message that starts agentspace: and ends in a period");
        assert_eq!(
            out.err_lines(),
            [
                format!("Ready: {id}"),
                format!(
                    "agentspace: Could not send the localswim token, because {cause}; the board will not work on the host this session."
                )
            ],
            "{shown}: a signing failure is a push failure: it names its cause and fails nothing"
        );
        assert_eq!(out.code, Some(0), "{shown}: {:?}", out.err());
        assert_eq!(out.out(), "", "{shown}");
        assert!(
            rig.calls_of("ssh").is_empty(),
            "{shown}: the push is skipped when signing failed"
        );
    }
}

#[test]
fn up_starts_a_stopped_host_on_the_platform_and_no_other() {
    let rig = three_platforms();
    rig.instance("stopped");
    let out = rig.run(&[
        "up",
        "-Host",
        "agentspace",
        "-Platform",
        "linux/amd64",
        "-Quiet",
    ]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        rig.calls_of("ec2-start-instances")[0].args,
        aws_args("ec2", "start-instances", &["--instance-ids", THIRD_ID])
    );
    assert_eq!(rig.calls_of("ec2-start-instances").len(), 1);
    for other in [ID, OTHER_ID, WINDOWS_ID] {
        assert!(!any_call_names(&rig, other));
    }
}

// ---- grow ----

#[test]
fn grow_changes_the_volume_of_the_host_on_the_platform_it_is_given() {
    for (platform, id) in REACHED {
        let rig = three_platforms();
        let shown = format!("grow, -Platform {platform:?}");
        let mut args = line(&["grow"], platform);
        args.extend(["-GiB", "4"]);
        let out = rig.run(&args);
        assert_eq!(
            rig.keys(),
            [
                "ec2-describe-instances",
                "ec2-describe-volumes",
                "ec2-modify-volume"
            ],
            "{shown}: the volume's change stands, and no ssh is started once signing has failed"
        );
        assert_eq!(rig.calls()[0].args, describe_args(id), "{shown}");
        assert_eq!(
            rig.calls()[2].args,
            aws_args(
                "ec2",
                "modify-volume",
                &["--volume-id", VOL, "--size", "12"]
            ),
            "{shown}"
        );
        let lines = out.err_lines();
        assert_eq!(
            lines[0],
            format!(
                "Growing {VOL} from 8 to 12 GiB (EBS allows one change per volume every 6 hours)..."
            ),
            "{shown}"
        );
        assert_eq!(
            lines.last().map(String::as_str),
            Some(keygen_missing(&rig).as_str()),
            "{shown}"
        );
        assert_eq!(out.code, Some(1), "{shown}");
        for other in [ID, OTHER_ID, THIRD_ID, WINDOWS_ID] {
            if other != id {
                assert!(
                    !any_call_names(&rig, other),
                    "{shown}: no call names another host's instance"
                );
            }
        }
    }
}

// ---- A project's host on one platform never stands in for its host on another ----

#[test]
fn a_host_on_another_platform_never_stands_in_for_the_one_asked_for() {
    // Each registry lacks one of agentspace's three hosts; the line that names it is refused
    // with that host's own message, and nothing runs or is written.
    let cases: [(&str, Option<&str>, [&str; 2], String); 3] = [
        (
            "no arm64 record",
            None,
            ["linux/amd64", "windows/amd64"],
            no_host("agentspace"),
        ),
        (
            "no amd64 record",
            Some("linux/amd64"),
            ["linux/arm64", "windows/amd64"],
            no_host_on("agentspace", "linux/amd64"),
        ),
        (
            "no windows record",
            Some("windows/amd64"),
            ["linux/arm64", "linux/amd64"],
            no_host_on("agentspace", "windows/amd64"),
        ),
    ];
    for (what, asked, held, message) in cases {
        for command in [
            "up",
            "down",
            "status",
            "grow",
            "deploy",
            "sync",
            "bootstrap",
        ] {
            let rig = Rig::standard();
            let mut records = vec![box_rec(Some(ID))];
            for platform in held {
                let id = if platform == "linux/amd64" {
                    THIRD_ID
                } else {
                    OTHER_ID
                };
                records.push(host_rec_on("agentspace", platform, "ready", Some(id)));
            }
            rig.write_hosts(&records);
            let before = rig.hosts_bytes();
            let out = rig.run(&line(&[command], asked));
            assert_fails_with(&out, &message, &format!("{what}, {command}"));
            assert_eq!(out.err().lines().count(), 1, "{what}, {command}");
            assert!(
                rig.calls().is_empty(),
                "{what}, {command}: no program started and no AWS call made"
            );
            assert_eq!(
                rig.hosts_bytes(),
                before,
                "{what}, {command}: no record added"
            );
            assert!(rig.tmp_files().is_empty(), "{what}, {command}");
            assert!(!rig.signed_anything(), "{what}, {command}");
            assert!(
                !rig.state_dir().join("hosts").exists(),
                "{what}, {command}: no folder made"
            );
        }
    }
}

#[test]
fn a_project_with_a_host_on_no_platform_but_the_box_is_refused_with_v007s_message_unchanged() {
    let rig = Rig::standard();
    let out = rig.run(&["status", "-Host", "nosuch"]);
    assert_fails_with(&out, &no_host("nosuch"), "no -Platform");
    let out = rig.run(&["status", "-Host", "nosuch", "-Platform", "linux/arm64"]);
    assert_fails_with(&out, &no_host("nosuch"), "-Platform linux/arm64");
    assert!(rig.calls().is_empty());
    assert_eq!(rig.hosts_bytes(), None, "a lookup writes nothing");
}

#[test]
fn swimcoachs_host_on_another_platform_is_a_host_like_any_other_and_never_the_box() {
    // Only swimcoach's `linux/arm64` host is the box. Its `linux/amd64` host has its own record and
    // stack, is signed for like any host that is not the box, and with no record is refused.
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec_on("swimcoach", "linux/amd64", "ready", Some(THIRD_ID)),
    ]);
    let out = rig.run(&["down", "-Host", "swimcoach", "-Platform", "linux/amd64"]);
    assert_eq!(
        rig.calls()[0].args,
        aws_args("ec2", "stop-instances", &["--instance-ids", THIRD_ID]),
        "its own instance, never the box's"
    );
    assert_eq!(
        out.err_lines(),
        [format!("Stopping {THIRD_ID}."), keygen_missing(&rig)],
        "it is signed for, as the box never is"
    );
    assert!(!any_call_names(&rig, ID));

    // The box with no record of swimcoach on that platform: nothing stands in for it.
    let rig = Rig::standard();
    rig.write_hosts(&[box_rec(Some(ID))]);
    let out = rig.run(&["status", "-Host", "swimcoach", "-Platform", "linux/amd64"]);
    assert_fails_with(
        &out,
        &no_host_on("swimcoach", "linux/amd64"),
        "swimcoach on linux/amd64, with only the box",
    );
    assert!(rig.calls().is_empty(), "the box's instance is never asked");
}

// ---- -Platform linux/arm64 is no -Platform in every respect ----

#[test]
fn platform_linux_arm64_is_the_same_as_no_platform_in_every_respect() {
    let lines: [Vec<&str>; 4] = [
        vec!["up"],
        vec!["down"],
        vec!["status"],
        vec!["grow", "-GiB", "3"],
    ];
    for plain in lines {
        for spelling in [
            "linux/arm64",
            "LINUX/ARM64",
            "linux/aarch64",
            "linux/arm64/v8",
        ] {
            let mut one_line = plain.clone();
            one_line.extend(["-Host", "agentspace"]);
            let mut two_line = one_line.clone();
            two_line.extend(["-Platform", spelling]);
            let one = three_platforms();
            let two = three_platforms();
            if plain[0] == "grow" {
                one.instance("stopped");
                two.instance("stopped");
            }
            let a = one.run(&one_line);
            let b = two.run(&two_line);
            assert_eq!(b.code, a.code, "{two_line:?}");
            // A message that names the rig's own folder, the CA check's, is read with the folder
            // named as `<root>`, since each rig is made in a folder of its own.
            let scrub = |rig: &Rig, text: String| {
                text.replace(rig.system_root.to_string_lossy().as_ref(), "<root>")
            };
            assert_eq!(
                scrub(&two, b.err()),
                scrub(&one, a.err()),
                "{two_line:?}: the same messages"
            );
            assert_eq!(b.out(), a.out(), "{two_line:?}");
            let calls = |rig: &Rig| -> Vec<(String, Vec<String>)> {
                rig.calls().into_iter().map(|c| (c.key, c.args)).collect()
            };
            assert_eq!(calls(&two), calls(&one), "{two_line:?}: the same calls");
            assert_eq!(two.hosts_bytes(), one.hosts_bytes(), "{two_line:?}");
        }
    }
}
