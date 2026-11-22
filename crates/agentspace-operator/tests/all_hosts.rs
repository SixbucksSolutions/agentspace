//! `status -All` and `down -All` over a registry of several hosts, through the test seam's
//! stand-ins (docs/operator.md: Every host at once: `-All`; status; down; How the operator reaches
//! a host; Calls by command). Each host's part is the command's own steps with that host, after a
//! `Host` line that `-Quiet` does not hide; `-All` never signs a certificate and never writes an
//! alias, so the cost guard (D51) never fails on the CA. `sync -All` is dispatched to the sync
//! crate, and is in `sync_crate_dispatch.rs`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use rig::*;

/// The box, a host being made with no instance yet, a ready host and a failed one that has one.
fn four_hosts() -> Rig {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec("alpha", "creating", None),
        host_rec("agentspace", "ready", Some(OTHER_ID)),
        host_rec("zeta", "failed", Some(THIRD_ID)),
    ]);
    rig
}

fn status_lines(id: &str) -> String {
    format!(
        "Instance  {id}\nState     running\nType      m9g.xlarge\nDisk      8 GiB gp3 ({VOL})\nSSM       online\n"
    )
}

/// What `status -All` says, once, after its last host, when the rig holds no lease state file
/// (operator.md, status -All: a missing file is not "no leases", so no host gets a `Leases` or
/// `Guard` line). It is part of the report, and never changes the exit code. The cases that read
/// the file are in `status_leases.rs`.
fn no_leases_file(rig: &Rig) -> String {
    format!(
        "agentspace: The lease state file {} cannot be read: the file does not exist.\n",
        rig.state_dir().join("leases.json").display()
    )
}

const BOX_LINE: &str = "Host      swimcoach (agentbox), ready\n";
const ALPHA_LINE: &str = "Host      alpha (agentspace-host-alpha), creating\n";
const AGENTSPACE_LINE: &str = "Host      agentspace (agentspace-host-agentspace), ready\n";
const ZETA_LINE: &str = "Host      zeta (agentspace-host-zeta), failed\n";

// ---- status -All ----

#[test]
fn status_all_reports_every_host_in_the_registrys_order_after_its_host_line() {
    let rig = four_hosts();
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.out(), "", "status prints to stderr alone");
    assert_eq!(
        out.err(),
        format!(
            "{BOX_LINE}{}{ALPHA_LINE}Instance  none recorded\n{AGENTSPACE_LINE}{}{ZETA_LINE}{}{}",
            status_lines(ID),
            status_lines(OTHER_ID),
            status_lines(THIRD_ID),
            no_leases_file(&rig)
        )
    );
    // Nine calls, three per host that has an instance, in the registry's order, and none for
    // alpha: no stack call either, since a record that is not ready and has no ID is skipped.
    let keys = rig.keys();
    assert_eq!(
        keys,
        [
            "ec2-describe-instances",
            "ec2-describe-volumes",
            "ssm-describe-instance-information",
            "ec2-describe-instances",
            "ec2-describe-volumes",
            "ssm-describe-instance-information",
            "ec2-describe-instances",
            "ec2-describe-volumes",
            "ssm-describe-instance-information",
        ]
    );
    let calls = rig.calls();
    for (index, id) in [(0, ID), (3, OTHER_ID), (6, THIRD_ID)] {
        assert_eq!(
            calls[index].args,
            aws_args("ec2", "describe-instances", &["--instance-ids", id])
        );
        assert_eq!(
            calls[index + 2].args,
            aws_args(
                "ssm",
                "describe-instance-information",
                &["--filters", &format!("Key=InstanceIds,Values={id}")]
            ),
            "one call never names two instances"
        );
    }
}

#[test]
fn the_host_line_and_every_report_line_print_whatever_quiet_says() {
    // One rig for both runs: the report ends with the lease state file's path, which is the rig's.
    let rig = four_hosts();
    let loud = rig.run(&["status", "-All"]);
    let quiet = rig.run(&["status", "-All", "-Quiet"]);
    assert_eq!(quiet.code, Some(0));
    assert_eq!(quiet.err(), loud.err());
    assert!(
        quiet.err().ends_with(&no_leases_file(&rig)),
        "{:?}",
        quiet.err()
    );
}

