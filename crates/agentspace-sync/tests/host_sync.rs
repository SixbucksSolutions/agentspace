//! `sync -Host` for a host that is not the box: its calls over the host's alias, its folder of its
//! own, its instance ID from the registry, and its push stamp (docs/operator.md, sync; How the
//! operator reaches a host; Which host a command acts on; The operator's folder, one per command;
//! The token push; docs/state.md, A host's folder and A host's push stamp).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `sync_target_using` for `Target::Host` in a child of the test binary, under a throwaway home with
//! a throwaway CA, with stand-ins for the AWS CLI, ssh, scp and go. The operator's key, certificate
//! and alias are in `host_access`, and the failures in `host_sync_failures`.
#![cfg(windows)]

mod common;
mod hostcalls;

use agentspace_sync::Target;
use common::expect::*;
use common::hosts::*;
use common::*;
use hostcalls::setup::*;

#[test]
fn sync_over_a_hosts_alias_makes_the_calls_of_the_box_in_order() {
    let rig = sync_rig(true);
    rig.with_claude_md();
    rig.run(|rig| {
        let fp = fingerprint(rig);
        let alias = alias_of(PROJECT);
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        let log = rig.log();
        let built = built(rig);
        rig.same_list(
            "The order of sync's messages and calls",
            &timeline_of(&log, |_| true),
            &expected_timeline(&fp, true, built),
        );
        check_calls_with(rig, &log, true, built, &alias, Folder::PerCommand);
        rig.same_list(
            "The first AWS call: the host's own instance, from its record, not the retired cache's",
            &log.calls("aws")[0].args,
            &describe_instances_args(ID2),
        );
        rig.check(
            log.calls("aws")
                .iter()
                .all(|c| c.args.get(1).map(String::as_str) != Some("describe-stacks")),
            "A record that holds an instance ID MUST NOT make sync read the stack",
        );
        rig.check(say.warnings().is_empty(), "sync MUST print no warning here");
        check_environment(rig, &log);

        // One folder for what the command sends: box/system and the localswim CLI, and the build's
        // output; and none left behind.
        let scp = log.calls("scp");
        let system = command_folder_of(&scp[0].args[4]);
        let cli = command_folder_of(&scp[3].args[3]);
        rig.check(
            system.is_some() && system == cli,
            "box/system and the localswim CLI MUST be in one folder of the command's own",
        );
        let go = log.calls("go");
        let out = go[0]
            .args
            .iter()
            .position(|a| a == "-o")
            .and_then(|i| go[0].args.get(i + 1))
            .cloned()
            .unwrap_or_default();
        rig.check(
            command_folder_of(&out) == system,
            "The localswim CLI MUST be built into the command's own folder",
        );
        rig.check(
            rig.command_folders().is_empty(),
            "The command's folder MUST be removed as the command ends",
        );
    });
}

#[test]
fn sync_copies_box_system_to_a_host_as_it_does_to_the_box() {
    let rig = sync_rig(false);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        let log = rig.log();
        let snap = log.dir_snap("scp", 1);
        let want = listing(&repo_root().join("box").join("system"));
        rig.check(
            matches!(&snap, Some((_, entries)) if *entries == want),
            "The staging copy MUST hold exactly the repository's box/system files, byte for byte",
        );
    });
}

#[test]
fn two_syncs_each_write_into_a_folder_of_their_own() {
    let rig = sync_rig(false);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "The first sync",
        );
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "The second sync",
        );
        let log = rig.log();
        let scp = log.calls("scp");
        let first = command_folder_of(&scp[0].args[4]);
        let second = command_folder_of(&scp[2].args[4]);
        rig.check(
            first.is_some() && second.is_some() && first != second,
            "Two commands MUST NOT send from one folder: each draws its own <hex>",
        );
        rig.check(
            rig.command_folders().is_empty(),
            "Each command's folder MUST be removed as it ends",
        );
    });
}

#[test]
fn sync_sweeps_an_old_command_folder_and_leaves_a_younger_one() {
    let rig = sync_rig(false);
    let old = "0123456789abcdef0123456789abcdef";
    let young = "fedcba9876543210fedcba9876543210";
    rig.leftover_folder(old, 25);
    rig.leftover_folder(young, 23);
    rig.run(move |rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        rig.check(
            rig.command_folders() == vec![young.to_string()],
            "sync MUST remove a command folder created more than 24 hours ago, and leave a younger one",
        );
    });
}

// ---------------------------------------------------------------- The host's instance ID

