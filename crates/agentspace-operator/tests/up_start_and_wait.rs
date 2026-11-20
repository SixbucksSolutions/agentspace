//! `up`: the instance-ID cache, and the start-and-wait step, through the test seam's stand-ins
//! (docs/operator.md: up, Laptop files, Starting the box and waiting for it, Calls by command,
//! AWS failures, What is read from each answer). No push is due in any test here: there is no
//! localswim receipt in the throwaway `LOCALAPPDATA`, so the push is skipped without a message.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use rig::*;

fn describe_args() -> Vec<String> {
    aws_args("ec2", "describe-instances", &["--instance-ids", ID])
}

fn ssm_describe_args() -> Vec<String> {
    aws_args(
        "ssm",
        "describe-instance-information",
        &["--filters", &format!("Key=InstanceIds,Values={ID}")],
    )
}

/// The stack's answers and a cache that names the box, then `up`.
fn cached_rig() -> Rig {
    let rig = Rig::standard();
    rig.write_cache(ID);
    rig
}

// ---- The happy path, and what it asks ----

#[test]
fn an_old_running_box_is_asked_its_state_twice_and_then_it_is_ready() {
    let rig = cached_rig();
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "stderr {:?}", out.err());
    assert_eq!(out.out(), "", "up writes nothing to stdout");
    assert_eq!(out.err(), format!("Ready: {ID}\n"));
    let calls = rig.calls();
    assert_eq!(
        rig.keys(),
        ["ec2-describe-instances", "ec2-describe-instances"],
        "the state, then the launch time; no wait, no start, no stack"
    );
    for c in &calls {
        assert_eq!(c.args, describe_args());
        assert_eq!(c.role, "aws");
    }
}

#[test]
fn the_second_describe_is_for_the_launch_time_and_runs_only_for_a_running_box() {
    // FB-09: a box that was not `running` waits without the second describe.
    for state in ["stopped", "pending"] {
        let rig = cached_rig();
        rig.instance(state);
        let out = rig.run(&["up"]);
        assert_eq!(out.code, Some(0), "{state}: {:?}", out.err());
        let describes = rig.calls_of("ec2-describe-instances").len();
        assert_eq!(describes, 1, "{state}: one describe-instances only");
    }
}

#[test]
fn a_cache_that_holds_an_id_is_trusted_over_the_stack() {
    let rig = cached_rig();
    rig.write_cache(OTHER_ID);
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), format!("Ready: {OTHER_ID}\n"));
    assert!(
        !rig.keys()
            .contains(&"cloudformation-describe-stacks".to_string()),
        "a good cache means no stack call"
    );
    for c in rig.calls() {
        assert!(c.args.contains(&OTHER_ID.to_string()), "{:?}", c.key);
    }
}

// ---- The cache ----

#[test]
fn a_missing_cache_is_rebuilt_from_the_stacks_output_and_written_exactly() {
    let rig = Rig::standard();
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        rig.keys().first().map(String::as_str),
        Some("cloudformation-describe-stacks")
    );
    assert_eq!(
        rig.calls()[0].args,
        aws_args(
            "cloudformation",
            "describe-stacks",
            &["--stack-name", "agentbox"]
        )
    );
    assert_eq!(
        rig.cache_bytes(),
        Some(Rig::cache_file(ID)),
        "UTF-8, compact JSON, no byte order mark, no line ending (state.md)"
    );
    assert!(
        rig.tmp_files().is_empty(),
        "the atomic replace leaves no temporary file"
    );
    assert_eq!(out.err(), format!("Ready: {ID}\n"));
}

