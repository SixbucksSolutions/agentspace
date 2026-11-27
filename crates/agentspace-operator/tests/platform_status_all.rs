//! `status -All`, `down -All` and `sync -All` over a registry that holds hosts of more than one
//! platform: each host named by its host name on its `Host` line, `status -All` printing the
//! platform of each host off `linux/arm64` on a line of its own and still reporting leases as
//! counts, the hosts in the file's order, and `-All` never signing (docs/operator.md: Every host at
//! once; A host's platform and its names; status, status -All; Messages; pins P12, P46 and P57).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use std::fs;

use rig::*;

const AMD64: &str = "linux/amd64";
const WINDOWS: &str = "windows/amd64";

fn five(id: &str, state: &str) -> String {
    let ssm = if state == "running" { "online" } else { "-" };
    format!(
        "Instance  {id}\nState     {state}\nType      m9g.xlarge\nDisk      8 GiB gp3 ({VOL})\nSSM       {ssm}\n"
    )
}

/// A `Host` line: the host's name, its stack and its state, the label padded to 10 characters.
fn host_line(project: &str, platform: &str, stack: &str, state: &str) -> String {
    format!(
        "Host      {} ({stack}), {state}\n",
        host_name(project, platform)
    )
}

fn platform_line(platform: &str) -> String {
    format!("Platform  {platform}\n")
}

fn leases_line(n: usize) -> String {
    format!("Leases    {n} live\n")
}

fn instance_of(rig: &Rig, n: u32, state: &str, launch: Option<i64>) {
    let launch = launch.map_or_else(old_launch, iso_at);
    rig.ok_nth("ec2-describe-instances", n, &full_instance(state, &launch));
}

fn write_leases(rig: &Rig, records: &[String]) {
    fs::create_dir_all(rig.state_dir()).expect("the state folder");
    fs::write(rig.state_dir().join("leases.json"), leases_doc(records))
        .expect("the lease state file");
}

/// The `<h:mm><am|pm>` of the guard's stop for the instant `unix`, as the one formatter makes it.
fn clock_text(unix: i64) -> String {
    agentspace_zone::local_clock_time((unix + 59).div_euclid(60) * 60)
        .expect("the system gives the laptop's zone")
}

// ---- status -All ----

#[test]
fn status_all_prints_each_hosts_name_and_platform_and_counts_its_leases_on_its_platform() {
    // The page's example, with the file's order: the box (stopped), agentspace's `linux/arm64`
    // host under one live lease of its own platform, a host being made with no instance, a
    // `windows/amd64` host that is stopped, and agentspace's `linux/amd64` host, running with
    // only `linux/arm64` leases of its project, so unexplained.
    let now = now_unix();
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec_on("agentspace", "linux/arm64", "ready", Some(OTHER_ID)),
        host_rec_on("example", AMD64, "creating", None),
        host_rec_on("agentspace", WINDOWS, "ready", Some(WINDOWS_ID)),
        host_rec_on("agentspace", AMD64, "ready", Some(THIRD_ID)),
    ]);
    instance_of(&rig, 1, "stopped", None);
    instance_of(&rig, 2, "running", Some(now - 300));
    instance_of(&rig, 3, "stopped", None);
    instance_of(&rig, 4, "running", Some(now - 300));
    write_leases(&rig, &[ready_lease(1, "agentspace")]);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.out(), "", "status prints to stderr alone");
    let guard = format!(
        "Guard     unexplained; stops at {}
",
        clock_text(now - 300 + 900)
    );
    let expected: Vec<String> = vec![
        host_line("swimcoach", "linux/arm64", "agentbox", "ready"),
        five(ID, "stopped"),
        leases_line(0),
        host_line(
            "agentspace",
            "linux/arm64",
            "agentspace-host-agentspace",
            "ready",
        ),
        five(OTHER_ID, "running"),
        leases_line(1),
        host_line(
            "example",
            AMD64,
            "agentspace-linux-amd64-host-example",
            "creating",
        ),
        platform_line(AMD64),
        "Instance  none recorded
"
        .to_string(),
        leases_line(0),
        host_line(
            "agentspace",
            WINDOWS,
            "agentspace-windows-amd64-host-agentspace",
            "ready",
        ),
        platform_line(WINDOWS),
        five(WINDOWS_ID, "stopped"),
        leases_line(0),
        host_line(
            "agentspace",
            AMD64,
            "agentspace-linux-amd64-host-agentspace",
            "ready",
        ),
        platform_line(AMD64),
        five(THIRD_ID, "running"),
        leases_line(0),
        guard,
    ];
    assert_eq!(out.err(), expected.concat());
}