#[test]
fn status_all_signs_nothing_and_writes_no_alias_for_any_host() {
    // The rig's SystemRoot holds no ssh-keygen.exe, and no CA exists: a command that signed would
    // fail. D51's check never fails on the CA.
    let rig = four_hosts();
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert!(!rig.signed_anything());
    assert!(!rig.host_dir("agentspace").exists());
    assert!(!rig.host_alias_path("agentspace").exists());
}

#[test]
fn status_all_over_a_missing_registry_is_the_box_alone() {
    let rig = Rig::standard();
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err(),
        format!("{BOX_LINE}{}{}", status_lines(ID), no_leases_file(&rig))
    );
    assert_eq!(
        rig.keys()[0],
        "cloudformation-describe-stacks",
        "a missing instance ID is read from the stack as every command reads it"
    );
    assert_eq!(rig.hosts_bytes(), Some(Rig::hosts_file_box(ID)));
}

#[test]
fn a_ready_record_without_an_instance_id_is_read_from_its_stack_and_a_creating_one_is_skipped() {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec("beta", "ready", None),
        host_rec("alpha", "creating", None),
        host_rec("zeta", "failed", None),
    ]);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err(),
        format!(
            "{BOX_LINE}{}Host      beta (agentspace-host-beta), ready\n{}{ALPHA_LINE}Instance  none recorded\n{ZETA_LINE}Instance  none recorded\n{}",
            status_lines(ID),
            status_lines(ID),
            no_leases_file(&rig)
        )
    );
    let stacks = rig.calls_of("cloudformation-describe-stacks");
    assert_eq!(stacks.len(), 1, "only the ready host's stack is asked");
    assert_eq!(
        stacks[0].args,
        aws_args(
            "cloudformation",
            "describe-stacks",
            &["--stack-name", "agentspace-host-beta"]
        )
    );
    assert_eq!(
        rig.hosts_bytes(),
        Some(hosts_doc(&[
            box_rec(Some(ID)),
            host_rec("beta", "ready", Some(ID)),
            host_rec("alpha", "creating", None),
            host_rec("zeta", "failed", None),
        ])),
        "the ID is written into that record alone; a state is never changed"
    );
}

#[test]
fn a_failure_on_one_host_ends_that_hosts_part_and_the_command_goes_on() {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec("agentspace", "ready", Some(OTHER_ID)),
        host_rec("zeta", "failed", Some(THIRD_ID)),
    ]);
    // The second describe-instances of the whole run is the second host's first call.
    rig.fail_nth(
        "ec2-describe-instances",
        2,
        "An error occurred (InvalidInstanceID.NotFound): no such instance\n",
        254,
    );
    let out = rig.run(&["status", "-All"]);
    assert_eq!(
        out.err(),
        format!(
            "{BOX_LINE}{}{AGENTSPACE_LINE}agentspace: An error occurred (InvalidInstanceID.NotFound): no such instance\n{ZETA_LINE}{}{}agentspace: 1 of 3 hosts failed.\n",
            status_lines(ID),
            status_lines(THIRD_ID),
            no_leases_file(&rig)
        ),
        "the failed host prints its failure line after its Host line, with no partial report, \
         and the report goes on; the lease state file's line comes before the closing count"
    );
    assert_eq!(out.code, Some(1));
}

#[test]
fn the_closing_count_is_of_the_records_and_counts_every_host_that_failed() {
    let rig = four_hosts();
    rig.fail("ec2-describe-volumes", "An error occurred: boom\n", 254);
    let out = rig.run(&["status", "-All"]);
    let err = out.err();
    assert_eq!(out.code, Some(1), "{err:?}");
    assert_eq!(
        err.lines().last(),
        Some("agentspace: 3 of 4 hosts failed."),
        "three hosts had an instance and failed; the skipped one is no failure: {err:?}"
    );
}