#[test]
fn every_kind_of_unusable_cache_counts_as_missing_and_is_rebuilt() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("whitespace", b"  \r\n\t ".to_vec()),
        ("not JSON", b"not json".to_vec()),
        ("the prototype's plain id", ID.as_bytes().to_vec()),
        ("an empty object", b"{}".to_vec()),
        ("an array", b"[]".to_vec()),
        ("null", b"null".to_vec()),
        (
            "a wrong version",
            br#"{"version":1,"instance_id":"i-1234567890abcdef0"}"#.to_vec(),
        ),
        (
            "no version",
            br#"{"instance_id":"i-1234567890abcdef0"}"#.to_vec(),
        ),
        (
            "a short id",
            br#"{"version":0,"instance_id":"i-123"}"#.to_vec(),
        ),
        (
            "an upper-case id",
            br#"{"version":0,"instance_id":"i-1234567890ABCDEF0"}"#.to_vec(),
        ),
        (
            "an id with a trailing line feed",
            b"{\"version\":0,\"instance_id\":\"i-1234567890abcdef0\\n\"}".to_vec(),
        ),
        (
            "an id that is not a string",
            br#"{"version":0,"instance_id":7}"#.to_vec(),
        ),
        (
            "an extra member",
            br#"{"version":0,"instance_id":"i-1234567890abcdef0","extra":1}"#.to_vec(),
        ),
        ("UTF-16", {
            let mut b = vec![0xFF, 0xFE];
            b.extend(utf16le(
                r#"{"version":0,"instance_id":"i-1234567890abcdef0"}"#,
            ));
            b
        }),
    ];
    for (what, bytes) in cases {
        let rig = Rig::standard();
        rig.write_cache_bytes(&bytes);
        let out = rig.run(&["up"]);
        assert_eq!(out.code, Some(0), "{what}: stderr {:?}", out.err());
        assert_eq!(
            rig.keys().first().map(String::as_str),
            Some("cloudformation-describe-stacks"),
            "{what}: treated as missing, so the stack is asked"
        );
        assert_eq!(
            rig.cache_bytes(),
            Some(Rig::cache_file(ID)),
            "{what}: rebuilt"
        );
        assert!(rig.tmp_files().is_empty(), "{what}");
    }
}

#[test]
fn a_cache_with_a_byte_order_mark_and_whitespace_is_read() {
    let rig = Rig::standard();
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(
        format!("\r\n {{ \"version\" : 0 ,\n \"instance_id\" : \"{OTHER_ID}\" }}\n").as_bytes(),
    );
    rig.write_cache_bytes(&bytes);
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert!(
        !rig.keys()
            .contains(&"cloudformation-describe-stacks".to_string()),
        "a readable cache means no stack call"
    );
    assert_eq!(out.err(), format!("Ready: {OTHER_ID}\n"));
}

#[test]
fn a_cache_that_cannot_be_opened_fails_every_command_with_fa_16s_message() {
    for command in [vec!["up"], vec!["down"], vec!["status"], vec!["grow"]] {
        let rig = Rig::standard();
        std::fs::create_dir_all(rig.cache_path()).expect("a folder at the cache's path");
        let out = rig.run(&command);
        let message = format!(
            "agentspace: Cannot read the instance-ID cache {}; remove it and run the command again.",
            rig.cache_path().display()
        );
        assert_fails_with(&out, &message, &format!("{command:?}"));
        assert_eq!(out.err().lines().count(), 1, "{command:?}");
        assert!(
            rig.calls().is_empty(),
            "{command:?}: nothing is started before the cache is read"
        );
        assert!(
            rig.cache_path().is_dir(),
            "{command:?}: the folder is left as it was"
        );
    }
}

