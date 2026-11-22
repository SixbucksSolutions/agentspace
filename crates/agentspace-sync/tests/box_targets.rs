//! The box through the new entries: `Target::Box` makes the calls v0.06's `sync` and `deploy` made,
//! writes no key, certificate or alias, and keeps the box's push stamp where it was; and
//! `provision` for the box's project goes through the box's own `deploy`, `sync` and `bootstrap`
//! (docs/operator.md, Which host a command acts on, "Without -Host"; From the command line; deploy;
//! provision; Programs and install layout, The operator's folder, one per command).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The existing tests of `sync_using` and
//! `deploy_using` are the proof of the box's own path. These run the crate's hidden
//! `sync_target_using`, `deploy_target_using` and `provision_using` in a child of the test binary,
//! under a throwaway home.
//!
//! **The box's calls are v0.06's but for two things operator.md moved** (Calls by command: "As before
//! v0.07, but for the template's folder"; Which host a command acts on, "Without -Host"): what the
//! box's command sends is in the command's own folder, `operator\<hex>\`, and the box's instance ID
//! is its record's in the registry, where the retired cache held it. The rig seeds the box's record
//! with `ID` (operator.md, The test seam, step 2).
#![cfg(windows)]

mod common;
mod hostcalls;

use agentspace_sync::Target;
use common::expect::*;
use common::*;
use hostcalls::setup::*;

#[test]
fn the_box_through_sync_target_makes_the_calls_of_v0_06() {
    let rig = Rig::new(true);
    rig.make_ca();
    rig.with_claude_md();
    rig.run(|rig| {
        let fp = fingerprint(rig);
        let say = rig.say();
        rig.succeeded(rig.do_sync_target(&say, Target::Box), "sync");
        let log = rig.log();
        let built = built(rig);
        rig.same_list(
            "The order of sync's messages and calls",
            &timeline_of(&log, |_| true),
            &expected_timeline(&fp, true, built),
        );
        check_calls_with(rig, &log, true, built, "agentspace", Folder::PerCommand);
        rig.same_list(
            "The first AWS call",
            &log.calls("aws")[0].args,
            &describe_instances_args(ID),
        );
        rig.check(say.warnings().is_empty(), "sync MUST print no warning here");
        check_environment(rig, &log);
        rig.check(
            !rig.has_alias_files() && !rig.home_path(".agentspace/hosts").exists(),
            "The operator writes no key, certificate or alias for the box",
        );
    });
}

#[test]
fn the_box_keeps_its_push_stamp_and_a_host_gets_none_of_the_boxs() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.enable_push();
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync_target(&say, Target::Box), "sync");
        let log = rig.log();
        let last = log.remote_calls().into_iter().last().unwrap();
        rig.check(
            last.args.len() == 6
                && last.args[..5]
                    == svec(&["-o", "ClearAllForwardings=yes", "-o", "BatchMode=yes", "agentspace"])[..],
            "The box's push MUST be over `agentspace`",
        );
        rig.check(
            agentspace_push::stamp_path(&rig.ctx()).is_file()
                && !agentspace_hosts::host_stamp_path(&rig.home, "swimcoach").exists()
                && !rig.home_path(".agentspace/hosts").exists(),
            "The box's stamp MUST stay %USERPROFILE%\\.agentspace\\localswim-pushed.json, and no host folder is made for it",
        );
    });
}

#[test]
fn the_box_through_deploy_target_passes_every_member_and_neither_host_name_nor_ca_public_key() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.run(|rig| {
        let fp = fingerprint(rig);
        let say = rig.say();
        rig.succeeded(rig.do_deploy_target(&say, Target::Box), "deploy");
        let log = rig.log();
        let aws = log.calls("aws");
        // The deploy, the read of the stack's output, sync's two `ec2 describe-instances`, and from
        // v0.08 the three calls of sync's patch wait (operator.md, The patch wait).
        rig.check(
            aws.len() == 7,
            &format!(
                "deploy and sync MUST make 7 AWS calls here, with no existence check, and made {}",
                aws.len()
            ),
        );
        rig.same_list(
            "The calls of sync's patch wait, after its start-and-wait",
            &aws[4..]
                .iter()
                .map(|c| format!("{} {}", c.args[0], c.args[1]))
                .collect::<Vec<_>>(),
            &svec(&[
                "ssm describe-instance-information",
                "ssm send-command",
                "ssm get-command-invocation",
            ]),
        );
        let template = template_arg(&aws[0].args);
        rig.same_list(
            "The cloudformation deploy call",
            &aws[0].args,
            &deploy_call_args(
                "agentbox",
                &template,
                &[
                    format!("SshPublicKey={KEY_LINE}"),
                    "SubnetId=SUBNET_VALUE".to_string(),
                    "VpcId=VPC_VALUE".to_string(),
                ],
            ),
        );
        rig.check(
            template
                .replace('/', "\\")
                .to_ascii_lowercase()
                .ends_with("\\agentspace.yaml"),
            "The template MUST be agentspace.yaml in the operator's folder",
        );
        rig.same_list(
            "The call that reads the instance ID",
            &aws[1].args,
            &describe_stacks_args("agentbox"),
        );
        rig.same_list(
            "The first call of sync's start-and-wait",
            &aws[2].args,
            &describe_instances_args(ID2),
        );
        rig.same_list(
            "sync's messages and calls",
            &timeline_of(&log, |_| true),
            &expected_timeline(&fp, false, false),
        );
        check_calls_with(rig, &log, false, false, "agentspace", Folder::PerCommand);
        rig.check(
            !rig.has_alias_files(),
            "The operator writes no key, certificate or alias for the box",
        );
        let record = rig.record("swimcoach");
        rig.check(
            record.instance_id.as_deref() == Some(ID2) && record.state.as_str() == "ready",
            "deploy MUST write the instance ID the stack's output names into the box's record, and nothing else",
        );
        rig.check(
            !rig.home_path(".agentspace/instance-id.json").exists(),
            "No v0.07 program writes the retired instance-ID cache",
        );
    });
}

