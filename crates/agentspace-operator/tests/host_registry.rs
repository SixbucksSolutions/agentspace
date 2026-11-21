//! The host registry as the operator uses it, through the test seam's stand-ins (docs/operator.md:
//! The host registry, as the operator uses it; Which host a command acts on, From the command
//! line; Laptop files; docs/state.md: The host registry, The instance ID). The registry replaced
//! the instance-ID cache (D45), so this file holds what the cache's cases once showed: a missing
//! file, a file that is not valid, a record without an instance ID, and a leftover
//! `instance-id.json` that nothing reads any more.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use rig::*;

/// Every command that acts on a host through the AWS CLI and no sync crate.
const AWS_COMMANDS: [&str; 4] = ["up", "down", "status", "grow"];

/// The standard rig for a command. `grow` gets a stopped instance, so that it ends at "The
/// filesystem grows on its own at next boot." and does not wait its 10 seconds for an ssh.
fn standard_for(command: &str) -> Rig {
    let rig = Rig::standard();
    if command == "grow" {
        rig.instance("stopped");
    }
    rig
}

fn registry_message(rig: &Rig) -> (String, String) {
    (
        format!(
            "agentspace: The host registry {} cannot be read: ",
            rig.hosts_path().display()
        ),
        ".".to_string(),
    )
}

/// Asserts the one-line failure of a registry that cannot be read, with a reason of any words.
fn assert_unreadable(rig: &Rig, out: &Out, what: &str) {
    let (head, tail) = registry_message(rig);
    let err = out.err();
    assert_eq!(out.code, Some(1), "{what}: exit code; stderr was {err:?}");
    assert_eq!(out.out(), "", "{what}: stdout MUST be empty on a failure");
    assert_eq!(err.lines().count(), 1, "{what}: one line, not {err:?}");
    let line = err.trim_end_matches('\n');
    assert!(
        line.starts_with(&head) && line.ends_with(&tail),
        "{what}: the message names the file and ends in a period: {err:?}"
    );
    assert!(
        line.len() > head.len() + tail.len(),
        "{what}: the message carries a reason: {err:?}"
    );
    assert!(
        rig.calls().is_empty(),
        "{what}: nothing is started and no AWS call is made before the registry is read"
    );
}

// ---- A missing registry ----

#[test]
fn a_missing_registry_reads_as_the_box_and_the_first_change_writes_it_with_the_instance_id() {
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
        rig.hosts_bytes(),
        Some(Rig::hosts_file_box(ID)),
        "the seed's record with the instance ID: UTF-8, compact JSON, no byte order mark, one LF"
    );
    assert!(
        rig.tmp_files().is_empty(),
        "the atomic replace leaves no temporary file"
    );
    assert_eq!(out.err(), format!("Ready: {ID}\n"));
}

#[test]
fn a_missing_registry_is_read_as_the_box_for_every_command_and_written_by_none_that_changes_nothing()
 {
    for command in AWS_COMMANDS {
        let rig = standard_for(command);
        let out = rig.run(&[command]);
        assert_eq!(out.code, Some(0), "{command}: {:?}", out.err());
        assert_eq!(
            rig.hosts_bytes(),
            Some(Rig::hosts_file_box(ID)),
            "{command}"
        );
    }
    // `-Host swimcoach` finds the box's record in the seed too, and writes the same file.
    for command in AWS_COMMANDS {
        let rig = standard_for(command);
        let out = rig.run(&[command, "-Host", "swimcoach"]);
        assert_eq!(out.code, Some(0), "{command}: {:?}", out.err());
        assert_eq!(
            rig.hosts_bytes(),
            Some(Rig::hosts_file_box(ID)),
            "{command} -Host swimcoach"
        );
    }
}

#[test]
fn a_registry_holding_the_box_is_never_rewritten_by_a_command_that_needs_no_change() {
    for command in AWS_COMMANDS {
        let rig = standard_for(command);
        rig.write_box_id(ID);
        let before = rig.hosts_bytes();
        let out = rig.run(&[command]);
        assert_eq!(out.code, Some(0), "{command}: {:?}", out.err());
        assert_eq!(rig.hosts_bytes(), before, "{command}");
        assert!(rig.tmp_files().is_empty(), "{command}");
    }
}

#[test]
fn a_record_that_holds_an_id_is_trusted_over_the_stack_for_every_command() {
    for command in AWS_COMMANDS {
        let rig = standard_for(command);
        rig.write_box_id(OTHER_ID);
        let out = rig.run(&[command]);
        assert_eq!(out.code, Some(0), "{command}: {:?}", out.err());
        assert!(
            !rig.keys()
                .contains(&"cloudformation-describe-stacks".to_string()),
            "{command}: no stack call"
        );
        for c in rig.calls() {
            let named = c.args.iter().any(|a| a.contains(OTHER_ID));
            let volume_only = c.key == "ec2-describe-volumes" || c.key == "ec2-modify-volume";
            assert!(
                named || volume_only,
                "{command}: {} named another id",
                c.key
            );
            assert!(
                !c.args.iter().any(|a| a.contains(ID)),
                "{command}: {} used the stack's ID, not the record's",
                c.key
            );
        }
    }
}