#[test]
fn a_stack_with_no_instance_id_output_fails_and_writes_no_cache() {
    let answers = [
        r#"{"Stacks":[{"Outputs":[{"OutputKey":"Other","OutputValue":"x"}]}]}"#,
        r#"{"Stacks":[{}]}"#,
        r#"{"Stacks":[]}"#,
        r#"{}"#,
        r#"{"Stacks":[{"Outputs":[{"OutputKey":"InstanceId","OutputValue":"not an id"}]}]}"#,
        r#"{"Stacks":[{"Outputs":[{"OutputKey":"InstanceId","OutputValue":7}]}]}"#,
    ];
    for answer in answers {
        for command in [vec!["up"], vec!["down"], vec!["status"], vec!["grow"]] {
            let rig = Rig::standard();
            rig.ok("cloudformation-describe-stacks", answer);
            let out = rig.run(&command);
            assert_fails_with(
                &out,
                "agentspace: Stack 'agentbox' has no InstanceId output.",
                &format!("{command:?} with {answer}"),
            );
            assert_eq!(
                rig.keys(),
                ["cloudformation-describe-stacks"],
                "{command:?}: nothing runs after the failure"
            );
            assert_eq!(rig.cache_bytes(), None, "{command:?}: no cache is written");
        }
    }
}

// ---- Starting the box ----

#[test]
fn a_stopped_box_is_started_and_then_waited_for() {
    let rig = cached_rig();
    rig.instance("stopped");
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.out(), "");
    assert_eq!(out.err(), format!("Starting {ID}...\nReady: {ID}\n"));
    assert_eq!(
        rig.keys(),
        [
            "ec2-describe-instances",
            "ec2-start-instances",
            "ssm-describe-instance-information",
            "ssm-send-command",
            "ssm-get-command-invocation",
        ]
    );
    let calls = rig.calls();
    assert_eq!(
        calls[1].args,
        aws_args("ec2", "start-instances", &["--instance-ids", ID])
    );
    assert_eq!(calls[2].args, ssm_describe_args());
}

#[test]
fn the_patch_question_goes_through_run_command_with_its_comment() {
    let rig = cached_rig();
    rig.instance("stopped");
    assert_eq!(rig.run(&["up"]).code, Some(0));
    let send = &rig.calls_of("ssm-send-command")[0];
    let head = aws_args(
        "ssm",
        "send-command",
        &[
            "--instance-ids",
            ID,
            "--document-name",
            "AWS-RunShellScript",
            "--comment",
            "agentspace up: patch check",
            "--parameters",
        ],
    );
    assert_eq!(&send.args[..head.len()], head.as_slice());
    assert_eq!(
        send.args.len(),
        head.len() + 1,
        "one --parameters value, and nothing after it"
    );
    assert!(
        send.args[head.len()].starts_with("commands="),
        "the parameter is commands=<question>"
    );
    let poll = &rig.calls_of("ssm-get-command-invocation")[0];
    assert_eq!(
        poll.args,
        aws_args(
            "ssm",
            "get-command-invocation",
            &["--command-id", "cmd-1234", "--instance-id", ID]
        )
    );
}

#[test]
fn a_pending_box_is_not_started_and_is_waited_for() {
    let rig = cached_rig();
    rig.instance("pending");
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), format!("Ready: {ID}\n"));
    assert!(!rig.keys().contains(&"ec2-start-instances".to_string()));
    assert!(
        rig.keys()
            .contains(&"ssm-describe-instance-information".to_string())
    );
}

#[test]
fn a_stopping_box_is_waited_on_without_checking_the_waits_exit_code_and_then_started() {
    let rig = cached_rig();
    rig.instance("stopping");
    rig.answer(
        "ec2-wait",
        None,
        None,
        Some("WAIT-NOISE-FROM-THE-CLI\n"),
        Some(255),
    );
    let out = rig.run(&["up"]);
    assert_eq!(
        out.code,
        Some(0),
        "a failed wait is not the failure: {:?}",
        out.err()
    );
    assert_eq!(
        out.err_lines(),
        [
            format!("Waiting for {ID} to finish stopping..."),
            "WAIT-NOISE-FROM-THE-CLI".to_string(),
            format!("Starting {ID}..."),
            format!("Ready: {ID}"),
        ],
        "the wait's own stderr passes through in a person's up"
    );
    assert_eq!(
        &rig.keys()[..3],
        ["ec2-describe-instances", "ec2-wait", "ec2-start-instances"]
    );
    // FB-07's order, the operation being `wait`: the service, the operation, `--region us-west-2`,
    // then the operation's own arguments, and no `--output json` (operator.md, AWS failures).
    let wait = &rig.calls_of("ec2-wait")[0];
    assert_eq!(
        wait.args,
        [
            "ec2",
            "wait",
            "--region",
            "us-west-2",
            "instance-stopped",
            "--instance-ids",
            ID
        ]
    );
}