#[test]
fn a_record_with_no_instance_id_is_filled_from_the_stack_and_nothing_else_changes() {
    let rig = sync_rig(false);
    rig.seed_hosts(&[Seed::new(PROJECT, "creating").attempt("2026-11-20T10:00:00Z")]);
    rig.stack_status("CREATE_COMPLETE");
    rig.run(|rig| {
        let (stack, _) = names();
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        let log = rig.log();
        let aws = log.calls("aws");
        rig.same_list(
            "The first AWS call reads the host's stack",
            &aws[0].args,
            &describe_stacks_args(&stack),
        );
        rig.same_list(
            "The next call starts what the stack made",
            &aws[1].args,
            &describe_instances_args(ID2),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_id.as_deref() == Some(ID2)
                && record.state.as_str() == "creating"
                && record.last_attempt_at.as_deref() == Some("2026-11-20T10:00:00Z"),
            "The instance ID MUST be written into the record, changing nothing else in it",
        );
    });
}

/// A host whose record is in `state` is synced as any is, and the record's state is left alone:
/// no command but provision writes one (operator.md, Which host a command acts on).
fn synced_in_state(state: &'static str) {
    let rig = sync_rig(false);
    rig.seed_hosts(&[Seed::new(PROJECT, state).id(ID2)]);
    rig.run(move |rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        let now = rig.record(PROJECT).state;
        rig.check(
            now.as_str() == state,
            &format!(
                "sync MUST NOT change a record's state: no command but provision does; it was {state} and is {}",
                now.as_str()
            ),
        );
    });
}

#[test]
fn a_creating_host_is_synced_and_its_state_left_alone() {
    synced_in_state("creating");
}

#[test]
fn a_failed_host_is_synced_and_its_state_left_alone() {
    synced_in_state("failed");
}

// ---------------------------------------------------------------- The push, with the host's stamp

#[test]
fn the_token_push_goes_over_the_hosts_alias_and_writes_that_hosts_stamp() {
    let rig = sync_rig(false);
    rig.enable_push();
    rig.run(|rig| {
        let alias = alias_of(PROJECT);
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        let log = rig.log();
        let last = log.remote_calls().into_iter().last().unwrap();
        rig.check(
            last.name == "ssh"
                && last.args.len() == 6
                && last.args[..5]
                    == svec(&["-o", "ClearAllForwardings=yes", "-o", "BatchMode=yes", &alias])[..],
            "The push MUST be `ssh -o ClearAllForwardings=yes -o BatchMode=yes agentspace-host.<project> <remote command>`",
        );
        rig.check(
            !last.args[5].contains(TOKEN),
            "The push's remote command MUST NOT carry the token",
        );
        let stdin = std::fs::read(rig.root.join("stdin.bin")).unwrap();
        rig.check(
            stdin == format!("LOCALSWIM_URL={BASE}\nLOCALSWIM_TOKEN={TOKEN}\n").as_bytes(),
            "The push's stdin MUST be exactly the two lines",
        );
        let stamp = agentspace_hosts::host_stamp_path(&rig.home, PROJECT);
        rig.check(
            std::fs::read(&stamp)
                .ok()
                .and_then(|b| agentspace_push::parse_stamp_file(&b))
                .is_some(),
            "A confirmed push MUST write the host's own stamp, hosts\\<project>\\localswim-pushed.json",
        );
        rig.check(
            !agentspace_push::stamp_path(&rig.ctx()).exists(),
            "A host's push MUST NOT write the box's stamp",
        );
        rig.check(say.warnings().is_empty(), "A confirmed push prints no warning");

        // The push is forced: a second sync pushes again, whatever the stamp says.
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "The second sync",
        );
        let pushes = rig
            .log()
            .calls("ssh")
            .iter()
            .filter(|c| c.args.iter().any(|a| a == "BatchMode=yes"))
            .count();
        rig.check(
            pushes == 2,
            &format!("sync's push is forced, so two runs MUST push twice; they pushed {pushes} times"),
        );
    });
}

#[test]
fn a_push_a_host_does_not_confirm_warns_and_does_not_fail_sync_or_write_the_stamp() {
    let rig = sync_rig(false);
    rig.enable_push();
    rig.rule("ssh", 0, "BatchMode=yes", 0, "", "");
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync MUST NOT fail on the push",
        );
        rig.check(
            say.warnings()
                .iter()
                .any(|w| w.contains("The instance did not confirm the localswim token")),
            "An unconfirmed push MUST warn that the instance did not confirm the localswim token",
        );
        rig.check(
            !agentspace_hosts::host_stamp_path(&rig.home, PROJECT).exists(),
            "An unconfirmed push MUST NOT write the host's stamp",
        );
    });
}
