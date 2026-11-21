//! `sync`'s failures: each step's message, the CA check before the box is touched, the start-and-
//! wait step's failures, and the operator folder's (docs/operator.md, sync steps 1 to 7; AWS
//! failures; Starting the box and waiting for it; What the prototype took from its checkout, FA-16;
//! The test seam: "a stand-in that exits non-zero for one of them reaches that step's failure
//! message").
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each case runs `sync` in a child of the
//! test binary, with one stand-in call made to fail, and checks the message, and that nothing
//! after the failing call was started (A failure MUST end the command at once).
#![cfg(windows)]

mod common;

use agentspace_operator_api::{CallerFailure, SIGN_IN_MESSAGE};
use common::*;

/// Runs `sync` with the `n`th call of `prog` exiting `code`, and checks the failure's message and
/// the calls of ssh, scp and go that were started up to and including the failing one.
fn case(go: bool, claude: bool, prog: &str, n: u32, code: i32, message: &str, calls: &[&str]) {
    let rig = Rig::new(go);
    rig.make_ca();
    if claude {
        rig.with_claude_md();
    }
    rig.rule(prog, n, "", code, "", "");
    let (message, calls) = (message.to_string(), svec(calls));
    rig.run(move |rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync(&say), "sync");
        rig.same("The failure message", &f.message, &message);
        let log = rig.log();
        let names: Vec<String> = log
            .remote_calls()
            .iter()
            .map(|r| format!("{}#{}", r.name, r.n))
            .collect();
        rig.same_list(
            "The calls started up to the failure, and none after it",
            &names,
            &calls,
        );
    });
}

#[test]
fn step_3_a_failed_removal_of_the_staging_folder_fails_before_the_copy() {
    case(
        false,
        false,
        "ssh",
        1,
        1,
        "agentspace: Removing the old staging folder on the instance failed.",
        &["ssh#1"],
    );
}

#[test]
fn step_3_an_ssh_that_cannot_connect_fails_its_step_the_same_way() {
    case(
        false,
        false,
        "ssh",
        1,
        255,
        "agentspace: Removing the old staging folder on the instance failed.",
        &["ssh#1"],
    );
}

#[test]
fn step_3_a_failed_copy_of_box_system() {
    case(
        false,
        false,
        "scp",
        1,
        1,
        "agentspace: Copy to the instance failed.",
        &["ssh#1", "scp#1"],
    );
}

#[test]
fn step_4_a_failed_copy_of_the_cas_public_key() {
    case(
        false,
        false,
        "scp",
        2,
        1,
        "agentspace: Copying the CA's public key to the instance failed.",
        &["ssh#1", "scp#1", "scp#2"],
    );
}

#[test]
fn step_5_a_failed_install_ends_sync_and_leaves_the_staging_folder() {
    // No call follows the install's, so nothing removes the staging folder: the next sync's
    // step 3 does.
    case(
        false,
        true,
        "ssh",
        2,
        1,
        "agentspace: Install on the instance failed.",
        &["ssh#1", "scp#1", "scp#2", "ssh#2"],
    );
}

#[test]
fn step_6_a_failed_copy_of_claude_md() {
    case(
        false,
        true,
        "scp",
        3,
        1,
        "agentspace: Copying CLAUDE.md to the instance failed.",
        &["ssh#1", "scp#1", "scp#2", "ssh#2", "ssh#3", "scp#3"],
    );
}

#[test]
fn step_6_the_folder_steps_exit_code_is_not_checked() {
    // B-34: the copy that follows is the check, so a failing folder step does not fail sync, and
    // what the step writes to stderr passes through.
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_claude_md();
    rig.rule("ssh", 3, "", 1, "", "folder step stand-in line\\n");
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_sync(&say),
            "A failing folder step MUST NOT fail sync",
        );
        let log = rig.log();
        rig.check(
            log.calls("scp").len() == 3,
            "The copy of CLAUDE.md MUST still be made after a failing folder step",
        );
    }) else {
        return;
    };
    assert!(
        out.stderr.contains("folder step stand-in line"),
        "What the folder step writes to stderr MUST pass through"
    );
}

