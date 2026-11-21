//! What stops `sync -Host` from reaching a host: a CA that fails its check, a key that does not
//! load, an alias file that is not this host's, and a ProxyCommand program that is not there
//! (docs/operator.md, How the operator reaches a host; sync, step 1; docs/state.md, A host's
//! operator alias).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `sync_target_using` for `Target::Host` in a child of the test binary, under a throwaway home,
//! and checks the message and that no ssh or scp was started.
#![cfg(windows)]

mod common;
mod hostcalls;

use std::fs;

use agentspace_sync::Target;
use common::hosts::*;
use common::*;
use hostcalls::setup::*;

#[test]
fn a_key_that_does_not_load_fails_the_command_naming_the_file_and_is_left_as_it_is() {
    let rig = sync_rig(false);
    rig.write_home(".agentspace/hosts/demo/id_ed25519", b"not a key\n");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        rig.check(
            f.message.starts_with("agentspace: ") && f.message.contains("id_ed25519"),
            &format!("The failure MUST name the key's file: `{}`", f.message),
        );
        rig.check(
            fs::read(rig.host_folder(PROJECT).join("id_ed25519")).unwrap() == b"not a key\n",
            "An existing key MUST never be replaced",
        );
        rig.check(
            rig.log().remote_calls().is_empty(),
            "No ssh or scp MUST be started when the key cannot be used",
        );
    });
}

#[test]
fn a_ca_that_fails_its_check_ends_a_hosts_sync_before_anything_is_touched() {
    let rig = sync_rig(false);
    rig.remove_home(".agentspace/ca");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        let want = match agentspace_ca::check_for_sync(&rig.ctx(), &ca_env()) {
            Err(w) => w,
            Ok(_) => panic!("The CA check passed, so this case is not a CA failure"),
        };
        rig.same("The failure message", &f.message, &want.message);
        rig.check(
            rig.log().all_calls().is_empty(),
            "A failing CA check MUST end sync before the host, or AWS, is touched",
        );
        rig.check(
            !rig.alias_file(PROJECT).exists() && !rig.host_folder(PROJECT).exists(),
            "No key, certificate or alias MUST be written when the CA fails its check",
        );
    });
}

#[test]
fn a_file_at_the_alias_path_that_is_not_this_hosts_is_left_as_it_is() {
    let rig = sync_rig(false);
    rig.run(|rig| {
        let path = rig.alias_file(PROJECT);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let cases: [&[u8]; 3] = [
            b"# agentspace host other; somebody else's\nHost x\n",
            b"# agentspace lease 3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90; a lease's\nHost x\n",
            b"Host agentspace-host.demo\n  User person\n",
        ];
        for bytes in cases {
            fs::write(&path, bytes).unwrap();
            let say = rig.say();
            let f = rig.failure(
                rig.do_sync_target(&say, Target::Host(PROJECT)),
                "sync -Host",
            );
            same_message(
                rig,
                "The failure message",
                &f.message,
                &format!(
                    "agentspace: {} is not agentspace's alias for '{PROJECT}'; it was left as it is.",
                    path.display()
                ),
            );
            rig.check(
                fs::read(&path).unwrap() == bytes,
                "A file that is not this host's alias MUST be left as it is",
            );
            rig.check(
                rig.log().remote_calls().is_empty(),
                "No ssh or scp MUST be started when the alias cannot be written",
            );
        }
    });
}

#[test]
fn a_proxy_program_that_is_not_beside_the_operator_fails_the_command_with_its_path() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.beside_operator(false);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID2)]);
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        let program = rig.operator_bin().join("agentspace-ssh-proxy.exe");
        same_message(
            rig,
            "The failure message",
            &f.message,
            &format!(
                "agentspace: {} is missing; the operator's alias for a host needs it beside the operator.",
                program.display()
            ),
        );
        rig.check(
            rig.log().remote_calls().is_empty(),
            "No ssh or scp MUST be started when the alias cannot be written",
        );
    });
}

#[test]
fn an_operator_folder_that_cannot_be_made_fails_the_command_naming_it() {
    // A file where the operator's folder has to be: the first folder that cannot be made is named
    // itself (operator.md, What the prototype took from its checkout, FA-16).
    let rig = sync_rig(false);
    rig.write_home(".agentspace/operator", b"a file where a folder belongs");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        same_message(
            rig,
            "The failure message",
            &f.message,
            &format!("agentspace: Cannot write {}.", rig.operator_dir().display()),
        );
        let log = rig.log();
        rig.check(
            log.calls("scp").is_empty() && log.calls("go").is_empty(),
            "Nothing MUST be copied when the command's folder cannot be made",
        );
    });
}
