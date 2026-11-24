//! `provision`'s deploy step finds its instance type and records the one it reads back
//! (E0.09-T03-S02, #0204; docs/operator.md, provision, steps 4.1 to 4.5 and Its log; docs/state.md,
//! The instance type, The order of the locks; docs/driver.md, A resizing run, Lock order;
//! docs/decisions.md, D66, D68 and D71 with their change notes).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `provision_using` in a child of the test binary, under a throwaway home with a throwaway CA, with
//! stand-ins for the AWS CLI, ssh, scp and go. Every stand-in records, at each call, the registry's
//! and the lease state file's content and whether the run's lock, the registry's and the lease
//! state lock are held.
//!
//! - **The type comes from the lease state file** through the EC2 driver's map: the largest size
//!   among the project's `pending` leases the run would place, or none when a live `ready` lease
//!   holds the host, or when there is no such lease (pins P2(a), P18, P20, P26).
//! - **The record's type is removed before the first call to AWS**, whether the run will pass a
//!   type or none, under the lease state lock and the registry's, and both are released before
//!   any call to AWS (pin P26). A run's failed deploy step leaves none (pin P30).
//! - **Every run records the type `describe-stacks` reads back**, and logs the type it passed on
//!   step 4's line (pin P24).
//! - **The box's run finds its type the same way** and never reads `stack-parameters.json`'s
//!   `InstanceType` (D67, D71).
//!
//! **Step 9 is not what these tests judge.** `place_pending` is built by E0.09-T03-S01 (#0203), so
//! each test accepts what it did to the leases, and reads the lease state file only where the run
//! has not yet reached it.
#![cfg(windows)]

mod common;
mod hostcalls;

use std::path::PathBuf;
use std::time::Duration;

use common::hosts::*;
use common::leases::{self, FAR, IDS, PAST};
use common::provlog::LogLine;
use common::*;
use hostcalls::setup::*;

/// The type a ready host's record holds before the run, and the one its stack holds after it.
const BEFORE: &str = "m9g.large";
const AFTER: &str = "m9g.2xlarge";
/// What the parameters file of `prov_rig_with` names for `InstanceType`: no call may carry it.
const FILE_TYPE: &str = "m9g.8xlarge";

const DEPLOY_FAILED: &str = "Stack deploy failed; see the stack events in the console.";

fn lease_file_path(rig: &Rig) -> PathBuf {
    rig.home_path(".agentspace/leases.json")
}

/// The stand-in `aws`'s stack answers for `kind`: an existing stack for a host that was `ready`,
/// no stack yet for any other, and `instance_type` as the stack's `InstanceType` once made.
fn stack_of_kind(kind: Kind, instance_type: &'static str) -> impl FnOnce(&Rig) {
    move |rig: &Rig| {
        if kind == Kind::Ready {
            rig.stack_status_typed("CREATE_COMPLETE", instance_type);
        } else {
            rig.stack_missing_typed(&stack_of(PROJECT), instance_type);
        }
    }
}

/// A run of [`PROJECT`]'s host, as `kind` says. A `ready` record holds `record_type` before the
/// run. `leases`, when there are any, are the lease state file. Every stand-in watches both
/// files, and probes the lease state lock beside the run's lock and the registry's.
fn rig_with(
    kind: Kind,
    record_type: Option<&'static str>,
    leases: Option<Vec<String>>,
    stack: impl FnOnce(&Rig),
) -> Rig {
    let rig = prov_rig_with(kind, |rig| {
        stack(rig);
        rig.watch_file(lease_file_path(rig));
        rig.probe_lock(rig.home_path(".agentspace/leases.lock"));
    });
    if kind == Kind::Ready {
        let mut seed = Seed::new(PROJECT, "ready").id(ID).attempt(OLD_ATTEMPT);
        if let Some(t) = record_type {
            seed = seed.itype(t);
        }
        rig.seed_hosts(&[seed]);
    }
    if let Some(l) = leases {
        rig.write_home(".agentspace/leases.json", leases::file(&l).as_bytes());
    }
    rig
}

/// The text a stand-in records for a file: its line ends written `\n` and `\r`.
fn as_watched(text: &str) -> String {
    text.replace('\r', "\\r").replace('\n', "\\n")
}