#[test]
fn a_lease_on_the_hosts_platform_explains_it_and_one_on_another_platform_does_not() {
    let now = now_unix();
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec_on("agentspace", "linux/arm64", "ready", Some(OTHER_ID)),
        host_rec_on("agentspace", AMD64, "ready", Some(THIRD_ID)),
    ]);
    instance_of(&rig, 1, "stopped", None);
    instance_of(&rig, 2, "running", Some(now - 300));
    instance_of(&rig, 3, "running", Some(now - 300));
    // One live lease of the project, on `linux/amd64`: it explains the amd64 host, and leaves the
    // arm64 host, which is running, unexplained.
    write_leases(&rig, &[ready_lease_on(1, "agentspace", AMD64)]);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let guard = format!(
        "Guard     unexplained; stops at {}\n",
        clock_text(now - 300 + 900)
    );
    let expected: Vec<String> = vec![
        host_line("swimcoach", "linux/arm64", "agentbox", "ready"),
        five(ID, "stopped"),
        leases_line(0),
        host_line(
            "agentspace",
            "linux/arm64",
            "agentspace-host-agentspace",
            "ready",
        ),
        five(OTHER_ID, "running"),
        leases_line(0),
        guard,
        host_line(
            "agentspace",
            AMD64,
            "agentspace-linux-amd64-host-agentspace",
            "ready",
        ),
        platform_line(AMD64),
        five(THIRD_ID, "running"),
        leases_line(1),
    ];
    assert_eq!(out.err(), expected.concat());
}

#[test]
fn status_all_lists_the_hosts_in_the_files_order_whatever_their_platforms() {
    // A project's `linux/amd64` record before its `linux/arm64` one lists them so (pin P46).
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec_on("agentspace", AMD64, "ready", Some(THIRD_ID)),
        host_rec_on("agentspace", "linux/arm64", "ready", Some(OTHER_ID)),
    ]);
    instance_of(&rig, 1, "stopped", None);
    instance_of(&rig, 2, "stopped", None);
    instance_of(&rig, 3, "stopped", None);
    write_leases(&rig, &[]);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let expected: Vec<String> = vec![
        host_line("swimcoach", "linux/arm64", "agentbox", "ready"),
        five(ID, "stopped"),
        leases_line(0),
        host_line(
            "agentspace",
            AMD64,
            "agentspace-linux-amd64-host-agentspace",
            "ready",
        ),
        platform_line(AMD64),
        five(THIRD_ID, "stopped"),
        leases_line(0),
        host_line(
            "agentspace",
            "linux/arm64",
            "agentspace-host-agentspace",
            "ready",
        ),
        five(OTHER_ID, "stopped"),
        leases_line(0),
    ];
    assert_eq!(out.err(), expected.concat());
    // Each describe-instances call names its host's instance, in the file's order.
    let described: Vec<String> = rig
        .calls_of("ec2-describe-instances")
        .iter()
        .map(|c| c.args.last().cloned().unwrap_or_default())
        .collect();
    assert_eq!(described, [ID, THIRD_ID, OTHER_ID]);
}

#[test]
fn a_linux_arm64_hosts_part_is_v011s_byte_for_byte_with_no_platform_line() {
    // A record that holds `"platform":"linux/arm64"` is read as one that holds none, and the
    // platform line is for a host off `linux/arm64` alone, the box included.
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        format!(
            "{{\"project\":\"agentspace\",\"platform\":\"linux/arm64\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"{OTHER_ID}\"}}"
        ),
        host_rec_on("agentspace", AMD64, "ready", Some(THIRD_ID)),
    ]);
    write_leases(&rig, &[]);
    instance_of(&rig, 1, "stopped", None);
    instance_of(&rig, 2, "stopped", None);
    instance_of(&rig, 3, "stopped", None);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let expected: Vec<String> = vec![
        host_line("swimcoach", "linux/arm64", "agentbox", "ready"),
        five(ID, "stopped"),
        leases_line(0),
        host_line(
            "agentspace",
            "linux/arm64",
            "agentspace-host-agentspace",
            "ready",
        ),
        five(OTHER_ID, "stopped"),
        leases_line(0),
        host_line(
            "agentspace",
            AMD64,
            "agentspace-linux-amd64-host-agentspace",
            "ready",
        ),
        platform_line(AMD64),
        five(THIRD_ID, "stopped"),
        leases_line(0),
    ];
    assert_eq!(out.err(), expected.concat());
}

#[test]
fn the_platform_line_is_part_of_the_report_and_quiet_does_not_hide_it() {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec_on("agentspace", AMD64, "ready", Some(THIRD_ID)),
    ]);
    write_leases(&rig, &[]);
    instance_of(&rig, 1, "stopped", None);
    instance_of(&rig, 2, "stopped", None);
    let loud = rig.run(&["status", "-All"]);
    let rig_quiet = Rig::standard();
    rig_quiet.write_hosts(&[
        box_rec(Some(ID)),
        host_rec_on("agentspace", AMD64, "ready", Some(THIRD_ID)),
    ]);
    write_leases(&rig_quiet, &[]);
    instance_of(&rig_quiet, 1, "stopped", None);
    instance_of(&rig_quiet, 2, "stopped", None);
    let quiet = rig_quiet.run(&["status", "-All", "-Quiet"]);
    assert_eq!(quiet.code, Some(0), "{:?}", quiet.err());
    assert!(
        quiet.err().contains("Platform  linux/amd64\n"),
        "{:?}",
        quiet.err()
    );
    assert_eq!(quiet.err(), loud.err(), "-Quiet changes none of the report");
}

