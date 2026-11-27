//! The guard decides each host by its project and its platform (docs/operator.md, guard, "From
//! v0.12 the guard decides each host by its project and its platform", step 6, step 7 and "Its
//! log"; docs/state.md, "Live leases and the lease horizon" and "A host's platform and names";
//! D91, D96; E0.12-T03-S04, #0257), written from those pages alone.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Every registry here holds two or three hosts of ONE project, `orchard`, one on each platform,
//! so a decision by the project alone explains, or stops, the wrong host. Every run goes through
//! the stand-ins of `run_support`: a recording `Aws`, a recording `Say` and a throwaway home. No
//! test calls real AWS, and none sets a process variable.
//!
//! A Windows host's horizon command is `agentspace-windows`' row (E0.12-T03-S06), which this
//! story's code asks for and does not write. The tests that need a Windows host to be SENT its
//! horizon assert the wiring alone (the document, the JSON value of `--parameters`, the horizon
//! as one word of the command) and none of the row's text. Every test that stops a Windows host
//! sends it nothing, since a stopped host is sent no horizon, so those hold with the row unbuilt.
#![cfg(windows)]

#[allow(unused_imports)]
mod run_support;

use run_support::*;

const WINDOWS: &str = "windows/amd64";
const LINUX_AMD64: &str = "linux/amd64";
const ARM64: &str = "linux/arm64";
const WIN_NAME: &str = "orchard.windows-amd64";
const AMD_NAME: &str = "orchard.linux-amd64";

// ---------------------------------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------------------------------

/// A host record of `orchard` on `platform`, as state.md writes it: no `platform` member on
/// `linux/arm64`, and the stack of D91's table for the others.
fn host_on(platform: &str, state: &str, instance_id: Option<&str>) -> Value {
    let stack = match platform {
        ARM64 => "agentspace-host-orchard".to_string(),
        other => format!("agentspace-{}-host-orchard", other.replace('/', "-")),
    };
    let mut record = json!({"project": "orchard", "stack": stack, "state": state});
    if platform != ARM64 {
        record["platform"] = json!(platform);
    }
    if let Some(instance_id) = instance_id {
        record["instance_id"] = json!(instance_id);
    }
    record
}

/// A lease of `orchard` on `platform`, from one of `run_support`'s builders.
fn on(mut lease: Value, platform: &str) -> Value {
    lease["project"] = json!("orchard");
    lease["platform"] = json!(platform);
    if lease.get("ssh").is_some() {
        lease["ssh"]["user"] = json!(if platform == WINDOWS {
            "agent"
        } else {
            "ubuntu"
        });
    }
    lease
}

fn orchard_ready(platform: &str, expires_at: &str) -> Value {
    on(lease_ready("orchard", expires_at), platform)
}

fn orchard_released(platform: &str, released_at: &str, expires_at: Option<&str>) -> Value {
    on(lease_released("orchard", released_at, expires_at), platform)
}

/// The box (ID `id(0)`, explained by a live `swimcoach` lease), and these records, which the
/// caller gives IDs from `id(1)`.
fn world(name: &str, hosts: &[Value], leases: &[Value]) -> World {
    let w = World::new(name);
    let mut records = vec![box_host(Some(&id(0)))];
    records.extend(hosts.iter().cloned());
    w.write_hosts(&records);
    let mut all = vec![lease_ready("swimcoach", &today("23:00:00"))];
    all.extend(leases.iter().cloned());
    w.write_leases(&all);
    w.aws
        .add_instance(&id(0), "running", Some(&launch("08:00:00")));
    w
}

/// The document a send used, and the `--parameters` value it carried.
fn document_of(rec: &Rec) -> String {
    after(rec, "--document-name")
}

fn parameters_of(rec: &Rec) -> String {
    after(rec, "--parameters")
}

fn after(rec: &Rec, flag: &str) -> String {
    let at = rec
        .args
        .iter()
        .position(|a| a == flag)
        .unwrap_or_else(|| panic!("The call has no {flag}"));
    rec.args[at + 1].clone()
}