fn aws_calls(log: &Log, verb: &str) -> Vec<Rec> {
    log.calls("aws")
        .into_iter()
        .filter(|c| c.args.get(1).map(String::as_str) == Some(verb))
        .cloned()
        .collect()
}

fn record_at(log: &Log, n: u32, project: &str) -> String {
    match log.watched("aws", n, "hosts.json") {
        Some(content) => record_in(content, project),
        None => panic!("The registry was not watched at aws call {n}"),
    }
}

fn has(record: &str, member: &str) -> bool {
    record.contains(&format!("\"{member}\":"))
}

/// The run's one `deploy` line, `done` or `failed`.
fn deploy_line(rig: &Rig) -> LogLine {
    match rig
        .own_log_lines(PROJECT)
        .into_iter()
        .find(|l| l.step == "deploy")
    {
        Some(l) => l,
        None => panic!("The run's log MUST hold a deploy line"),
    }
}

/// What step 4.2 leaves for every call to AWS to see: the record without a type, otherwise as it
/// was (a `ready` host keeps `ready` and its instance ID), and the run's lock held, the
/// registry's and the lease state lock free, at every program the run started.
fn check_before_aws_and_locks(rig: &Rig, log: &Log, kind: Kind) {
    let first = &log.calls("aws")[0];
    let record = record_at(log, first.n, PROJECT);
    rig.check(
        !has(&record, "instance_type") && has(&record, "last_attempt_at"),
        &format!("Before the first call to AWS the record MUST hold no type, and the run's start time: {record}"),
    );
    if kind == Kind::Ready {
        rig.check(
            record.contains("\"state\":\"ready\"") && record.contains(&format!("\"instance_id\":\"{ID}\"")),
            &format!("A ready host's record MUST stay ready and keep its instance ID while its type is gone: {record}"),
        );
    }
    for (file, want) in [
        ("provision.lock", true),
        ("hosts.lock", false),
        ("leases.lock", false),
    ] {
        let probes = log.probed(file);
        let seen: Vec<String> = probes
            .iter()
            .map(|p| format!("{}#{}={}", p.0, p.1, if p.2 { "held" } else { "free" }))
            .collect();
        rig.check(
            probes.len() == log.all_calls().len() && probes.iter().all(|p| p.2 == want),
            &format!(
                "{file} MUST be {} at every program the run starts ({} programs): only provision.lock is held during a call to AWS; the probes found {seen:?}",
                if want { "held" } else { "free" },
                log.all_calls().len()
            ),
        );
    }
}

fn deploy_override_args(rig: &Rig, log: &Log, kind: Kind, extra: &[String]) -> (Rec, bool) {
    let deploy = aws_calls(log, "deploy").remove(0);
    let (stack, _) = names();
    let template = template_arg(&deploy.args);
    let ok = if kind == Kind::Ready {
        deploy.args == deploy_call_args(&stack, &template, extra)
    } else {
        // A stack it creates: the creation parameters, then the type. The order of the
        // parameters is not what the spec fixes, so the overrides are compared sorted.
        let mut want = creation_overrides(rig, &stack);
        want.extend(extra.iter().cloned());
        want.sort();
        let at = deploy
            .args
            .iter()
            .position(|a| a == "--parameter-overrides")
            .unwrap_or(deploy.args.len());
        let mut got: Vec<String> = deploy.args.get(at + 1..).unwrap_or(&[]).to_vec();
        got.sort();
        deploy.args[..at.min(deploy.args.len())] == deploy_call_args(&stack, &template, &[])[..]
            && got == want
    };
    (deploy, ok)
}

// ------------------------------------------------------------------ No pending lease

#[test]
fn a_run_with_no_pending_lease_passes_no_type_and_records_the_one_it_reads_back() {
    let rig = rig_with(
        Kind::Ready,
        Some(BEFORE),
        None,
        stack_of_kind(Kind::Ready, AFTER),
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let log = rig.log();
        let (stack, _) = names();
        let deploy = aws_calls(&log, "deploy").remove(0);
        rig.same_list(
            "The cloudformation deploy call: no parameter, so the stack keeps its type",
            &deploy.args,
            &deploy_call_args(&stack, &template_arg(&deploy.args), &[]),
        );
        check_before_aws_and_locks(rig, &log, Kind::Ready);
        let line = deploy_line(rig);
        rig.check(
            line.name() == "deploy done" && line.field.is_none(),
            "A run that found no type writes `deploy done` with no type field, as before v0.09",
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.as_deref() == Some(AFTER)
                && record.instance_id.as_deref() == Some(ID2)
                && record.state.as_str() == "ready",
            "The run MUST record the type describe-stacks reads back, whether or not it passed one",
        );
    });
}