#[test]
fn when_the_wait_gave_up_the_start_request_is_the_check() {
    let rig = cached_rig();
    rig.instance("stopping");
    rig.fail("ec2-wait", "Waiter timed out\n", 255);
    rig.fail(
        "ec2-start-instances",
        "An error occurred (IncorrectInstanceState): cannot start\n",
        254,
    );
    let out = rig.run(&["up"]);
    assert_fails_with(
        &out,
        "agentspace: An error occurred (IncorrectInstanceState): cannot start",
        "start after a wait that gave up",
    );
}

#[test]
fn any_other_state_fails_with_its_name() {
    for state in ["shutting-down", "terminated"] {
        let rig = cached_rig();
        rig.instance(state);
        let out = rig.run(&["up"]);
        assert_fails_with(
            &out,
            &format!("agentspace: Instance {ID} is {state}."),
            state,
        );
        assert_eq!(
            rig.keys(),
            ["ec2-describe-instances"],
            "{state}: nothing after it"
        );
    }
}

#[test]
fn a_failure_ends_the_command_at_once() {
    let rig = cached_rig();
    rig.instance("stopped");
    rig.fail("ec2-start-instances", "Something broke\n", 1);
    let out = rig.run(&["up"]);
    assert_fails_with(&out, "agentspace: Something broke", "a failed start");
    assert_eq!(
        rig.keys(),
        ["ec2-describe-instances", "ec2-start-instances"],
        "nothing runs after the failed call"
    );
    assert_eq!(
        out.err(),
        format!("Starting {ID}...\nagentspace: Something broke\n")
    );
}

#[test]
fn an_answer_of_start_instances_that_is_not_json_does_not_fail_up() {
    // Operator.md, What is read from each answer: `ec2 start-instances`'s answer is not read
    // beyond its exit code, so a CLI that exits 0 with text on stdout still succeeds.
    for answer in ["not json at all\n", ""] {
        let rig = cached_rig();
        rig.instance("stopped");
        rig.ok("ec2-start-instances", answer);
        let out = rig.run(&["up"]);
        assert_eq!(out.code, Some(0), "{answer:?}: {:?}", out.err());
        assert_eq!(
            out.err(),
            format!("Starting {ID}...\nReady: {ID}\n"),
            "{answer:?}"
        );
    }
}

// ---- The 20-minute rule ----

#[test]
fn a_running_box_launched_within_20_minutes_is_waited_for() {
    for seconds_ago in [60, 19 * 60, -3600] {
        let rig = cached_rig();
        rig.ok(
            "ec2-describe-instances",
            &full_instance("running", &iso_ago(seconds_ago)),
        );
        let out = rig.run(&["up"]);
        assert_eq!(out.code, Some(0), "{seconds_ago}: {:?}", out.err());
        assert_eq!(out.err(), format!("Ready: {ID}\n"), "{seconds_ago}");
        assert_eq!(
            rig.keys(),
            [
                "ec2-describe-instances",
                "ec2-describe-instances",
                "ssm-describe-instance-information",
                "ssm-send-command",
                "ssm-get-command-invocation",
            ],
            "launched {seconds_ago} s ago"
        );
    }
}