#[test]
fn an_expired_sign_in_ends_the_whole_command_at_once() {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec("agentspace", "ready", Some(OTHER_ID)),
        host_rec("zeta", "failed", Some(THIRD_ID)),
    ]);
    rig.fail_nth(
        "ec2-describe-instances",
        2,
        "The SSO session has expired\n",
        255,
    );
    let out = rig.run(&["status", "-All"]);
    let err = out.err();
    assert_eq!(out.code, Some(1), "{err:?}");
    assert_eq!(
        err.lines().last(),
        Some(SIGN_IN),
        "#0034's message is the last line: {err:?}"
    );
    assert!(
        !err.contains("zeta"),
        "no later host is reached: every later AWS call would fail the same way: {err:?}"
    );
}

// ---- down -All ----

#[test]
fn down_all_stops_every_host_that_holds_an_instance_in_the_registrys_order() {
    let rig = four_hosts();
    let out = rig.run(&["down", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.out(), "");
    assert_eq!(
        out.err(),
        format!(
            "{BOX_LINE}Stopping {ID}.\n{ALPHA_LINE}Skipping: no instance is recorded for this host.\n{AGENTSPACE_LINE}Stopping {OTHER_ID}.\n{ZETA_LINE}Stopping {THIRD_ID}.\n"
        )
    );
    assert_eq!(
        rig.keys(),
        [
            "ec2-stop-instances",
            "ec2-stop-instances",
            "ec2-stop-instances"
        ]
    );
    let stopped: Vec<Vec<String>> = rig.calls().into_iter().map(|c| c.args).collect();
    assert_eq!(
        stopped,
        [ID, OTHER_ID, THIRD_ID]
            .iter()
            .map(|id| aws_args("ec2", "stop-instances", &["--instance-ids", id]))
            .collect::<Vec<_>>(),
        "one call per instance, never two in one"
    );
}

#[test]
fn down_all_with_quiet_prints_the_host_lines_alone_and_still_stops_every_host() {
    let rig = four_hosts();
    let out = rig.run(&["down", "-All", "-Quiet"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err(),
        format!("{BOX_LINE}{ALPHA_LINE}{AGENTSPACE_LINE}{ZETA_LINE}"),
        "the Host lines print whatever -Quiet says; the progress lines do not"
    );
    assert_eq!(rig.calls_of("ec2-stop-instances").len(), 3);
}

#[test]
fn down_all_signs_nothing_so_the_cost_guard_never_fails_on_the_ca() {
    let rig = four_hosts();
    let out = rig.run(&["down", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert!(!rig.signed_anything());
    assert!(!rig.host_dir("agentspace").exists());
}

#[test]
fn down_all_reads_a_ready_hosts_missing_instance_id_from_its_stack_and_stops_it() {
    let rig = Rig::standard();
    rig.write_hosts(&[box_rec(Some(ID)), host_rec("beta", "ready", None)]);
    let out = rig.run(&["down", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        rig.keys(),
        [
            "ec2-stop-instances",
            "cloudformation-describe-stacks",
            "ec2-stop-instances"
        ]
    );
    assert_eq!(
        rig.hosts_bytes(),
        Some(hosts_doc(&[
            box_rec(Some(ID)),
            host_rec("beta", "ready", Some(ID))
        ]))
    );
}

#[test]
fn down_all_goes_on_past_a_failed_stop_and_counts_it() {
    let rig = four_hosts();
    rig.fail_nth(
        "ec2-stop-instances",
        2,
        "An error occurred (UnauthorizedOperation): no\n",
        254,
    );
    let out = rig.run(&["down", "-All"]);
    assert_eq!(
        out.err(),
        format!(
            "{BOX_LINE}Stopping {ID}.\n{ALPHA_LINE}Skipping: no instance is recorded for this host.\n{AGENTSPACE_LINE}agentspace: An error occurred (UnauthorizedOperation): no\n{ZETA_LINE}Stopping {THIRD_ID}.\nagentspace: 1 of 4 hosts failed.\n"
        )
    );
    assert_eq!(out.code, Some(1));
    assert_eq!(
        rig.calls_of("ec2-stop-instances").len(),
        3,
        "the next host is still stopped"
    );
}

#[test]
fn a_registry_of_one_host_with_all_is_the_box_alone() {
    let rig = Rig::standard();
    rig.write_box_id(ID);
    let out = rig.run(&["down", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), format!("{BOX_LINE}Stopping {ID}.\n"));
}