#[test]
fn step_7_a_failed_build_of_the_localswim_cli() {
    case(
        true,
        false,
        "go",
        1,
        1,
        "agentspace: Building the localswim CLI failed.",
        &["ssh#1", "scp#1", "scp#2", "ssh#2", "go#1"],
    );
}

#[test]
fn step_7_a_failed_copy_of_the_localswim_cli() {
    case(
        true,
        false,
        "scp",
        3,
        1,
        "agentspace: Copying the localswim CLI failed.",
        &["ssh#1", "scp#1", "scp#2", "ssh#2", "go#1", "scp#3"],
    );
}

#[test]
fn step_7_a_failed_install_of_the_localswim_cli() {
    case(
        true,
        false,
        "ssh",
        3,
        1,
        "agentspace: Installing the localswim CLI failed.",
        &["ssh#1", "scp#1", "scp#2", "ssh#2", "go#1", "scp#3", "ssh#3"],
    );
}

// ---------------------------------------------------------------- Step 1: the CA, first

fn ca_failure_case(setup: impl Fn(&Rig), spec_text: impl Fn(&Rig) -> Option<String>) {
    let rig = Rig::new(false);
    setup(&rig);
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync(&say), "sync");
        let want = match agentspace_ca::check_for_sync(&rig.ctx(), &ca_env()) {
            Err(w) => w,
            Ok(_) => panic!("The CA check passed, so this case is not a CA failure"),
        };
        rig.same(
            "sync's failure is the CA check's own message",
            &f.message,
            &want.message,
        );
        if let Some(text) = spec_text(rig) {
            rig.same("The spec's message", &f.message, &text);
        }
        let log = rig.log();
        rig.check(
            log.all_calls().is_empty(),
            "A failing CA check MUST end sync before the box, or AWS, is touched",
        );
        rig.check(
            say.progress_lines().is_empty(),
            "A failing CA check prints no progress",
        );
    });
}

#[test]
fn step_1_a_laptop_with_no_ca_fails_and_never_creates_one() {
    ca_failure_case(
        |_rig| {},
        |rig| {
            let key = rig
                .home
                .join(".agentspace")
                .join("ca")
                .join("agentspace_ca");
            Some(format!(
                "agentspace: {} does not exist; run ca.",
                key.display()
            ))
        },
    );
}

#[test]
fn step_1_sync_never_creates_a_ca() {
    let rig = Rig::new(false);
    rig.run(|rig| {
        let say = rig.say();
        let _ = rig.do_sync(&say);
        rig.check(
            !rig.home.join(".agentspace").join("ca").exists(),
            "sync MUST NOT create a CA",
        );
    });
}

#[test]
fn step_1_a_half_made_ca_fails_with_cas_message() {
    ca_failure_case(
        |rig| {
            rig.make_ca();
            rig.remove_home(".agentspace/ca/agentspace_ca.pub");
        },
        |rig| {
            let key = rig
                .home
                .join(".agentspace")
                .join("ca")
                .join("agentspace_ca");
            let public = rig
                .home
                .join(".agentspace")
                .join("ca")
                .join("agentspace_ca.pub");
            Some(format!(
                "agentspace: {} exists but {} does not. ca does not repair a half-made CA: restore the public key, or write it again from the private key with ssh-keygen -y, then run ca again.",
                key.display(),
                public.display()
            ))
        },
    );
}

#[test]
fn step_1_a_missing_ssh_keygen_fails_first_with_cas_step_1_message() {
    ca_failure_case(
        |rig| {
            rig.make_ca();
            rig.set_system_root(rig.root.join("no-system-root"));
        },
        |rig| {
            let keygen = rig
                .root
                .join("no-system-root")
                .join("System32")
                .join("OpenSSH")
                .join("ssh-keygen.exe");
            Some(format!(
                "agentspace: {} is missing; install the Windows OpenSSH client.",
                keygen.display()
            ))
        },
    );
}