#[test]
fn a_ready_host_with_no_recorded_type_gets_it_from_its_first_run() {
    // Each host's first run after v0.09's install records its type (D67).
    let rig = rig_with(
        Kind::Ready,
        None,
        None,
        stack_of_kind(Kind::Ready, STACK_TYPE),
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        rig.check(
            rig.record(PROJECT).instance_type.as_deref() == Some(STACK_TYPE),
            "A run MUST record the type of a host whose record held none",
        );
    });
}

// ------------------------------------------------------------------ Pending leases

/// One `pending` lease of `size` for a host as `kind` says: the deploy passes `ty`, the type of
/// that size, as D65's table gives it, and logs it.
fn one_pending_lease_case(kind: Kind, size: &'static str, ty: &'static str) {
    let rig = rig_with(
        kind,
        Some(BEFORE),
        Some(vec![leases::pending(IDS[0], PROJECT, size)]),
        stack_of_kind(kind, ty),
    );
    rig.run(move |rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let log = rig.log();
        let (deploy, ok) = deploy_override_args(rig, &log, kind, &[format!("InstanceType={ty}")]);
        rig.check(
            ok,
            &format!(
                "The deploy MUST pass InstanceType={ty} for a pending lease of size {size}: {:?}",
                deploy.args
            ),
        );
        rig.check(
            log.all_calls()
                .iter()
                .all(|c| c.args.iter().all(|a| !a.contains(FILE_TYPE))),
            "The parameters file's InstanceType MUST reach no call",
        );
        check_before_aws_and_locks(rig, &log, kind);
        // The lease state file is read, not written, until step 9.
        rig.check(
            log.watched("aws", deploy.n, "leases.json")
                .map(str::to_string)
                == Some(as_watched(&leases::file(&[leases::pending(
                    IDS[0], PROJECT, size,
                )]))),
            "Step 4 MUST read the lease state file and write nothing to it",
        );
        let line = deploy_line(rig);
        rig.check(
            line.name() == "deploy done" && line.type_field() == Some(ty),
            &format!("Step 4's line MUST be `deploy done type={ty}`: {line:?}"),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.as_deref() == Some(ty) && record.state.as_str() == "ready",
            "The run MUST record the type its stack reports once resized, and end ready",
        );
    });
}

#[test]
fn a_pending_lease_of_size_s_makes_the_deploy_pass_m9g_large_for_a_new_host() {
    one_pending_lease_case(Kind::Failed, "s", "m9g.large");
}

#[test]
fn a_pending_lease_of_size_m_makes_the_deploy_pass_m9g_xlarge_even_where_it_changes_nothing() {
    one_pending_lease_case(Kind::Ready, "m", "m9g.xlarge");
}

#[test]
fn a_pending_lease_of_size_l_makes_the_deploy_pass_m9g_2xlarge_to_a_ready_host() {
    one_pending_lease_case(Kind::Ready, "l", "m9g.2xlarge");
}

#[test]
fn a_pending_lease_of_size_xl_makes_the_deploy_pass_m9g_4xlarge_for_a_host_with_no_record() {
    one_pending_lease_case(Kind::NoRecord, "xl", "m9g.4xlarge");
}

#[test]
fn several_pending_leases_of_mixed_sizes_make_the_deploy_pass_the_largest_ones_type() {
    // The pending leases a run would place: the project's, of the EC2 driver, of a size in
    // `SIZES`. Every other record is ignored, though some of them name a larger size (pins P18
    // and P20; driver.md, Sizes and instance types).
    let records = vec![
        leases::pending(IDS[0], PROJECT, "s"),
        leases::pending(IDS[1], PROJECT, "l"),
        leases::pending(IDS[2], PROJECT, "m"),
        leases::pending(IDS[3], "otherproject", "xl"),
        leases::released(IDS[4], PROJECT, "xl"),
        leases::pending_by(IDS[5], PROJECT, "xl", Some("elsewhere")),
        leases::pending_by(IDS[6], PROJECT, "xl", None),
        leases::pending(IDS[7], PROJECT, "xxl"),
    ];
    let rig = rig_with(
        Kind::Ready,
        Some(BEFORE),
        Some(records),
        stack_of_kind(Kind::Ready, AFTER),
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let log = rig.log();
        let (deploy, ok) =
            deploy_override_args(rig, &log, Kind::Ready, &[format!("InstanceType={AFTER}")]);
        rig.check(
            ok,
            &format!("The deploy MUST pass the type of the largest pending size the run would place, l: {:?}", deploy.args),
        );
        rig.check(
            deploy_line(rig).type_field() == Some(AFTER),
            "Step 4's line MUST log the type the run passed",
        );
    });
}