fn box_refuses_case(member: &'static str) {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    let text = format!(
        "{{\"version\":0,\"parameters\":{{\"SubnetId\":\"SUBNET_VALUE\",\"VpcId\":\"VPC_VALUE\",\"{member}\":\"x\"}}}}"
    );
    rig.write_home(".agentspace/stack-parameters.json", text.as_bytes());
    rig.run(move |rig| {
        let path = rig.home.join(".agentspace").join("stack-parameters.json");
        let prefix = format!(
            "agentspace: {} is not a valid stack parameters file: ",
            path.display()
        );
        let say = rig.say();
        let f = rig.failure(rig.do_deploy_target(&say, Target::Box), "deploy");
        let reason = f.message.strip_prefix(&prefix);
        rig.check(
            matches!(reason, Some(r) if r.contains(member) && r.ends_with('.') && !r.contains('\n')),
            &format!("A file that names {member} MUST fail the box's deploy as `agentspace: <path> is not a valid stack parameters file: <reason>.`, the reason naming it; it was `{}`", f.message),
        );
        rig.check(
            rig.log().all_calls().is_empty(),
            "The box's deploy MUST fail before any program starts",
        );
    });
}

#[test]
fn a_parameters_file_that_names_host_name_is_not_valid_for_the_box() {
    box_refuses_case("HostName");
}

#[test]
fn a_parameters_file_that_names_ca_public_key_is_not_valid_for_the_box() {
    box_refuses_case("CaPublicKey");
}

// ---------------------------------------------------------------- provision -Host swimcoach

#[test]
fn provision_for_the_boxs_project_runs_the_boxs_own_steps_and_keeps_its_record_ready() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.beside_operator(true);
    rig.capture_ssh_stdin();
    watch_registry(&rig);
    rig.probe_lock(agentspace_hosts::host_run_lock_path(&rig.home, "swimcoach"));
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, "swimcoach"), "provision -Host swimcoach");
        let log = rig.log();

        // The box's own deploy: no existence check, the box's parameters, the box's stack.
        let aws = log.calls("aws");
        rig.check(
            aws[0].args.get(1).map(String::as_str) == Some("deploy")
                && aws[0].args.iter().any(|a| a == "agentbox")
                && aws[0].args.iter().any(|a| a == "SubnetId=SUBNET_VALUE")
                && !aws[0].args.iter().any(|a| a.starts_with("HostName=") || a.starts_with("CaPublicKey=")),
            "The box's deploy step MUST be the box's full deploy: the stack agentbox, every member of the file, and neither HostName nor CaPublicKey",
        );

        // Its sync and bootstrap go over `agentspace`, and no key, certificate or alias is written.
        let ssh = log.calls("ssh");
        rig.check(
            !ssh.is_empty() && ssh.iter().all(|c| c.args.iter().any(|a| a == "agentspace")),
            "Every ssh of the box's run MUST be over `agentspace`",
        );
        rig.check(
            !rig.has_alias_files() && !rig.host_folder("swimcoach").join("id_ed25519").exists(),
            "The operator writes no key, certificate or alias for the box",
        );

        // Its log and lock are in hosts\swimcoach\, its record stays ready, and no sign-in line.
        let names = rig.own_log_names("swimcoach");
        rig.check(
            names.len() == 8
                && names[0] == "run begun"
                && names[5] == "record done"
                && names[7] == "run done",
            &format!("The box's run MUST log as any run does: {names:?}"),
        );
        let states = log.watched_calls("hosts.json");
        rig.check(
            !states.is_empty()
                && states
                    .iter()
                    .all(|(_, _, c)| state_is(&record_in(c, "swimcoach"), "ready")),
            "The box's record MUST stay ready throughout",
        );
        let probes = log.probed("provision.lock");
        rig.check(
            !probes.is_empty() && probes.iter().all(|p| p.2),
            "The run's lock MUST be held for the box's run too",
        );
        rig.check(
            rig.record("swimcoach").state.as_str() == "ready"
                && rig.record("swimcoach").last_attempt_at.is_some(),
            "The box's record MUST end ready, with the run's attempt",
        );
        rig.check(
            !say.progress_lines().iter().any(|l| l.starts_with("Sign in on this host")),
            "The sign-in line is never printed for the box",
        );
        rig.check(
            say.progress_lines().first().map(String::as_str)
                == Some("Provisioning the host of 'swimcoach'..."),
            "The run MUST begin with `Provisioning the host of '<project>'...`",
        );
    });
}