// ---- A registry that cannot be read ----

fn unusable_registries() -> Vec<(&'static str, Vec<u8>)> {
    let doc = |records: &[String]| hosts_doc(records);
    let raw = |text: &str| text.as_bytes().to_vec();
    vec![
        ("empty", Vec::new()),
        ("whitespace", b"  \r\n\t ".to_vec()),
        ("not JSON", raw("not json")),
        ("the prototype's plain id", raw(ID)),
        ("an empty object", raw("{}")),
        ("an array", raw("[]")),
        ("null", raw("null")),
        (
            "a wrong version",
            raw(&format!("{{\"version\":1,\"hosts\":[{}]}}", box_rec(None))),
        ),
        (
            "no version",
            raw(&format!("{{\"hosts\":[{}]}}", box_rec(None))),
        ),
        (
            "hosts that is not an array",
            raw("{\"version\":0,\"hosts\":7}"),
        ),
        ("no box record", doc(&[])),
        (
            "no box record but another host",
            doc(&[host_rec("agentspace", "ready", None)]),
        ),
        ("two box records", doc(&[box_rec(None), box_rec(None)])),
        (
            "the box's stack under another project",
            doc(&[rec("agentspace", "agentbox", "ready", None)]),
        ),
        (
            "two records for one project",
            doc(&[
                box_rec(None),
                host_rec("agentspace", "ready", None),
                host_rec("agentspace", "creating", None),
            ]),
        ),
        (
            "a stack that names another project",
            doc(&[
                box_rec(None),
                rec("agentspace", "agentspace-host-swimcoach", "ready", None),
            ]),
        ),
        (
            "a stack that is neither agentbox nor a host's",
            doc(&[box_rec(None), rec("agentspace", "other", "ready", None)]),
        ),
        (
            "a state of another name",
            doc(&[box_rec(None), host_rec("agentspace", "stopped", None)]),
        ),
        (
            "an instance ID that is too short",
            doc(&[box_rec(Some("i-123"))]),
        ),
        (
            "an upper-case instance ID",
            doc(&[box_rec(Some("i-1234567890ABCDEF0"))]),
        ),
        (
            "an instance ID of null",
            raw(
                r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","instance_id":null}]}"#,
            ),
        ),
        (
            "a member that is not in the schema",
            raw(
                r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","extra":1}]}"#,
            ),
        ),
        (
            "a time that names no real instant",
            raw(
                r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","last_attempt_at":"2026-02-30T00:00:00Z"}]}"#,
            ),
        ),
        (
            "a leap second",
            raw(
                r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","last_attempt_at":"2026-06-30T23:59:60Z"}]}"#,
            ),
        ),
        (
            "a member named twice",
            raw(
                r#"{"version":0,"hosts":[{"project":"swimcoach","project":"swimcoach","stack":"agentbox","state":"ready"}]}"#,
            ),
        ),
        (
            "a project that is not normalized",
            doc(&[
                box_rec(None),
                rec("Agentspace", "agentspace-host-Agentspace", "ready", None),
            ]),
        ),
        ("UTF-16", {
            let mut b = vec![0xFF, 0xFE];
            b.extend(utf16le(&format!(
                "{{\"version\":0,\"hosts\":[{}]}}",
                box_rec(None)
            )));
            b
        }),
        ("bytes that are not UTF-8", {
            let mut b = br#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready"}]}"#
                .to_vec();
            b.push(0xFF);
            b
        }),
    ]
}

#[test]
fn a_registry_that_is_not_valid_fails_every_command_that_acts_on_a_host_and_is_never_repaired() {
    for (what, bytes) in unusable_registries() {
        for command in AWS_COMMANDS {
            let rig = standard_for(command);
            rig.write_hosts_bytes(&bytes);
            let out = rig.run(&[command]);
            assert_unreadable(&rig, &out, &format!("{command} with {what}"));
            assert_eq!(
                rig.hosts_bytes(),
                Some(bytes.clone()),
                "{command} with {what}: never read as missing, never repaired"
            );
            assert!(rig.tmp_files().is_empty(), "{command} with {what}");
        }
    }
}