#[test]
fn a_running_box_launched_20_minutes_ago_or_more_is_not_waited_for() {
    for launch in [
        iso_ago(21 * 60),
        iso_ago(3 * 86_400),
        "2020-06-01T10:00:00-07:00".to_string(),
    ] {
        let rig = cached_rig();
        rig.ok("ec2-describe-instances", &full_instance("running", &launch));
        let out = rig.run(&["up"]);
        assert_eq!(out.code, Some(0), "{launch}: {:?}", out.err());
        assert_eq!(
            rig.keys(),
            ["ec2-describe-instances", "ec2-describe-instances"],
            "{launch}"
        );
    }
}

// ---- What the patch check's answer means ----

#[test]
fn an_answer_other_than_pending_is_ready() {
    // operator.md: `unknown`, an empty answer and an unexpected one count as finished.
    for stdout_content in [
        "{\"Status\":\"Success\",\"StandardOutputContent\":\"unknown\\n\"}",
        "{\"Status\":\"Success\",\"StandardOutputContent\":\"\"}",
        "{\"Status\":\"Failed\",\"StandardOutputContent\":\"something else\\n\"}",
        "{\"Status\":\"Success\"}",
        "{\"Status\":\"Success\",\"StandardOutputContent\":\"  patched \\n\"}",
    ] {
        let rig = cached_rig();
        rig.instance("pending");
        rig.ok("ssm-get-command-invocation", stdout_content);
        let out = rig.run(&["up"]);
        assert_eq!(out.code, Some(0), "{stdout_content}: {:?}", out.err());
        assert_eq!(out.err(), format!("Ready: {ID}\n"), "{stdout_content}");
        assert_eq!(
            rig.calls_of("ssm-get-command-invocation").len(),
            1,
            "{stdout_content}"
        );
    }
}

#[test]
fn an_invocation_that_is_not_readable_yet_or_not_ended_is_polled_again_each_second() {
    let rig = cached_rig();
    rig.instance("pending");
    rig.fail_nth(
        "ssm-get-command-invocation",
        1,
        "InvocationDoesNotExist\n",
        254,
    );
    rig.ok_nth(
        "ssm-get-command-invocation",
        2,
        "{\"Status\":\"InProgress\"}",
    );
    rig.ok_nth("ssm-get-command-invocation", 3, "{\"Status\":\"Delayed\"}");
    rig.ok_nth(
        "ssm-get-command-invocation",
        4,
        "{\"StatusDetails\":\"no Status\"}",
    );
    rig.ok_nth("ssm-get-command-invocation", 5, "{\"Status\":\"Pending\"}");
    rig.ok_nth(
        "ssm-get-command-invocation",
        6,
        &invocation_json("Success", "patched"),
    );
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), format!("Ready: {ID}\n"));
    assert_eq!(rig.calls_of("ssm-get-command-invocation").len(), 6);
    assert_eq!(
        rig.calls_of("ssm-send-command").len(),
        1,
        "the same command is polled, and no new one is sent"
    );
    assert!(
        out.elapsed >= std::time::Duration::from_millis(4500),
        "five pauses of a second between six polls, took {:?}",
        out.elapsed
    );
}

#[test]
fn each_phase_of_the_wait_is_shown_once() {
    // Two checks find the agent offline, then two find the patch run pending, then it is done.
    // The pauses between checks are 5 seconds each, so this takes 15 seconds.
    let rig = cached_rig();
    rig.instance("pending");
    rig.ok_nth(
        "ssm-describe-instance-information",
        1,
        &ssm_json("ConnectionLost"),
    );
    rig.ok_nth(
        "ssm-describe-instance-information",
        2,
        "{\"InstanceInformationList\":[]}",
    );
    rig.ok_nth("ssm-describe-instance-information", 3, &ssm_json("Online"));
    rig.ok_nth("ssm-describe-instance-information", 4, &ssm_json("Online"));
    rig.ok_nth("ssm-describe-instance-information", 5, &ssm_json("Online"));
    rig.ok_nth(
        "ssm-get-command-invocation",
        1,
        &invocation_json("Success", "pending"),
    );
    rig.ok_nth(
        "ssm-get-command-invocation",
        2,
        &invocation_json("Success", "pending"),
    );
    rig.ok_nth(
        "ssm-get-command-invocation",
        3,
        &invocation_json("Success", "patched"),
    );
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err_lines(),
        [
            "Waiting for Session Manager...".to_string(),
            "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...".to_string(),
            format!("Ready: {ID}"),
        ]
    );
    assert_eq!(rig.calls_of("ssm-describe-instance-information").len(), 5);
    assert!(
        out.elapsed >= std::time::Duration::from_secs(14),
        "four pauses of 5 seconds, took {:?}",
        out.elapsed
    );
    assert_eq!(out.out(), "");
}