// ------------------------------------------------------------------ A live ready lease holds the host

/// A `ready` host holding `holder`, and a pending lease of size `xl`: the deploy passes `expected`,
/// no type when the holder is live and the project's.
fn holder_case(holder: String, expected: Option<&'static str>) {
    let pending = leases::pending(IDS[1], PROJECT, "xl");
    let records = vec![holder, pending];
    let text = leases::file(&records);
    let rig = rig_with(
        Kind::Ready,
        None,
        Some(records),
        stack_of_kind(Kind::Ready, STACK_TYPE),
    );
    rig.run(move |rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let log = rig.log();
        let extra: Vec<String> = expected
            .iter()
            .map(|t| format!("InstanceType={t}"))
            .collect();
        let (deploy, ok) = deploy_override_args(rig, &log, Kind::Ready, &extra);
        rig.check(
            ok,
            &format!(
                "The deploy's parameters MUST be {extra:?}: {:?}",
                deploy.args
            ),
        );
        rig.check(
            log.watched("aws", deploy.n, "leases.json")
                .map(str::to_string)
                == Some(as_watched(&text)),
            "Step 4 MUST write nothing to the lease state file",
        );
        let line = deploy_line(rig);
        rig.check(
            line.name() == "deploy done" && line.type_field() == expected,
            &format!("Step 4's line MUST log type={expected:?}: {line:?}"),
        );
        check_before_aws_and_locks(rig, &log, Kind::Ready);
    });
}

#[test]
fn a_live_ready_lease_holding_the_host_stops_the_resize_and_the_type_read_back_is_recorded() {
    // D68's change note, pin P2(a): the host is not stopped while a ready lease holds it, whatever a
    // pending lease asks for. The record held no type (across the install), so the run records the
    // one it reads back without resizing.
    let rig_records = vec![
        leases::ready(IDS[0], PROJECT, "m", FAR),
        leases::pending(IDS[1], PROJECT, "xl"),
    ];
    let text = leases::file(&rig_records);
    let rig = rig_with(
        Kind::Ready,
        None,
        Some(rig_records),
        stack_of_kind(Kind::Ready, STACK_TYPE),
    );
    rig.run(move |rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let log = rig.log();
        let (stack, _) = names();
        let deploy = aws_calls(&log, "deploy").remove(0);
        rig.same_list(
            "The cloudformation deploy call: no InstanceType, so the host is not stopped",
            &deploy.args,
            &deploy_call_args(&stack, &template_arg(&deploy.args), &[]),
        );
        rig.check(
            log.watched("aws", deploy.n, "leases.json")
                .map(str::to_string)
                == Some(as_watched(&text)),
            "Step 4 MUST write nothing to the lease state file",
        );
        let line = deploy_line(rig);
        rig.check(
            line.name() == "deploy done" && line.field.is_none(),
            &format!("A run that passed no type logs no type field: {line:?}"),
        );
        rig.check(
            rig.record(PROJECT).instance_type.as_deref() == Some(STACK_TYPE),
            "The run MUST record the type it reads back, though it passed none",
        );
        check_before_aws_and_locks(rig, &log, Kind::Ready);
    });
}

#[test]
fn an_expired_ready_lease_holds_no_host_so_the_pending_lease_resizes_it() {
    holder_case(
        leases::ready(IDS[0], PROJECT, "m", PAST),
        Some("m9g.4xlarge"),
    );
}

#[test]
fn another_projects_ready_lease_holds_no_host_of_this_one() {
    holder_case(
        leases::ready(IDS[0], "otherproject", "m", FAR),
        Some("m9g.4xlarge"),
    );
}