#[test]
fn a_registry_that_is_not_valid_fails_a_command_that_names_a_host_or_every_host() {
    let bytes = b"not json".to_vec();
    for line in [
        vec!["up", "-Host", "agentspace"],
        vec!["down", "-Host", "swimcoach"],
        vec!["status", "-Host", "agentspace"],
        vec!["grow", "-Host", "agentspace"],
        vec!["status", "-All"],
        vec!["down", "-All"],
        vec!["deploy"],
        vec!["deploy", "-Host", "agentspace"],
        vec!["sync"],
        vec!["sync", "-Host", "agentspace"],
        vec!["bootstrap"],
        vec!["bootstrap", "-Host", "agentspace"],
    ] {
        let rig = Rig::standard();
        rig.write_hosts_bytes(&bytes);
        let out = rig.run(&line);
        let what = format!("{line:?}");
        let (head, _) = registry_message(&rig);
        let err = out.err();
        assert_eq!(out.code, Some(1), "{what}: {err:?}");
        assert_eq!(out.out(), "", "{what}");
        assert!(
            err.starts_with(&head),
            "{what}: the registry's message comes first and with no Host line: {err:?}"
        );
        assert!(rig.calls().is_empty(), "{what}: no program started");
        assert_eq!(rig.hosts_bytes(), Some(bytes.clone()), "{what}");
    }
}

#[test]
fn ca_never_reads_the_registry() {
    // `ca` makes no AWS call and touches no host (operator.md, ca): it finds the box's
    // ssh-keygen missing in the rig's empty SystemRoot, whatever the registry holds.
    let rig = Rig::standard();
    rig.write_hosts_bytes(b"not json");
    let out = rig.run(&["ca"]);
    assert_eq!(out.code, Some(1));
    assert!(
        out.err().contains("ssh-keygen.exe is missing"),
        "{:?}",
        out.err()
    );
    assert!(!out.err().contains("host registry"), "{:?}", out.err());
}

#[test]
fn a_registry_with_a_byte_order_mark_and_whitespace_is_read() {
    for command in AWS_COMMANDS {
        let rig = standard_for(command);
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(
            format!(
                "\r\n {{ \"version\" : 0 ,\n \"hosts\" : [ {{ \"project\" : \"swimcoach\", \"stack\" : \"agentbox\", \"state\" : \"ready\", \"instance_id\" : \"{OTHER_ID}\" }} ] }}\n"
            )
            .as_bytes(),
        );
        rig.write_hosts_bytes(&bytes);
        let out = rig.run(&[command]);
        assert_eq!(out.code, Some(0), "{command}: {:?}", out.err());
        assert!(
            !rig.keys()
                .contains(&"cloudformation-describe-stacks".to_string()),
            "{command}: a readable record means no stack call"
        );
        assert_eq!(rig.hosts_bytes(), Some(bytes), "{command}: left as it was");
    }
}

#[test]
fn something_at_the_registrys_path_that_is_not_a_file_is_not_a_missing_file() {
    for command in AWS_COMMANDS {
        let rig = standard_for(command);
        std::fs::create_dir_all(rig.hosts_path()).expect("a folder at the registry's path");
        let out = rig.run(&[command]);
        assert_unreadable(&rig, &out, command);
        assert!(
            rig.hosts_path().is_dir(),
            "{command}: the folder is left as it was"
        );
    }
}

// ---- A record without an instance ID ----

#[test]
fn a_stack_with_no_instance_id_output_fails_and_writes_no_registry() {
    let answers = [
        r#"{"Stacks":[{"Outputs":[{"OutputKey":"Other","OutputValue":"x"}]}]}"#,
        r#"{"Stacks":[{}]}"#,
        r#"{"Stacks":[]}"#,
        r#"{}"#,
        r#"{"Stacks":[{"Outputs":[{"OutputKey":"InstanceId","OutputValue":"not an id"}]}]}"#,
        r#"{"Stacks":[{"Outputs":[{"OutputKey":"InstanceId","OutputValue":7}]}]}"#,
    ];
    for answer in answers {
        for command in AWS_COMMANDS {
            let rig = standard_for(command);
            rig.ok("cloudformation-describe-stacks", answer);
            let out = rig.run(&[command]);
            assert_fails_with(
                &out,
                "agentspace: Stack 'agentbox' has no InstanceId output.",
                &format!("{command} with {answer}"),
            );
            assert_eq!(
                rig.keys(),
                ["cloudformation-describe-stacks"],
                "{command}: nothing runs after the failure"
            );
            assert_eq!(rig.hosts_bytes(), None, "{command}: no registry is written");
        }
    }
}