fn send_to<'a>(sends: &'a [Rec], instance: &str) -> &'a Rec {
    sends
        .iter()
        .find(|s| s.instance().as_deref() == Some(instance))
        .unwrap_or_else(|| panic!("No horizon was sent to {instance}"))
}

/// The horizon, in Unix seconds, that a Windows send's `{"commands":["..."]}` value carries as
/// one word of its one command.
fn json_horizon(parameters: &str) -> i64 {
    let value: Value = serde_json::from_str(parameters).expect("--parameters is a JSON value");
    let object = value.as_object().expect("--parameters is a JSON object");
    assert_eq!(object.len(), 1, "The object has one member");
    let commands = object["commands"].as_array().expect("commands is an array");
    assert_eq!(commands.len(), 1, "commands holds one command");
    let command = commands[0].as_str().expect("The command is a string");
    let words: Vec<&str> = command
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= 9 && w.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    assert_eq!(words.len(), 1, "The command holds the horizon as one word");
    words[0].parse().unwrap()
}

// ---------------------------------------------------------------------------------------------
// A host is explained by a live lease of its project on its platform (step 6).
// ---------------------------------------------------------------------------------------------

/// D91, operator.md guard: a running Windows host whose project holds only Linux leases is
/// unexplained, and so is stopped once due, while the project's `linux/arm64` host, which a live
/// `linux/arm64` lease explains, is left alone.
#[test]
fn a_running_windows_host_whose_project_holds_only_linux_leases_is_unexplained() {
    let w = world(
        "win-only-linux-leases",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(WINDOWS, "ready", Some(&id(2))),
        ],
        &[orchard_ready(ARM64, &today("23:00:00"))],
    );
    for n in [1, 2] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    let report = w.run_ok(now());
    assert_eq!(w.stopped_ids(), vec![id(2)], "Only the Windows host stops");
    assert_eq!(
        w.rests(),
        vec![format!(
            "host={WIN_NAME} stopped since={}",
            today("08:00:00")
        )]
    );
    assert_eq!((report.running, report.stopped), (3, 1));
}

/// The other way round (D91): a running `linux/arm64` host whose project holds only a live
/// Windows lease is unexplained, and the Windows host is explained.
#[test]
fn a_running_linux_arm64_host_whose_project_holds_only_a_windows_lease_is_unexplained() {
    let w = world(
        "arm-only-win-lease",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(WINDOWS, "ready", Some(&id(2))),
        ],
        &[orchard_ready(WINDOWS, &today("23:00:00"))],
    );
    for n in [1, 2] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    let _ = w.run(now());
    assert_eq!(w.stopped_ids(), vec![id(1)], "Only the Linux host stops");
    assert_eq!(
        w.rests()
            .into_iter()
            .filter(|r| !r.contains("failed"))
            .collect::<Vec<_>>(),
        vec![format!("host=orchard stopped since={}", today("08:00:00"))]
    );
}

/// operator.md guard: a running `linux/amd64` host whose project holds only `linux/arm64` leases
/// is unexplained too; the platform is the whole key, not the OS.
#[test]
fn a_running_linux_amd64_host_whose_project_holds_only_linux_arm64_leases_is_unexplained() {
    let w = world(
        "amd-only-arm-leases",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(LINUX_AMD64, "ready", Some(&id(2))),
        ],
        &[orchard_ready(ARM64, &today("23:00:00"))],
    );
    for n in [1, 2] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    w.run_ok(now());
    assert_eq!(w.stopped_ids(), vec![id(2)]);
    assert_eq!(
        w.rests(),
        vec![format!(
            "host={AMD_NAME} stopped since={}",
            today("08:00:00")
        )]
    );
}

