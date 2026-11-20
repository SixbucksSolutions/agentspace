//! `down` and `status` through the test seam's stand-ins (docs/operator.md: down, status, Laptop
//! files, Calls by command, What is read from each answer, AWS failures).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use rig::*;

fn cached_rig() -> Rig {
    let rig = Rig::standard();
    rig.write_cache(ID);
    rig
}

// ---- down ----

#[test]
fn down_asks_ec2_to_stop_the_instance_and_says_so() {
    let rig = cached_rig();
    rig.ok(
        "ec2-stop-instances",
        "{\"StoppingInstances\":[{\"CurrentState\":{\"Name\":\"stopping\"}}]}\n",
    );
    let out = rig.run(&["down"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.out(),
        "",
        "stdout is empty: the CLI's answer is not the operator's output"
    );
    assert_eq!(out.err(), format!("Stopping {ID}.\n"));
    assert_eq!(
        rig.keys(),
        ["ec2-stop-instances"],
        "the one call, and no wait"
    );
    assert_eq!(
        rig.calls()[0].args,
        aws_args("ec2", "stop-instances", &["--instance-ids", ID])
    );
}

#[test]
fn down_asks_whatever_state_the_instance_is_in_without_reading_it() {
    for state in ["stopped", "running", "pending", "stopping"] {
        let rig = cached_rig();
        rig.instance(state);
        let out = rig.run(&["down"]);
        assert_eq!(out.code, Some(0), "{state}: {:?}", out.err());
        assert_eq!(rig.keys(), ["ec2-stop-instances"], "{state}");
    }
}

#[test]
fn down_without_a_cache_reads_the_stack_and_writes_the_cache() {
    let rig = Rig::standard();
    let out = rig.run(&["down"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        rig.keys(),
        ["cloudformation-describe-stacks", "ec2-stop-instances"]
    );
    assert_eq!(rig.cache_bytes(), Some(Rig::cache_file(ID)));
    assert_eq!(out.err(), format!("Stopping {ID}.\n"));
}

#[test]
fn down_quiet_prints_nothing_but_still_stops_the_box() {
    let rig = cached_rig();
    let out = rig.run(&["down", "-Quiet"]);
    assert_eq!(out.code, Some(0));
    assert_eq!(out.err(), "");
    assert_eq!(out.out(), "");
    assert_eq!(rig.keys(), ["ec2-stop-instances"]);
}

#[test]
fn a_refused_stop_fails_with_the_clis_text_and_never_says_stopping() {
    let rig = cached_rig();
    rig.fail(
        "ec2-stop-instances",
        "An error occurred (UnauthorizedOperation): no\n",
        254,
    );
    let out = rig.run(&["down"]);
    assert_fails_with(
        &out,
        "agentspace: An error occurred (UnauthorizedOperation): no",
        "down",
    );
    assert_eq!(
        out.err().lines().count(),
        1,
        "no Stopping line before the request is accepted"
    );

    let rig = cached_rig();
    rig.fail("ec2-stop-instances", "The SSO session has EXPIRED\n", 255);
    assert_fails_with(
        &rig.run(&["down", "-Quiet"]),
        SIGN_IN,
        "down -Quiet, expired",
    );
}

// ---- status ----

fn status_lines(id: &str, state: &str, itype: &str, size: u32, vol: &str, ssm: &str) -> String {
    format!(
        "Instance  {id}\nState     {state}\nType      {itype}\nDisk      {size} GiB gp3 ({vol})\nSSM       {ssm}\n"
    )
}

#[test]
fn status_prints_five_lines_to_stderr_and_nothing_to_stdout() {
    let rig = cached_rig();
    let out = rig.run(&["status"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.out(), "");
    assert_eq!(
        out.err(),
        status_lines(ID, "running", "m9g.xlarge", 8, VOL, "online")
    );
    assert_eq!(
        rig.keys(),
        [
            "ec2-describe-instances",
            "ec2-describe-volumes",
            "ssm-describe-instance-information"
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
            "ssm",
            "describe-instance-information",
            &["--filters", &format!("Key=InstanceIds,Values={ID}")]
        )
    );
}

#[test]
fn status_prints_whatever_quiet_says() {
    let rig = cached_rig();
    let out = rig.run(&["status", "-Quiet"]);
    assert_eq!(out.code, Some(0));
    assert_eq!(
        out.err(),
        status_lines(ID, "running", "m9g.xlarge", 8, VOL, "online")
    );
}

#[test]
fn status_reads_the_size_and_the_type_from_the_answers() {
    let rig = cached_rig();
    rig.ok(
        "ec2-describe-instances",
        &instance_json(
            "running",
            None,
            Some("m9g.4xlarge"),
            Some("/dev/sda1"),
            &[("/dev/sda1", VOL)],
        ),
    );
    rig.ok(
        "ec2-describe-volumes",
        "{\"Volumes\":[{\"Size\":64,\"VolumeType\":\"io2\"}]}",
    );
    let out = rig.run(&["status"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err(),
        status_lines(ID, "running", "m9g.4xlarge", 64, VOL, "online"),
        "gp3 is a constant; a launch time is not needed"
    );
}

#[test]
fn the_root_volume_is_the_one_at_the_root_device_name() {
    let rig = cached_rig();
    let other = "vol-fedcba9876543210f";
    rig.ok(
        "ec2-describe-instances",
        &instance_json(
            "running",
            None,
            Some("m9g.xlarge"),
            Some("/dev/sda1"),
            &[("/dev/sdb", other), ("/dev/sda1", VOL), ("/dev/sdc", other)],
        ),
    );
    let out = rig.run(&["status"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        rig.calls_of("ec2-describe-volumes")[0].args,
        aws_args("ec2", "describe-volumes", &["--volume-ids", VOL])
    );
    assert!(out.err().contains(&format!("({VOL})")));
}

#[test]
fn ssm_is_not_yet_for_a_running_box_whose_agent_is_not_online() {
    for answer in [
        ssm_json("ConnectionLost"),
        ssm_json("Inactive"),
        "{\"InstanceInformationList\":[]}".to_string(),
    ] {
        let rig = cached_rig();
        rig.ok("ssm-describe-instance-information", &answer);
        let out = rig.run(&["status"]);
        assert_eq!(out.code, Some(0), "{answer}: {:?}", out.err());
        assert_eq!(
            out.err(),
            status_lines(ID, "running", "m9g.xlarge", 8, VOL, "not yet"),
            "{answer}"
        );
    }
}

#[test]
fn ssm_is_a_dash_and_not_asked_in_every_other_state() {
    for state in [
        "pending",
        "stopping",
        "stopped",
        "shutting-down",
        "terminated",
    ] {
        let rig = cached_rig();
        rig.instance(state);
        let out = rig.run(&["status"]);
        assert_eq!(out.code, Some(0), "{state}: {:?}", out.err());
        assert_eq!(
            out.err(),
            status_lines(ID, state, "m9g.xlarge", 8, VOL, "-"),
            "{state}"
        );
        assert!(
            !rig.keys()
                .contains(&"ssm-describe-instance-information".to_string()),
            "{state}: Session Manager is asked only of a running box"
        );
    }
}

#[test]
fn status_never_starts_the_box_or_changes_anything() {
    for state in ["stopped", "stopping", "running"] {
        let rig = cached_rig();
        rig.instance(state);
        let out = rig.run(&["status"]);
        assert_eq!(out.code, Some(0));
        for key in rig.keys() {
            assert!(
                !key.contains("start")
                    && !key.contains("stop")
                    && !key.contains("modify")
                    && key != "ssh",
                "{state}: {key}"
            );
        }
    }
}

#[test]
fn status_without_a_cache_reads_the_stack_first() {
    let rig = Rig::standard();
    let out = rig.run(&["status"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(rig.keys()[0], "cloudformation-describe-stacks");
    assert_eq!(rig.cache_bytes(), Some(Rig::cache_file(ID)));
}

#[test]
fn status_names_the_member_of_an_answer_that_is_missing_or_of_another_form() {
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "no state",
            "{\"Reservations\":[{\"Instances\":[{}]}]}".to_string(),
            "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].State.Name.",
        ),
        (
            "no instance type",
            instance_json(
                "running",
                None,
                None,
                Some("/dev/sda1"),
                &[("/dev/sda1", VOL)],
            ),
            "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].InstanceType.",
        ),
        (
            "no root device name",
            instance_json(
                "running",
                None,
                Some("m9g.xlarge"),
                None,
                &[("/dev/sda1", VOL)],
            ),
            "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].RootDeviceName.",
        ),
        (
            "not JSON",
            "Reservations: none".to_string(),
            "agentspace: Unexpected answer from ec2 describe-instances: not JSON.",
        ),
    ];
    for (what, answer, message) in cases {
        let rig = cached_rig();
        rig.ok("ec2-describe-instances", &answer);
        let out = rig.run(&["status"]);
        assert_fails_with(&out, message, what);
    }
    let rig = cached_rig();
    rig.ok("ec2-describe-volumes", "{\"Volumes\":[{}]}");
    assert_fails_with(
        &rig.run(&["status"]),
        "agentspace: Unexpected answer from ec2 describe-volumes: no Volumes[0].Size.",
        "no size",
    );
    let rig = cached_rig();
    rig.ok("ec2-describe-volumes", "{\"Volumes\":[]}");
    assert_fails_with(
        &rig.run(&["status"]),
        "agentspace: Unexpected answer from ec2 describe-volumes: no Volumes[0].Size.",
        "no volume",
    );
}

#[test]
fn status_failures_are_the_clis_text_or_the_sign_in_message() {
    for key in [
        "ec2-describe-instances",
        "ec2-describe-volumes",
        "ssm-describe-instance-information",
    ] {
        let rig = cached_rig();
        rig.fail(key, "Boom from the CLI\n", 255);
        let out = rig.run(&["status"]);
        assert_fails_with(&out, "agentspace: Boom from the CLI", key);

        let rig = cached_rig();
        rig.fail(key, "Unable to locate credentials.\n", 255);
        assert_fails_with(&rig.run(&["status", "-Quiet"]), SIGN_IN, key);
    }
}