#[test]
fn a_pending_lease_alone_holds_no_hosts_size() {
    holder_case(leases::pending(IDS[0], PROJECT, "s"), Some("m9g.4xlarge"));
}

// ------------------------------------------------------------------ The box

/// A run on the box: swimcoach's record holds `BEFORE`, the parameters file names `InstanceType`
/// as [`FILE_TYPE`] and the stack holds [`AFTER`] once made.
fn box_rig_with(leases: Option<Vec<String>>) -> Rig {
    let rig = prov_rig_with(Kind::Ready, |rig| {
        rig.stack_status_typed("CREATE_COMPLETE", AFTER);
        rig.watch_file(lease_file_path(rig));
    });
    rig.seed_hosts_box(
        Seed::new("swimcoach", "ready").id(ID).itype(BEFORE),
        &[Seed::new(PROJECT, "ready").id(ID).attempt(OLD_ATTEMPT)],
    );
    if let Some(l) = leases {
        rig.write_home(".agentspace/leases.json", leases::file(&l).as_bytes());
    }
    rig
}

fn box_overrides(extra: &[String]) -> Vec<String> {
    let mut v = vec![
        format!("SshPublicKey={KEY_LINE}"),
        "SubnetId=SUBNET_VALUE".to_string(),
        "VpcId=VPC_VALUE".to_string(),
    ];
    v.extend(extra.iter().cloned());
    v
}

#[test]
fn the_boxs_run_passes_the_type_of_swimcoachs_pending_lease_and_never_the_files() {
    let rig = box_rig_with(Some(vec![
        leases::pending(IDS[0], "swimcoach", "l"),
        leases::pending(IDS[1], PROJECT, "xl"),
    ]));
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_provision(&say, "swimcoach"),
            "provision -Host swimcoach",
        );
        let log = rig.log();
        let deploy = &log.calls("aws")[0];
        rig.same_list(
            "The box's cloudformation deploy call, InstanceType last",
            &deploy.args,
            &deploy_call_args(
                "agentbox",
                &template_arg(&deploy.args),
                &box_overrides(&[format!("InstanceType={AFTER}")]),
            ),
        );
        rig.check(
            log.all_calls()
                .iter()
                .all(|c| c.args.iter().all(|a| !a.contains(FILE_TYPE))),
            "stack-parameters.json's InstanceType MUST reach no call",
        );
        let first_record = record_at(&log, deploy.n, "swimcoach");
        rig.check(
            !has(&first_record, "instance_type") && has(&first_record, "instance_id"),
            &format!("The box's type MUST be removed before cloudformation deploy: {first_record}"),
        );
        let lines = rig.own_log_lines("swimcoach");
        let line = lines.iter().find(|l| l.step == "deploy");
        rig.check(
            line.is_some_and(|l| l.name() == "deploy done" && l.type_field() == Some(AFTER)),
            "The box's step 4 line MUST log the type it passed",
        );
        rig.check(
            rig.record("swimcoach").instance_type.as_deref() == Some(AFTER)
                && rig.record("swimcoach").state.as_str() == "ready",
            "The box's record MUST hold the type its stack reports, and stay ready",
        );
    });
}

#[test]
fn the_boxs_run_with_no_pending_lease_passes_no_type_and_reads_none_from_the_file() {
    let rig = box_rig_with(None);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, "swimcoach"), "provision -Host swimcoach");
        let log = rig.log();
        let deploy = &log.calls("aws")[0];
        rig.same_list(
            "The box's cloudformation deploy call: every member of the file but InstanceType",
            &deploy.args,
            &deploy_call_args(
                "agentbox",
                &template_arg(&deploy.args),
                &box_overrides(&[]),
            ),
        );
        rig.check(
            log.all_calls()
                .iter()
                .all(|c| c.args.iter().all(|a| !a.contains(FILE_TYPE))),
            "stack-parameters.json's InstanceType MUST reach no call",
        );
        rig.check(
            rig.record("swimcoach").instance_type.as_deref() == Some(AFTER),
            "The box's first run records the type its stack holds, which changes nothing on it (D70)",
        );
    });
}

// ------------------------------------------------------------------ A failed deploy step