/// Each host is explained by a live lease on its own platform, a `pending` one included, and
/// nothing is stopped or logged for any of the three.
#[test]
fn each_host_is_explained_by_a_live_lease_on_its_own_platform() {
    let w = world(
        "each-explained",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(LINUX_AMD64, "ready", Some(&id(2))),
            host_on(WINDOWS, "ready", Some(&id(3))),
        ],
        &[
            orchard_ready(ARM64, &today("23:00:00")),
            orchard_ready(LINUX_AMD64, &today("23:00:00")),
            on(lease_pending("orchard"), WINDOWS),
        ],
    );
    for n in [1, 2, 3] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    let _ = w.run(now());
    assert!(w.stopped_ids().is_empty(), "No host is stopped");
    assert!(w.rests().iter().all(|r| !r.contains("unexplained")));
    assert!(w.rests().iter().all(|r| !r.contains("stopped")));
}

/// A live lease explains a host by its platform, and an ended lease of its own platform does
/// not: a Windows host whose only Windows lease was released is unexplained, though the project
/// holds a live `linux/arm64` one.
#[test]
fn a_released_lease_on_the_hosts_platform_explains_nothing_beside_a_live_one_on_another() {
    let w = world(
        "released-win",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(WINDOWS, "ready", Some(&id(2))),
        ],
        &[
            orchard_ready(ARM64, &today("23:00:00")),
            orchard_released(WINDOWS, &today("09:00:00"), Some(&today("12:00:00"))),
        ],
    );
    for n in [1, 2] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    w.run_ok(now());
    assert_eq!(w.stopped_ids(), vec![id(2)]);
    // Its since is the later of its launch and its own platform's last lease end, 09:00.
    assert_eq!(
        w.rests(),
        vec![format!(
            "host={WIN_NAME} stopped since={}",
            today("09:00:00")
        )]
    );
}

/// state.md, Live leases: a host's last lease end is its own platform's, so a lease that ended
/// on another platform later moves nothing. The Windows lease ended at 19:30, so the Windows
/// host is due at 19:45, though the project's `linux/arm64` lease ended at 19:55, which a
/// decision over the whole project would take for the since and keep the host until 20:10.
#[test]
fn a_hosts_last_lease_end_is_its_own_platforms() {
    let w = world(
        "own-last-end",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(WINDOWS, "ready", Some(&id(2))),
        ],
        &[
            orchard_released(ARM64, &today("19:55:00"), Some(&today("22:00:00"))),
            orchard_released(WINDOWS, &today("19:30:00"), Some(&today("22:00:00"))),
        ],
    );
    for n in [1, 2] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    w.run_ok(now());
    // The Linux host's last end is 19:55, so it stops at 20:10, after the run's time.
    assert_eq!(w.stopped_ids(), vec![id(2)], "Only the Windows host is due");
    assert_eq!(
        w.rests(),
        vec![
            format!(
                "host=orchard unexplained since={} stops={}",
                today("19:55:00"),
                today("20:10:00")
            ),
            format!("host={WIN_NAME} stopped since={}", today("19:30:00")),
        ]
    );
}

/// A host record that holds `"platform":"linux/arm64"` is the same host's as one without the
/// member (state.md, A host's platform and names), so a `linux/arm64` lease explains it and it
/// is named by its project.
#[test]
fn a_record_holding_linux_arm64_explicitly_is_the_arm64_host() {
    let mut record = host_on(ARM64, "ready", Some(&id(1)));
    record["platform"] = json!(ARM64);
    let w = world(
        "explicit-arm64",
        &[record],
        &[orchard_released(
            ARM64,
            &today("19:00:00"),
            Some(&today("21:00:00")),
        )],
    );
    w.aws
        .add_instance(&id(1), "running", Some(&launch("08:00:00")));
    w.run_ok(now());
    assert_eq!(w.stopped_ids(), vec![id(1)]);
    assert_eq!(
        w.rests(),
        vec![format!("host=orchard stopped since={}", today("19:00:00"))]
    );
}

// ---------------------------------------------------------------------------------------------
// What the guard says and logs, by host name (Messages; Its log).
// ---------------------------------------------------------------------------------------------