#[test]
fn a_session_manager_agent_that_is_not_online_is_not_asked_the_patch_question() {
    // A box whose agent is offline once: the question is sent only after the agent is Online.
    let rig = cached_rig();
    rig.instance("pending");
    rig.ok_nth(
        "ssm-describe-instance-information",
        1,
        &ssm_json("ConnectionLost"),
    );
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let keys = rig.keys();
    let first_send = keys
        .iter()
        .position(|k| k == "ssm-send-command")
        .expect("a send");
    let second_describe = keys
        .iter()
        .enumerate()
        .filter(|(_, k)| *k == "ssm-describe-instance-information")
        .nth(1)
        .map(|(i, _)| i)
        .expect("a second check");
    assert!(first_send > second_describe, "{keys:?}");
}

// ---- What is read from each answer ----

#[test]
fn up_reads_only_the_members_it_needs_from_describe_instances() {
    // FB-08: `up` reads State.Name, and LaunchTime for a running box; the others do not fail it.
    let rig = cached_rig();
    rig.ok(
        "ec2-describe-instances",
        &instance_json("running", Some(&old_launch()), None, None, &[]),
    );
    let out = rig.run(&["up"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), format!("Ready: {ID}\n"));
}

#[test]
fn a_member_that_up_reads_and_is_missing_fails_with_its_path() {
    let rig = cached_rig();
    rig.ok(
        "ec2-describe-instances",
        "{\"Reservations\":[{\"Instances\":[{}]}]}",
    );
    let out = rig.run(&["up"]);
    assert_fails_with(
        &out,
        "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].State.Name.",
        "no state",
    );
    for launch in [None, Some("2020-01-01T00:00:00")] {
        let rig = cached_rig();
        rig.ok(
            "ec2-describe-instances",
            &instance_json(
                "running",
                launch,
                Some("m9g.xlarge"),
                Some("/dev/sda1"),
                &[],
            ),
        );
        let out = rig.run(&["up"]);
        assert_fails_with(
            &out,
            "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].LaunchTime.",
            &format!("launch time {launch:?}"),
        );
        assert_eq!(
            rig.keys(),
            ["ec2-describe-instances", "ec2-describe-instances"],
            "the launch time is read by the second call"
        );
    }
}

#[test]
fn an_answer_that_is_not_json_fails_with_the_calls_name() {
    let rig = cached_rig();
    rig.ok("ec2-describe-instances", "this is not json\n");
    let out = rig.run(&["up"]);
    assert_fails_with(
        &out,
        "agentspace: Unexpected answer from ec2 describe-instances: not JSON.",
        "not JSON",
    );
    let rig = Rig::standard();
    rig.ok("cloudformation-describe-stacks", "<html/>");
    let out = rig.run(&["up"]);
    assert_fails_with(
        &out,
        "agentspace: Unexpected answer from cloudformation describe-stacks: not JSON.",
        "stack not JSON",
    );
}

// ---- AWS failures (card #0034) ----