#[test]
fn step_1_a_ca_in_a_git_working_tree_fails_as_ca_fails_it() {
    // `ca`'s step 2 is part of sync's check (FA-14): a `.git` entry above the CA's folder.
    ca_failure_case(
        |rig| {
            rig.make_ca();
            rig.mkdir_home(".git");
        },
        |rig| {
            let ca = rig.home.join(".agentspace").join("ca");
            Some(format!(
                "agentspace: {} is inside {}; the CA key MUST NOT live in a synced folder or in the repo.",
                ca.display(),
                rig.home.display()
            ))
        },
    );
}

// ---------------------------------------------------------------- Step 2: start and wait

#[test]
fn step_2_an_expired_sign_in_fails_with_the_sign_in_message_before_the_box_is_touched() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.rule(
        "aws",
        0,
        "describe-instances",
        255,
        "",
        "Unable to locate credentials\\n",
    );
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync(&say), "sync");
        rig.same("The failure message", &f.message, SIGN_IN_MESSAGE);
        rig.check(
            f.caller == CallerFailure::Same,
            "The sign-in failure's caller line is the person's message",
        );
        rig.check(
            rig.log().remote_calls().is_empty(),
            "No ssh or scp may start",
        );
    });
}

#[test]
fn step_2_an_instance_in_another_state_fails_without_touching_the_box() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.rule(
        "aws",
        0,
        "describe-instances",
        0,
        &json_instance("terminated"),
        "",
    );
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync(&say), "sync");
        rig.same(
            "The failure message",
            &f.message,
            &format!("agentspace: Instance {ID} is terminated."),
        );
        rig.check(
            rig.log().remote_calls().is_empty(),
            "No ssh or scp may start",
        );
    });
}

// ---------------------------------------------------------------- The operator folder (FA-16)

#[test]
fn an_operator_folder_that_cannot_be_written_fails_before_anything_is_copied() {
    let rig = Rig::new(false);
    rig.make_ca();
    // A file where the operator's folder has to be.
    rig.write_home(".agentspace/operator", b"not a folder");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync(&say), "sync");
        // The path is the first one whose creation or write failed: the operator's folder
        // itself, never a file or folder below it (operator.md, What the prototype took from its
        // checkout, FA-16; architect, 2026-11-19, for E0.06-T03-S02).
        let operator = rig.home.join(".agentspace").join("operator");
        rig.same(
            "The failure message, which names the operator's folder itself",
            &f.message,
            &format!("agentspace: Cannot write {}.", operator.display()),
        );
        let log = rig.log();
        rig.check(
            log.calls("scp").is_empty() && log.calls("go").is_empty(),
            "Nothing may be copied when the files it copies cannot be written",
        );
    });
}

#[test]
fn a_v0_06_system_folder_that_is_held_open_does_not_stop_a_sync_that_writes_its_own_folder() {
    // Until v0.07 sync removed whatever the operator's `system` folder held before it wrote the
    // files, and a file held open there failed it. From v0.07 each command writes into a folder of
    // its own, and what a v0.06 operator left is left alone (operator.md, The operator's folder,
    // one per command), so the same held file changes nothing.
    let rig = Rig::new(false);
    rig.make_ca();
    rig.write_home(".agentspace/operator/system/held.txt", b"held");
    rig.run(|rig| {
        use std::os::windows::fs::OpenOptionsExt;
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(
                rig.home
                    .join(".agentspace")
                    .join("operator")
                    .join("system")
                    .join("held.txt"),
            )
            .unwrap();
        let say = rig.say();
        let result = rig.do_sync(&say);
        drop(held);
        rig.succeeded(result, "sync");
        let log = rig.log();
        rig.check(
            log.calls("scp").len() == 2
                && common::expect::command_folder_of(&log.calls("scp")[0].args[4]).is_some(),
            "sync MUST copy box/system from the command's own folder",
        );
    });
}