/// A `cloudformation deploy` that fails, for a ready host holding `BEFORE`, with `records` as its
/// leases: the failure's message, the log's line, the record and the lease file.
fn failed_deploy_case(records: Vec<String>, passed: Option<&'static str>) {
    let text = leases::file(&records);
    let rig = rig_with(Kind::Ready, Some(BEFORE), Some(records), |rig| {
        rig.stack_status_typed("CREATE_COMPLETE", BEFORE);
        rig.rule("aws", 0, "cloudformation deploy", 1, "", "boom\\n");
    });
    rig.run(move |rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        rig.same(
            "The failure's message",
            &f.message,
            &format!("agentspace: {DEPLOY_FAILED}"),
        );
        let names: Vec<String> = rig.own_log_lines(PROJECT).iter().map(LogLine::name).collect();
        rig.same_list(
            "The run's own lines",
            &names,
            &svec(&["run begun", "deploy failed"]),
        );
        let line = deploy_line(rig);
        rig.check(
            line.type_field() == passed && line.reason.as_deref() == Some(DEPLOY_FAILED),
            &format!("A failed deploy is logged with the type it was asked for, and its reason: {line:?}"),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.is_none()
                && record.state.as_str() == "ready"
                && record.instance_id.as_deref() == Some(ID),
            "A run's failed deploy step MUST leave no type, whether it passed one or none, and keep the host ready",
        );
        rig.check(
            std::fs::read_to_string(lease_file_path(rig)).ok().as_deref() == Some(text.as_str()),
            "The leases MUST stay as they were, pending: a failed run never answers one",
        );
        rig.check(
            rig.log().remote_calls().is_empty(),
            "Nothing MUST follow a failed deploy step",
        );
    });
}

#[test]
fn a_failed_deploy_that_passed_a_type_is_logged_with_it_and_leaves_no_type() {
    failed_deploy_case(vec![leases::pending(IDS[0], PROJECT, "l")], Some(AFTER));
}

#[test]
fn a_failed_deploy_that_passed_no_type_leaves_no_type_too_by_design() {
    // Unlike a person's deploy, a run removed the type before it decided (pins P26 and P30).
    failed_deploy_case(vec![], None);
}

#[test]
fn a_failed_read_back_leaves_no_type_and_no_instance_id_and_logs_the_type_passed() {
    let records = vec![leases::pending(IDS[0], PROJECT, "l")];
    let rig = rig_with(Kind::Ready, Some(BEFORE), Some(records), |rig| {
        rig.read_back_fails("read-back stand-in error\\n")
    });
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        rig.same(
            "The failure's message",
            &f.message,
            "agentspace: read-back stand-in error",
        );
        let line = deploy_line(rig);
        rig.check(
            line.name() == "deploy failed"
                && line.type_field() == Some(AFTER)
                && line.reason.as_deref() == Some("read-back stand-in error"),
            &format!("A failed read-back is a failed deploy step, logged with the type it passed: {line:?}"),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.is_none() && record.instance_id.is_none() && record.state.as_str() == "ready",
            "A failed read-back MUST leave the record with no type and no instance ID, still ready",
        );
    });
}

#[test]
fn a_stack_with_no_instance_type_parameter_fails_the_runs_deploy_step() {
    let records = vec![leases::pending(IDS[0], PROJECT, "l")];
    let rig = rig_with(Kind::Ready, Some(BEFORE), Some(records), |rig| {
        rig.read_back_answers(&json_stack_with(
            Some("CREATE_COMPLETE"),
            Some(ID2),
            Some(&json_params(&[("RootVolumeGiB", "8")])),
        ));
    });
    rig.run(|rig| {
        let (stack, _) = names();
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        let message = format!("Stack '{stack}' has no InstanceType parameter.");
        rig.same(
            "The failure's message",
            &f.message,
            &format!("agentspace: {message}"),
        );
        let line = deploy_line(rig);
        rig.check(
            line.name() == "deploy failed"
                && line.type_field() == Some(AFTER)
                && line.reason.as_deref() == Some(message.as_str()),
            &format!("The failed deploy step MUST be logged with its message: {line:?}"),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_id.as_deref() == Some(ID2)
                && record.instance_type.is_none()
                && record.state.as_str() == "ready",
            "The instance ID it read MUST still be written, alone, and the host stay ready",
        );
        rig.check(
            rig.log().remote_calls().is_empty(),
            "The run MUST NOT go on to sync",
        );
    });
}