/// Step 6 and Messages: a host off `linux/arm64` is named `'<host name>'` in the progress lines,
/// and a `linux/arm64` host's lines are unchanged.
#[test]
fn the_progress_lines_name_a_host_off_arm64_by_its_host_name() {
    let w = world(
        "progress-names",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(WINDOWS, "ready", Some(&id(2))),
            host_on(LINUX_AMD64, "ready", Some(&id(3))),
        ],
        &[orchard_ready(ARM64, &today("23:00:00"))],
    );
    // The Windows host is due; the `linux/amd64` host was launched at 19:50, so it is not.
    w.aws
        .add_instance(&id(1), "running", Some(&launch("08:00:00")));
    w.aws
        .add_instance(&id(2), "running", Some(&launch("08:00:00")));
    w.aws
        .add_instance(&id(3), "running", Some(&launch("19:50:00")));
    w.run_ok(now());
    let lines = w.progress();
    assert_eq!(
        lines[0],
        format!(
            "Stopping the host of '{WIN_NAME}' ({}): no live lease explains it.",
            id(2)
        )
    );
    assert!(
        is_unexplained_progress(&lines[1], AMD_NAME, &id(3)),
        "The unexplained line: {}",
        lines[1]
    );
    assert_eq!(lines.len(), 3);
}

/// Its log: `host=<host name>`, so two hosts of one project never share a line's name, in each
/// kind of line that is not a failure.
#[test]
fn the_logs_host_field_is_the_host_name() {
    let w = world(
        "log-host-field",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(WINDOWS, "ready", Some(&id(2))),
            host_on(LINUX_AMD64, "ready", Some(&id(3))),
        ],
        &[],
    );
    w.aws
        .add_instance(&id(1), "running", Some(&launch("19:00:00")));
    w.aws
        .add_instance(&id(2), "running", Some(&launch("08:00:00")));
    w.aws
        .add_instance(&id(3), "running", Some(&launch("19:50:00")));
    w.run_ok(now());
    assert_eq!(
        w.rests(),
        vec![
            format!("host=orchard stopped since={}", today("19:00:00")),
            format!("host={WIN_NAME} stopped since={}", today("08:00:00")),
            format!(
                "host={AMD_NAME} unexplained since={} stops={}",
                today("19:50:00"),
                today("20:05:00")
            ),
        ]
    );
}

/// A host's part that fails is named by its host name on stderr and in the log, a Windows host's
/// whose instance the answer does not hold included, and the other hosts of the project are
/// decided as they would be.
#[test]
fn a_failed_part_names_the_host_by_its_host_name() {
    let w = world(
        "failed-part-name",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(WINDOWS, "ready", Some(&id(2))),
            host_on(LINUX_AMD64, "ready", Some(&id(3))),
        ],
        &[
            orchard_ready(ARM64, &today("23:00:00")),
            orchard_ready(LINUX_AMD64, &today("23:00:00")),
        ],
    );
    // The Windows host's instance is missing from the answer; the amd64 host's send fails.
    w.aws
        .add_instance(&id(1), "running", Some(&launch("08:00:00")));
    w.aws
        .add_instance(&id(3), "running", Some(&launch("08:00:00")));
    w.aws.fail_send(&id(3), AwsError::Failed("Denied".into()));
    let report = w.run_ok(now());
    assert_eq!(report.failed, 2);
    w.expect_host_failure_warned(WIN_NAME, &format!("Instance {} was not found.", id(2)));
    w.expect_host_failure_warned(
        AMD_NAME,
        &format!("Could not send the lease horizon to {}: Denied", id(3)),
    );
    assert_eq!(
        w.rests(),
        vec![
            format!(
                "host={WIN_NAME} failed step=instance reason=Instance {} was not found.",
                id(2)
            ),
            format!(
                "host={AMD_NAME} failed step=horizon reason=Could not send the lease horizon to {}: Denied",
                id(3)
            ),
        ]
    );
}

// ---------------------------------------------------------------------------------------------
// A host's provisioning lock is its own folder's (step 6).
// ---------------------------------------------------------------------------------------------