#[test]
fn a_hosts_stack_with_no_instance_id_output_fails_by_its_own_name_and_changes_nothing() {
    for command in AWS_COMMANDS {
        let rig = standard_for(command);
        rig.write_hosts(&[box_rec(Some(ID)), host_rec("agentspace", "ready", None)]);
        let before = rig.hosts_bytes();
        rig.ok("cloudformation-describe-stacks", r#"{"Stacks":[{}]}"#);
        let out = rig.run(&[command, "-Host", "agentspace"]);
        assert_fails_with(
            &out,
            "agentspace: Stack 'agentspace-host-agentspace' has no InstanceId output.",
            command,
        );
        assert_eq!(rig.keys(), ["cloudformation-describe-stacks"], "{command}");
        assert_eq!(
            rig.calls()[0].args,
            aws_args(
                "cloudformation",
                "describe-stacks",
                &["--stack-name", "agentspace-host-agentspace"]
            ),
            "{command}"
        );
        assert_eq!(rig.hosts_bytes(), before, "{command}");
    }
}

#[test]
fn a_record_without_an_instance_id_gets_only_that_member_and_every_other_record_is_left_as_read() {
    let other = r#"{"project":"zeta","stack":"agentspace-host-zeta","state":"creating","last_attempt_at":"2026-11-20T10:00:00Z"}"#;
    let ready = host_rec("agentspace", "ready", Some(OTHER_ID));
    let rig = Rig::standard();
    rig.write_hosts(&[
        rec("swimcoach", "agentbox", "ready", None),
        ready.clone(),
        other.to_string(),
    ]);
    let out = rig.run(&["down"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        rig.keys(),
        ["cloudformation-describe-stacks", "ec2-stop-instances"]
    );
    assert_eq!(
        rig.hosts_bytes(),
        Some(hosts_doc(&[box_rec(Some(ID)), ready, other.to_string()])),
        "the box's record gains its instance_id after its state, and nothing else changes: same order, same members"
    );
}

#[test]
fn a_hosts_record_without_an_instance_id_is_filled_from_its_own_stack_by_a_command_that_names_it() {
    // The signing that every `-Host` command then does fails in the rig's empty SystemRoot, after
    // the stop; what this shows is the call and the one write.
    let rig = Rig::standard();
    rig.write_hosts(&[box_rec(None), host_rec("agentspace", "creating", None)]);
    let out = rig.run(&["down", "-Host", "agentspace"]);
    assert_eq!(
        rig.keys(),
        ["cloudformation-describe-stacks", "ec2-stop-instances"],
        "stderr was {:?}",
        out.err()
    );
    assert_eq!(
        rig.calls()[0].args,
        aws_args(
            "cloudformation",
            "describe-stacks",
            &["--stack-name", "agentspace-host-agentspace"]
        )
    );
    assert_eq!(
        rig.hosts_bytes(),
        Some(hosts_doc(&[
            box_rec(None),
            host_rec("agentspace", "creating", Some(ID))
        ])),
        "only that record's instance_id; its state is never changed by a command but provision"
    );
}

// ---- The retired cache ----

#[test]
fn a_leftover_instance_id_cache_is_ignored_and_left_as_it_was() {
    for command in AWS_COMMANDS {
        // With no registry the ID comes from the stack, never from the file.
        let rig = standard_for(command);
        rig.write_leftover_cache(OTHER_ID);
        let before = rig.leftover_cache_bytes();
        let out = rig.run(&[command]);
        assert_eq!(out.code, Some(0), "{command}: {:?}", out.err());
        assert_eq!(
            rig.keys().first().map(String::as_str),
            Some("cloudformation-describe-stacks"),
            "{command}: the stack is asked, since the registry is missing"
        );
        for c in rig.calls() {
            assert!(
                !c.args.iter().any(|a| a.contains(OTHER_ID)),
                "{command}: {} used the cache's ID",
                c.key
            );
        }
        assert_eq!(
            rig.hosts_bytes(),
            Some(Rig::hosts_file_box(ID)),
            "{command}"
        );
        assert_eq!(
            rig.leftover_cache_bytes(),
            before,
            "{command}: left as it was"
        );

        // With a registry the record's ID is used, and the file is still left alone.
        let rig = standard_for(command);
        rig.write_leftover_cache(OTHER_ID);
        rig.write_box_id(ID);
        let before = rig.leftover_cache_bytes();
        let out = rig.run(&[command]);
        assert_eq!(out.code, Some(0), "{command}: {:?}", out.err());
        assert!(
            !rig.keys()
                .contains(&"cloudformation-describe-stacks".to_string()),
            "{command}"
        );
        assert_eq!(rig.leftover_cache_bytes(), before, "{command}");
    }
}

#[test]
fn a_leftover_cache_that_cannot_be_read_fails_nothing() {
    // v0.06's "cannot read the instance-ID cache" failure is gone with the cache (D45).
    for command in AWS_COMMANDS {
        let rig = standard_for(command);
        std::fs::create_dir_all(rig.leftover_cache_path()).expect("a folder at the old path");
        let out = rig.run(&[command]);
        assert_eq!(out.code, Some(0), "{command}: {:?}", out.err());
        assert!(rig.leftover_cache_path().is_dir(), "{command}");
    }
}