// ------------------------------------------------------------------ A lease state file that cannot be read

/// The deploy step fails before any call to AWS, with `agentspace: The lease state file <path>
/// cannot be read: <reason>.`, the type already removed, the host's state as a failed deploy step
/// leaves it, and the leases untouched.
fn unreadable_case(kind: Kind, install: impl FnOnce(&Rig), same_bytes: Option<&'static [u8]>) {
    let rig = rig_with(kind, Some(BEFORE), None, stack_of_kind(kind, AFTER));
    install(&rig);
    rig.run(move |rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        let path = lease_file_path(rig);
        let prefix = format!("agentspace: The lease state file {} cannot be read: ", path.display());
        rig.check(
            norm(&f.message).starts_with(&norm(&prefix))
                && f.message.ends_with('.')
                && !f.message.contains('\n'),
            &format!("The failure MUST be `agentspace: The lease state file <path> cannot be read: <reason>.`; it was `{}`", f.message),
        );
        rig.check(
            rig.log().all_calls().is_empty(),
            "The step MUST fail before any call to AWS",
        );
        let names: Vec<String> = rig.own_log_lines(PROJECT).iter().map(LogLine::name).collect();
        rig.same_list(
            "The run's own lines",
            &names,
            &svec(&["run begun", "deploy failed"]),
        );
        let line = deploy_line(rig);
        rig.check(
            line.field.is_none()
                && line.reason.as_deref() == f.message.strip_prefix("agentspace: "),
            &format!("The line MUST carry no type, and the message as its reason: {line:?}"),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.is_none(),
            "The record MUST be left with no type, which step 4.2 removed",
        );
        if kind == Kind::Ready {
            rig.check(
                record.state.as_str() == "ready" && record.instance_id.as_deref() == Some(ID),
                "A ready host MUST be left ready, as every failure of the deploy step leaves it",
            );
        } else {
            rig.check(
                record.state.as_str() == "failed",
                "A host never ready MUST be recorded failed",
            );
        }
        if let Some(bytes) = same_bytes {
            rig.check(
                std::fs::read(lease_file_path(rig)).ok().as_deref() == Some(bytes),
                "The lease state file MUST be left as it was",
            );
        }
    });
}

const GARBAGE: &[u8] = b"this is not a lease state file\n";

#[test]
fn a_lease_state_file_that_is_not_json_fails_the_deploy_step_before_aws() {
    unreadable_case(
        Kind::Ready,
        |rig| rig.write_home(".agentspace/leases.json", GARBAGE),
        Some(GARBAGE),
    );
}

#[test]
fn a_lease_state_file_with_two_records_of_one_id_fails_the_deploy_step_before_aws() {
    unreadable_case(
        Kind::Ready,
        |rig| {
            let text = leases::file(&[
                leases::pending(IDS[0], PROJECT, "m"),
                leases::pending(IDS[0], PROJECT, "l"),
            ]);
            rig.write_home(".agentspace/leases.json", text.as_bytes());
        },
        None,
    );
}

#[test]
fn a_folder_at_the_lease_state_files_path_fails_the_deploy_step_before_aws() {
    unreadable_case(
        Kind::Ready,
        |rig| rig.mkdir_home(".agentspace/leases.json"),
        None,
    );
}

#[test]
fn an_unreadable_lease_state_file_records_a_host_never_ready_failed() {
    unreadable_case(
        Kind::NoRecord,
        |rig| rig.write_home(".agentspace/leases.json", GARBAGE),
        Some(GARBAGE),
    );
}

#[test]
fn a_missing_lease_state_file_holds_no_leases_and_asks_for_no_size() {
    let rig = rig_with(
        Kind::Ready,
        Some(BEFORE),
        None,
        stack_of_kind(Kind::Ready, BEFORE),
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let log = rig.log();
        let (stack, _) = names();
        let deploy = aws_calls(&log, "deploy").remove(0);
        rig.same_list(
            "The cloudformation deploy call",
            &deploy.args,
            &deploy_call_args(&stack, &template_arg(&deploy.args), &[]),
        );
    });
}

// ------------------------------------------------------------------ The lease state lock