#[test]
fn an_expired_sign_in_gets_agentspaces_own_message_at_every_call_up_makes() {
    let forms = [
        "An error occurred: Your session has expired. Please reauthenticate.\n",
        "Unable to locate credentials. You can configure credentials by running \"aws configure\".\n",
        "Error loading SSO Token: run aws login first\n",
        "TOKEN EXPIRED\n",
        "unable to LOCATE CREDENTIALS\n",
    ];
    for form in forms {
        for (key, no_cache, state) in [
            ("cloudformation-describe-stacks", true, "running"),
            ("ec2-describe-instances", false, "running"),
            ("ec2-start-instances", false, "stopped"),
            ("ssm-describe-instance-information", false, "pending"),
            ("ssm-send-command", false, "pending"),
        ] {
            let rig = Rig::standard();
            if !no_cache {
                rig.write_cache(ID);
            }
            rig.instance(state);
            rig.fail(key, form, 255);
            let out = rig.run(&["up"]);
            assert_fails_with(&out, SIGN_IN, &format!("{key} with {form:?}"));
        }
    }
}

#[test]
fn any_other_failure_gives_the_cli_text_trimmed() {
    for (key, state) in [
        ("ec2-describe-instances", "running"),
        ("ec2-start-instances", "stopped"),
        ("ssm-describe-instance-information", "pending"),
    ] {
        let rig = cached_rig();
        rig.instance(state);
        rig.fail(
            key,
            "\n  An error occurred (Throttling): slow down  \r\n\n",
            254,
        );
        let out = rig.run(&["up"]);
        assert_fails_with(
            &out,
            "agentspace: An error occurred (Throttling): slow down",
            key,
        );
    }
}

#[test]
fn the_error_text_is_stderr_then_stdout_then_the_exit_code() {
    let rig = cached_rig();
    rig.answer(
        "ec2-describe-instances",
        None,
        Some("from stdout\n"),
        Some("\n"),
        Some(2),
    );
    assert_fails_with(
        &rig.run(&["up"]),
        "agentspace: from stdout",
        "stderr blank, stdout text",
    );
    let rig = cached_rig();
    rig.answer("ec2-describe-instances", None, None, None, Some(3));
    assert_fails_with(
        &rig.run(&["up"]),
        "agentspace: The AWS CLI failed with exit code 3.",
        "both empty",
    );
    let rig = cached_rig();
    rig.answer(
        "ec2-describe-instances",
        None,
        Some("on stdout\n"),
        Some("on stderr\n"),
        Some(2),
    );
    assert_fails_with(&rig.run(&["up"]), "agentspace: on stderr", "stderr wins");
}

// ---- -Quiet ----

#[test]
fn quiet_suppresses_progress_and_ready_and_a_failure_still_prints() {
    let rig = cached_rig();
    rig.instance("stopped");
    let out = rig.run(&["up", "-Quiet"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), "", "no Starting, no Ready");
    assert_eq!(out.out(), "");
    assert!(
        rig.keys().contains(&"ec2-start-instances".to_string()),
        "the box is still started"
    );

    let rig = cached_rig();
    rig.instance("stopping");
    let out = rig.run(&["-Quiet", "up"]);
    assert_eq!(out.code, Some(0));
    assert!(
        !out.err().contains("Waiting for") && !out.err().contains("Starting"),
        "{:?}",
        out.err()
    );

    let rig = cached_rig();
    rig.instance("terminated");
    let out = rig.run(&["up", "-Quiet"]);
    assert_fails_with(
        &out,
        &format!("agentspace: Instance {ID} is terminated."),
        "quiet failure",
    );
    assert_eq!(out.err().lines().count(), 1);
}

#[test]
fn quiet_hides_each_wait_message_too() {
    let rig = cached_rig();
    rig.instance("pending");
    rig.ok_nth(
        "ssm-describe-instance-information",
        1,
        &ssm_json("ConnectionLost"),
    );
    let out = rig.op(&["up", "-Quiet"]).run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), "");
}