#[test]
fn the_platform_line_comes_right_after_the_host_line_before_a_failure_or_a_missing_instance() {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec_on("agentspace", AMD64, "ready", Some(THIRD_ID)),
        host_rec_on("example", WINDOWS, "failed", None),
    ]);
    instance_of(&rig, 1, "stopped", None);
    // The amd64 host's call fails; the report goes on with the next host.
    rig.fail_nth(
        "ec2-describe-instances",
        2,
        "An error occurred (UnauthorizedOperation) when calling the DescribeInstances operation: Not allowed",
        254,
    );
    write_leases(&rig, &[]);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(1), "{:?}", out.err());
    let lines = out.err_lines();
    let at = lines
        .iter()
        .position(|l| {
            l == "Host      agentspace.linux-amd64 (agentspace-linux-amd64-host-agentspace), ready"
        })
        .expect("the failed host's Host line");
    assert_eq!(lines[at + 1], "Platform  linux/amd64", "{lines:?}");
    assert!(
        lines[at + 2].starts_with("agentspace: "),
        "the failure line follows the platform line: {lines:?}"
    );
    let at = lines
        .iter()
        .position(|l| {
            l == "Host      example.windows-amd64 (agentspace-windows-amd64-host-example), failed"
        })
        .expect("the host with no instance's Host line");
    assert_eq!(
        &lines[at + 1..at + 4],
        [
            "Platform  windows/amd64",
            "Instance  none recorded",
            "Leases    0 live"
        ],
        "{lines:?}"
    );
    assert_eq!(
        lines.last().map(String::as_str),
        Some("agentspace: 1 of 3 hosts failed.")
    );
    // The platform line needs no AWS call: the host with no instance made none.
    assert_eq!(rig.calls_of("ec2-describe-instances").len(), 2);
}

#[test]
fn status_all_reports_leases_as_counts_and_never_names_one() {
    // The project's two live leases are on `linux/amd64`: the amd64 host counts both, and the
    // project's `linux/arm64` host, beside it in the registry, counts none of them.
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec_on("agentspace", "linux/arm64", "ready", Some(OTHER_ID)),
        host_rec_on("agentspace", AMD64, "ready", Some(THIRD_ID)),
    ]);
    instance_of(&rig, 1, "stopped", None);
    instance_of(&rig, 2, "stopped", None);
    instance_of(&rig, 3, "stopped", None);
    write_leases(
        &rig,
        &[
            ready_lease_on(1, "agentspace", AMD64),
            ready_lease_on(2, "agentspace", AMD64),
            ready_lease(3, "swimcoach"),
        ],
    );
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let err = out.err();
    let counts: Vec<&str> = err
        .lines()
        .filter_map(|l| l.strip_prefix("Leases    "))
        .collect();
    assert_eq!(
        counts,
        ["1 live", "0 live", "2 live"],
        "the box's, the arm64 host's and the amd64 host's, each a count: {err:?}"
    );
    for secret in ["5eedc0de", "feed", "agentspace-agentspace-"] {
        assert!(!err.contains(secret), "no lease ID or alias: {err:?}");
    }
}

// ---- down -All and sync -All ----

#[test]
fn down_all_names_each_host_by_its_host_name_stops_each_and_signs_nothing() {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec_on("agentspace", AMD64, "ready", Some(THIRD_ID)),
        host_rec_on("example", "linux/arm64", "creating", None),
        host_rec_on("agentspace", WINDOWS, "ready", Some(WINDOWS_ID)),
    ]);
    let out = rig.run(&["down", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err(),
        format!(
            "{}Stopping {ID}.\n{}Stopping {THIRD_ID}.\n{}Skipping: no instance is recorded for this host.\n{}Stopping {WINDOWS_ID}.\n",
            host_line("swimcoach", "linux/arm64", "agentbox", "ready"),
            host_line(
                "agentspace",
                AMD64,
                "agentspace-linux-amd64-host-agentspace",
                "ready"
            ),
            host_line(
                "example",
                "linux/arm64",
                "agentspace-host-example",
                "creating"
            ),
            host_line(
                "agentspace",
                WINDOWS,
                "agentspace-windows-amd64-host-agentspace",
                "ready"
            ),
        )
    );
    let stopped: Vec<String> = rig
        .calls_of("ec2-stop-instances")
        .iter()
        .map(|c| c.args.last().cloned().unwrap_or_default())
        .collect();
    assert_eq!(stopped, [ID, THIRD_ID, WINDOWS_ID], "the file's order");
    assert!(
        !rig.signed_anything(),
        "down -All never signs or writes an alias"
    );
}