#[test]
fn a_lease_state_lock_that_cannot_be_opened_fails_the_deploy_step_before_aws_and_keeps_the_type() {
    // A folder stands where `leases.lock` belongs (pin P35): not a lock another program holds.
    let rig = rig_with(
        Kind::Ready,
        Some(BEFORE),
        None,
        stack_of_kind(Kind::Ready, AFTER),
    );
    rig.mkdir_home(".agentspace/leases.lock");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        let path = rig.home_path(".agentspace/leases.lock");
        let prefix = format!("agentspace: Cannot take {}: ", path.display());
        rig.check(
            norm(&f.message).starts_with(&norm(&prefix))
                && f.message.ends_with('.')
                && !f.message.contains('\n'),
            &format!("The failure MUST be `agentspace: Cannot take <path>: <error>.`; it was `{}`", f.message),
        );
        rig.check(
            rig.log().all_calls().is_empty(),
            "The step MUST fail before any call to AWS",
        );
        let line = deploy_line(rig);
        rig.check(
            line.name() == "deploy failed"
                && line.field.is_none()
                && line.reason.as_deref() == f.message.strip_prefix("agentspace: "),
            &format!("The line MUST be `deploy failed reason=Cannot take <path>: <error>.`: {line:?}"),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.as_deref() == Some(BEFORE) && record.state.as_str() == "ready",
            "A lock taken before any write MUST leave the record's type as it was, and the host ready",
        );
    });
}

#[test]
fn a_lease_state_lock_held_by_another_program_fails_the_deploy_step_after_30_seconds() {
    let rig = rig_with(
        Kind::Ready,
        Some(BEFORE),
        None,
        stack_of_kind(Kind::Ready, AFTER),
    );
    rig.run(|rig| {
        // Held in this process for the whole call: the lock is a file opened with no sharing, so
        // a second open fails whoever holds it.
        let state_dir = rig.home_path(".agentspace");
        let held = match agentspace_state::lock(&state_dir) {
            Ok(lock) => lock,
            Err(_) => panic!("The throwaway lease state lock could not be taken"),
        };
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        let path = rig.home_path(".agentspace/leases.lock");
        same_message(
            rig,
            "The failure's message",
            &f.message,
            &format!(
                "agentspace: The lease state lock {} is held by another program; try again.",
                path.display()
            ),
        );
        rig.check(
            rig.log().all_calls().is_empty(),
            "The step MUST fail before any call to AWS",
        );
        let line = deploy_line(rig);
        rig.check(
            line.name() == "deploy failed"
                && line.field.is_none()
                && line.reason.as_deref().map(norm)
                    == f.message.strip_prefix("agentspace: ").map(norm),
            &format!("The line MUST carry the message as its reason: {line:?}"),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.as_deref() == Some(BEFORE) && record.state.as_str() == "ready",
            "A lock not taken MUST leave the record's type as it was, and the host ready",
        );
        drop(held);
    });
}

#[test]
fn the_deploy_step_waits_for_the_lease_state_lock_and_calls_aws_only_after_it() {
    // A creator holds the lease state lock for a while, as a `create` does for its whole place.
    // The run waits for it at its deploy step, so the holder's release, which writes a marker, is
    // seen by the run's first call to AWS (pin P26).
    let rig = rig_with(
        Kind::Ready,
        Some(BEFORE),
        None,
        stack_of_kind(Kind::Ready, AFTER),
    );
    let marker = rig.root.join("released.txt");
    rig.watch_file(marker.clone());
    rig.run(move |rig| {
        let state_dir = rig.home_path(".agentspace");
        let (ready, held) = std::sync::mpsc::channel();
        let marker = marker.clone();
        let holder = std::thread::spawn(move || {
            let lock = match agentspace_state::lock(&state_dir) {
                Ok(lock) => lock,
                Err(_) => panic!("The throwaway lease state lock could not be taken"),
            };
            let _ = ready.send(());
            std::thread::sleep(Duration::from_millis(1500));
            let _ = std::fs::write(&marker, b"released");
            drop(lock);
        });
        if held.recv().is_err() {
            panic!("The holder of the lease state lock never took it");
        }
        let say = rig.say();
        let done = rig.do_provision(&say, PROJECT);
        let _ = holder.join();
        rig.succeeded(done, "provision");
        let log = rig.log();
        rig.same(
            "The marker when the first call to AWS started",
            log.watched("aws", 1, "released.txt").unwrap_or("<none>"),
            "released",
        );
    });
}