/// `hosts\<project>.<os>-<arch>\provision.lock` is a host off `linux/arm64`'s lock: held, it
/// leaves that host, logged `unexplained` with `provisioning=running` and said by its host name.
#[test]
fn a_hosts_own_provisioning_lock_leaves_it() {
    let w = world(
        "own-lock",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(LINUX_AMD64, "ready", Some(&id(2))),
        ],
        &[orchard_ready(ARM64, &today("23:00:00"))],
    );
    for n in [1, 2] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    let _lock = hold(&w.provision_lock(AMD_NAME));
    w.run_ok(now());
    assert!(w.stopped_ids().is_empty(), "The held host is left");
    assert_eq!(
        w.rests(),
        vec![format!(
            "host={AMD_NAME} unexplained since={} stops={} provisioning=running",
            today("08:00:00"),
            today("08:15:00")
        )]
    );
    assert_eq!(
        w.progress()[0],
        format!(
            "The host of '{AMD_NAME}' ({}) has no live lease, but a provisioning run is working on it; it is not stopped.",
            id(2)
        )
    );
}

/// Another platform's lock holds nothing off: the `linux/arm64` host's lock held leaves a
/// Windows host of the same project due, and the Windows host's lock held leaves the `linux/amd64`
/// host due. Each is tried in its own folder, never the project's.
#[test]
fn another_hosts_provisioning_lock_does_not_leave_a_due_host() {
    let w = world(
        "other-lock",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(WINDOWS, "ready", Some(&id(2))),
        ],
        &[orchard_ready(ARM64, &today("23:00:00"))],
    );
    for n in [1, 2] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    let _lock = hold(&w.provision_lock("orchard"));
    w.run_ok(now());
    assert_eq!(
        w.stopped_ids(),
        vec![id(2)],
        "The Linux host's lock does not leave the Windows host"
    );

    let w = world(
        "other-lock-amd",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(LINUX_AMD64, "ready", Some(&id(2))),
            host_on(WINDOWS, "creating", None),
        ],
        &[orchard_ready(ARM64, &today("23:00:00"))],
    );
    for n in [1, 2] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    let _lock = hold(&w.provision_lock(WIN_NAME));
    w.run_ok(now());
    assert_eq!(
        w.stopped_ids(),
        vec![id(2)],
        "The Windows host's lock does not leave the `linux/amd64` host"
    );
}

// ---------------------------------------------------------------------------------------------
// Each host's horizon (step 7): its platform's leases, through its platform's document.
// ---------------------------------------------------------------------------------------------

/// Step 7 and state.md: each host is sent its own horizon, the later of its project's horizon on
/// its platform and its launch. Two hosts of one project on the two Linux platforms are sent two
/// horizons, each through `AWS-RunShellScript` in the form `commands=<command>`, unchanged.
#[test]
fn each_hosts_horizon_is_its_own_platforms_through_the_shell_document() {
    let w = world(
        "linux-horizons",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(LINUX_AMD64, "ready", Some(&id(2))),
        ],
        &[
            orchard_ready(ARM64, &today("22:00:00")),
            orchard_ready(LINUX_AMD64, &today("21:00:00")),
        ],
    );
    for n in [1, 2] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    w.run_ok(now());
    let sends = w.calls_of("ssm send-command");
    assert_eq!(sends.len(), 3, "The box and each host of the project");
    let arm = send_to(&sends, &id(1));
    let amd = send_to(&sends, &id(2));
    assert_eq!(horizon_of(arm), t(&today("22:00:00")));
    assert_eq!(horizon_of(amd), t(&today("21:00:00")));
    for send in [arm, amd] {
        assert_eq!(document_of(send), "AWS-RunShellScript");
        assert!(parameters_of(send).starts_with("commands="));
    }
}

