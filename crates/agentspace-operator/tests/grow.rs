//! `grow` through the test seam's stand-ins (docs/operator.md: grow, Programs it starts, Calls by
//! command, AWS failures, What is read from each answer). The online step's ssh waits 10 seconds
//! after its message, so each test that reaches it takes a little over that.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use rig::*;

fn cached_rig() -> Rig {
    let rig = Rig::standard();
    rig.write_box_id(ID);
    rig
}

fn growing(from: u32, to: u32) -> String {
    format!(
        "Growing {VOL} from {from} to {to} GiB (EBS allows one change per volume every 6 hours)..."
    )
}

const ONLINE: &str = "Growing the filesystem online...";
const NEXT_BOOT: &str = "The filesystem grows on its own at next boot.";

fn online_failed(new: u32) -> String {
    format!(
        "agentspace: Growing the filesystem online failed; the volume is already {new} GiB, and the filesystem grows on its own at next boot."
    )
}

#[test]
fn a_running_box_gets_its_volume_changed_and_then_one_ssh_after_10_seconds() {
    let rig = cached_rig();
    rig.ok(
        "ec2-modify-volume",
        "{\"VolumeModification\":{\"ModificationState\":\"modifying\"}}\n",
    );
    rig.answer(
        "ssh",
        None,
        Some("Filesystem      Size  Used Avail Use% Mounted on\n/dev/nvme0n1p1   9.7G  5.9G  3.8G  62% /\n"),
        Some("REMOTE-STDERR-LINE\n"),
        Some(0),
    );
    let out = rig.run(&["grow"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.out(),
        "Filesystem      Size  Used Avail Use% Mounted on\n/dev/nvme0n1p1   9.7G  5.9G  3.8G  62% /\n",
        "stdout is the remote step's stdout, and the CLI's answer is not passed through"
    );
    assert_eq!(
        out.err_lines(),
        [
            growing(8, 10),
            ONLINE.to_string(),
            "REMOTE-STDERR-LINE".to_string()
        ],
        "the messages, then the remote step's stderr"
    );
    assert_eq!(
        rig.keys(),
        [
            "ec2-describe-instances",
            "ec2-describe-volumes",
            "ec2-modify-volume",
            "ssh"
        ]
    );
    let calls = rig.calls();
    assert_eq!(
        calls[0].args,
        aws_args("ec2", "describe-instances", &["--instance-ids", ID])
    );
    assert_eq!(
        calls[1].args,
        aws_args("ec2", "describe-volumes", &["--volume-ids", VOL])
    );
    assert_eq!(
        calls[2].args,
        aws_args(
            "ec2",
            "modify-volume",
            &["--volume-id", VOL, "--size", "10"]
        )
    );
    let ssh = &calls[3];
    assert_eq!(
        &ssh.args[..3],
        ["-o", "ClearAllForwardings=yes", "agentspace"],
        "one ssh through the agentspace alias with forwards cleared"
    );
    assert_eq!(
        ssh.args.len(),
        4,
        "the remote command is one argument, the last"
    );
    assert_remote_command_rule(&ssh.args[3], "grow's remote command");
    let lower = ssh.args[3].to_ascii_lowercase();
    assert!(
        !lower.contains("install") && !lower.contains("apt-get") && !lower.contains("apt "),
        "the remote command MUST NOT install anything"
    );
    assert_eq!(ssh.nested, "1");
    assert_eq!(ssh.fd_state, 0);
    let gap = ssh.time_ms.saturating_sub(calls[2].time_ms);
    assert!(
        gap >= 9_800,
        "the ssh starts 10 seconds after the message: {gap} ms after the volume's change"
    );
    assert!(
        out.elapsed >= std::time::Duration::from_secs(10),
        "{:?}",
        out.elapsed
    );
}

#[test]
fn gib_sets_the_growth() {
    let rig = cached_rig();
    rig.ok("ec2-describe-volumes", "{\"Volumes\":[{\"Size\":20}]}");
    let out = rig.run(&["grow", "-GiB", "5"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err_lines(),
        [growing(20, 25), ONLINE.to_string()],
        "the size is the volume's plus -GiB"
    );
    assert_eq!(
        rig.calls_of("ec2-modify-volume")[0].args,
        aws_args(
            "ec2",
            "modify-volume",
            &["--volume-id", VOL, "--size", "25"]
        )
    );
    assert_eq!(rig.calls_of("ssh").len(), 1);
}

#[test]
fn a_gib_of_one_asks_aws_for_the_size_plus_one() {
    // D61, #0164: -GiB 1 is the smallest value `grow` accepts (grow_refusal.rs has the refusals).
    let rig = cached_rig();
    rig.instance("stopped");
    let out = rig.run(&["grow", "-GiB", "1"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err_lines(), [growing(8, 9), NEXT_BOOT.to_string()]);
    assert_eq!(
        rig.calls_of("ec2-modify-volume")[0].args,
        aws_args("ec2", "modify-volume", &["--volume-id", VOL, "--size", "9"])
    );
}

#[test]
fn a_size_that_aws_refuses_for_another_reason_fails_with_the_clis_text() {
    // A positive -GiB is passed to AWS, which may still refuse the change (the 6-hour rule, a
    // volume that is not modifiable); that refusal is the CLI's text, as step 2 says.
    for gib in ["1", "5"] {
        let rig = cached_rig();
        rig.fail(
            "ec2-modify-volume",
            "An error occurred (InvalidParameterValue): not modifiable now\n",
            254,
        );
        let out = rig.run(&["grow", "-GiB", gib]);
        assert_fails_with(
            &out,
            "agentspace: An error occurred (InvalidParameterValue): not modifiable now",
            gib,
        );
        assert_eq!(rig.calls_of("ec2-modify-volume").len(), 1, "-GiB {gib}");
        assert!(
            rig.calls_of("ssh").is_empty(),
            "-GiB {gib}: no ssh after a refusal"
        );
    }
}

#[test]
fn a_box_that_is_not_running_grows_its_filesystem_at_next_boot_and_is_not_started() {
    for state in [
        "stopped",
        "pending",
        "stopping",
        "shutting-down",
        "terminated",
    ] {
        let rig = cached_rig();
        rig.instance(state);
        let out = rig.run(&["grow"]);
        assert_eq!(out.code, Some(0), "{state}: {:?}", out.err());
        assert_eq!(out.out(), "", "{state}");
        assert_eq!(
            out.err_lines(),
            [growing(8, 10), NEXT_BOOT.to_string()],
            "{state}"
        );
        assert_eq!(
            rig.keys(),
            [
                "ec2-describe-instances",
                "ec2-describe-volumes",
                "ec2-modify-volume"
            ],
            "{state}: no ssh, and the box is not started"
        );
        assert!(
            out.elapsed < std::time::Duration::from_secs(9),
            "{state}: no 10-second wait without the online step"
        );
    }
}

#[test]
fn an_online_step_that_fails_fails_grow_and_says_the_volume_stands() {
    for code in [1, 2, 255] {
        let rig = cached_rig();
        rig.answer(
            "ssh",
            None,
            Some("partial output\n"),
            Some("resize2fs: bad\n"),
            Some(code),
        );
        let out = rig.run(&["grow", "-GiB", "3"]);
        assert_eq!(out.code, Some(1), "ssh exit {code}");
        assert_eq!(
            out.err_lines(),
            [
                growing(8, 11),
                ONLINE.to_string(),
                "resize2fs: bad".to_string(),
                online_failed(11),
            ],
            "ssh exit {code}: the remote step's stderr passes through, then the failure"
        );
        assert_eq!(out.out(), "partial output\n", "ssh exit {code}");
        assert_eq!(
            rig.calls_of("ec2-modify-volume").len(),
            1,
            "the volume's change stands"
        );
    }
}

#[test]
fn a_refused_change_fails_with_the_clis_text_and_starts_no_ssh() {
    let rig = cached_rig();
    rig.fail(
        "ec2-modify-volume",
        "An error occurred (VolumeModificationRateExceeded): wait 6 hours\n",
        254,
    );
    let out = rig.run(&["grow"]);
    assert_fails_with(
        &out,
        "agentspace: An error occurred (VolumeModificationRateExceeded): wait 6 hours",
        "second change within 6 hours",
    );
    assert_eq!(
        out.err_lines().len(),
        2,
        "the first message, then the failure: {:?}",
        out.err()
    );
    assert_eq!(out.err_lines()[0], growing(8, 10));
    assert_eq!(
        rig.keys(),
        [
            "ec2-describe-instances",
            "ec2-describe-volumes",
            "ec2-modify-volume"
        ]
    );
}

#[test]
fn an_expired_sign_in_gets_the_sign_in_message_at_each_call() {
    for key in [
        "ec2-describe-instances",
        "ec2-describe-volumes",
        "ec2-modify-volume",
    ] {
        let rig = cached_rig();
        rig.fail(key, "Your session has expired\n", 255);
        assert_fails_with(&rig.run(&["grow"]), SIGN_IN, key);
    }
}

#[test]
fn grow_reads_only_the_members_it_needs() {
    // FB-08: grow reads State.Name, RootDeviceName and the root volume's id; not the type or the
    // launch time.
    let rig = cached_rig();
    rig.ok(
        "ec2-describe-instances",
        &instance_json(
            "stopped",
            None,
            None,
            Some("/dev/sda1"),
            &[("/dev/sda1", VOL)],
        ),
    );
    let out = rig.run(&["grow"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());

    let rig = cached_rig();
    rig.ok(
        "ec2-describe-instances",
        &instance_json(
            "stopped",
            None,
            Some("m9g.xlarge"),
            None,
            &[("/dev/sda1", VOL)],
        ),
    );
    assert_fails_with(
        &rig.run(&["grow"]),
        "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].RootDeviceName.",
        "no root device name",
    );

    let rig = cached_rig();
    rig.ok("ec2-describe-volumes", "{\"Volumes\":[{}]}");
    assert_fails_with(
        &rig.run(&["grow"]),
        "agentspace: Unexpected answer from ec2 describe-volumes: no Volumes[0].Size.",
        "no size",
    );
    assert_eq!(
        rig.keys(),
        ["ec2-describe-instances", "ec2-describe-volumes"],
        "nothing changes before the size is known"
    );
}

#[test]
fn quiet_hides_the_progress_but_not_the_remote_steps_output_nor_a_failure() {
    let rig = cached_rig();
    rig.answer("ssh", None, Some("DF-OUT\n"), Some("DF-ERR\n"), Some(0));
    let out = rig.run(&["grow", "-Quiet"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.out(),
        "DF-OUT\n",
        "output passed through is not affected by -Quiet"
    );
    assert_eq!(out.err(), "DF-ERR\n");
    assert!(
        out.elapsed >= std::time::Duration::from_secs(10),
        "the wait still happens"
    );

    let rig = cached_rig();
    rig.instance("stopped");
    let out = rig.run(&["-Quiet", "grow"]);
    assert_eq!(out.code, Some(0));
    assert_eq!(out.err(), "");
    assert_eq!(out.out(), "");

    let rig = cached_rig();
    rig.answer("ssh", None, None, None, Some(255));
    let out = rig.run(&["grow", "-Quiet"]);
    assert_fails_with(&out, &online_failed(10), "quiet failure");
    assert_eq!(out.err().lines().count(), 1, "{:?}", out.err());
}

#[test]
fn grow_without_a_registry_reads_the_stack_first() {
    let rig = Rig::standard();
    rig.instance("stopped");
    let out = rig.run(&["grow"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(rig.keys()[0], "cloudformation-describe-stacks");
    assert_eq!(rig.hosts_bytes(), Some(Rig::hosts_file_box(ID)));
}

#[test]
fn an_answer_of_modify_volume_that_is_not_json_does_not_fail_grow() {
    // Operator.md, What is read from each answer: `ec2 modify-volume`'s answer is not read
    // beyond its exit code, so a CLI that exits 0 with text on stdout still succeeds.
    for answer in ["not json at all\n", ""] {
        let rig = cached_rig();
        rig.instance("stopped");
        rig.ok("ec2-modify-volume", answer);
        let out = rig.run(&["grow"]);
        assert_eq!(out.code, Some(0), "{answer:?}: {:?}", out.err());
        assert_eq!(
            out.err_lines(),
            [growing(8, 10), NEXT_BOOT.to_string()],
            "{answer:?}"
        );
        assert_eq!(rig.calls_of("ec2-modify-volume").len(), 1, "{answer:?}");
    }
}