/// D96, operator.md step 7, the seam's Session Manager row: a Windows host is sent its horizon,
/// its project's on `windows/amd64`, through `AWS-RunPowerShellScript`, with `--parameters` the
/// JSON value `{"commands":["<command>"]}` and never `commands=`, while a `linux/arm64` host of
/// the same project in the same run keeps `AWS-RunShellScript` and `commands=`.
///
/// This holds only once `agentspace-windows`' horizon row is built (E0.12-T03-S06): until then
/// the guard's ask of that row fails, and so does this test, which asserts no text of the row.
#[test]
fn a_windows_host_is_sent_its_horizon_through_the_powershell_document_as_a_json_value() {
    let w = world(
        "win-horizon",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(WINDOWS, "ready", Some(&id(2))),
        ],
        &[
            orchard_ready(ARM64, &today("22:00:00")),
            orchard_ready(WINDOWS, &today("21:30:00")),
        ],
    );
    for n in [1, 2] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    let report = w.run_ok(now());
    assert_eq!(report.failed, 0, "No part failed: {:?}", w.warnings());
    let sends = w.calls_of("ssm send-command");
    let win = send_to(&sends, &id(2));
    assert_eq!(document_of(win), "AWS-RunPowerShellScript");
    assert_eq!(
        win.args[win.args.iter().position(|a| a == "--comment").unwrap() + 1],
        "agentspace guard: lease horizon"
    );
    let parameters = parameters_of(win);
    assert!(
        !parameters.starts_with("commands="),
        "A Windows host never gets the shorthand"
    );
    assert_eq!(json_horizon(&parameters), t(&today("21:30:00")));

    let arm = send_to(&sends, &id(1));
    assert_eq!(document_of(arm), "AWS-RunShellScript");
    assert!(parameters_of(arm).starts_with("commands="));
    assert_eq!(horizon_of(arm), t(&today("22:00:00")));
}

/// A Windows host whose project holds only Linux leases is unexplained, and so its horizon is the
/// later of its launch and its own last lease end, never a Linux lease's `expires_at`: a host not
/// yet due is sent that horizon through its own document. Holds with the Windows row built.
#[test]
fn a_windows_hosts_horizon_ignores_the_projects_linux_leases() {
    let w = world(
        "win-horizon-ignores-linux",
        &[host_on(WINDOWS, "ready", Some(&id(2)))],
        &[orchard_ready(ARM64, &today("23:30:00"))],
    );
    // Launched 19:50: unexplained and not due (it stops at 20:05), so it is sent its horizon,
    // which is its launch, and not the Linux lease's 23:30.
    w.aws
        .add_instance(&id(2), "running", Some(&launch("19:50:00")));
    let report = w.run_ok(now());
    assert_eq!(report.failed, 0, "No part failed: {:?}", w.warnings());
    let sends = w.calls_of("ssm send-command");
    let win = send_to(&sends, &id(2));
    assert_eq!(document_of(win), "AWS-RunPowerShellScript");
    assert_eq!(json_horizon(&parameters_of(win)), t(&today("19:50:00")));
}

/// A Windows host is never sent the shell document, whatever else the run does with it: its
/// platform's document is PowerShell's, and a command written for `AWS-RunShellScript` would not
/// run there (operator.md, The platform seam, the Session Manager document row). Holds with the
/// Windows row built or not: a host not sent a horizon is sent nothing, and one that is, is sent
/// through `AWS-RunPowerShellScript`.
#[test]
fn a_windows_host_is_never_sent_the_shell_document() {
    let w = world(
        "win-never-shell",
        &[
            host_on(ARM64, "ready", Some(&id(1))),
            host_on(WINDOWS, "ready", Some(&id(2))),
        ],
        &[
            orchard_ready(ARM64, &today("22:00:00")),
            orchard_ready(WINDOWS, &today("21:30:00")),
        ],
    );
    for n in [1, 2] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    let _ = w.run(now());
    let to_windows: Vec<Rec> = w
        .calls_of("ssm send-command")
        .into_iter()
        .filter(|s| s.instance().as_deref() == Some(id(2).as_str()))
        .collect();
    for send in &to_windows {
        assert_eq!(document_of(send), "AWS-RunPowerShellScript");
    }
}
