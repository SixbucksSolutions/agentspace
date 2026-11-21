//! `sync`'s calls, in order, with their arguments and messages (docs/operator.md, sync, steps 1 to
//! 8 and the table of its nine calls; The test seam; Environment variables; What sync installs).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every test runs `sync` in a child of the
//! test binary under a throwaway home, with stand-ins for the AWS CLI, ssh, scp and go (see
//! `common`). The push (call 9) is off unless a test turns it on, since `LOCALAPPDATA` is then
//! unset and so there is no receipt.
//!
//! No test here reaches the real `go` or `C:\Projects\localswim` (operator.md, The test seam, "Not
//! covered"): `go` is the rig's stand-in, handed to `sync` by the rig's `go` finder, and the
//! localswim checkout is a folder of the rig's own (`common`).
#![cfg(windows)]

mod common;

use common::expect::*;
use common::*;

#[test]
fn sync_makes_its_calls_in_order_with_their_arguments() {
    let rig = Rig::new(true);
    rig.make_ca();
    rig.with_claude_md();
    let out = rig.run(|rig| {
        let fp = fingerprint(rig);
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync");
        let log = rig.log();
        let built = built(rig);
        rig.same_list(
            "The order of sync's messages and calls",
            &timeline_of(&log, |_| true),
            &expected_timeline(&fp, true, built),
        );
        check_calls(rig, &log, true, built);
        rig.check(
            say.warnings().is_empty(),
            "sync, with the push off, MUST print no warning",
        );
        // The push is call 9, and `LOCALAPPDATA` is unset: no receipt, no push, no failure.
        rig.check(
            !log.calls("ssh")
                .iter()
                .any(|c| c.args.iter().any(|a| a == "BatchMode=yes")),
            "With no receipt the push MUST be skipped",
        );
    });
    drop(out);
}

#[test]
fn sync_starts_the_box_before_it_touches_it() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync");
        let log = rig.log();
        let first_remote = log
            .items
            .iter()
            .position(|i| matches!(i, Item::Call(r) if r.name != "aws"))
            .expect("sync started no ssh or scp");
        let aws_before = log.items[..first_remote]
            .iter()
            .filter(|i| matches!(i, Item::Call(r) if r.name == "aws"))
            .count();
        rig.check(
            aws_before >= 1,
            "The start-and-wait step MUST run before the first ssh or scp",
        );
        let first = log.calls("aws");
        rig.same_list(
            "The first AWS call",
            &first[0].args,
            &describe_instances_args(ID),
        );
        rig.check(
            log.calls("aws").iter().all(|c| {
                c.args.len() >= 2 && c.args.windows(2).any(|w| w == ["--region", "us-west-2"])
            }),
            "Every AWS call MUST pass --region us-west-2",
        );
    });
}

#[test]
fn sync_without_claude_md_skips_its_two_steps() {
    let rig = Rig::new(true);
    rig.make_ca();
    rig.run(|rig| {
        let fp = fingerprint(rig);
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync");
        let log = rig.log();
        let built = built(rig);
        rig.same_list(
            "The order of sync's messages and calls",
            &timeline_of(&log, |_| true),
            &expected_timeline(&fp, false, built),
        );
        check_calls(rig, &log, false, built);
    });
}

#[test]
fn sync_skips_the_localswim_cli_when_go_is_missing() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_claude_md();
    rig.run(|rig| {
        let fp = fingerprint(rig);
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "A missing go is not a failure");
        let log = rig.log();
        rig.same_list(
            "The order of sync's messages and calls",
            &timeline_of(&log, |_| true),
            &expected_timeline(&fp, true, false),
        );
        rig.check(log.calls("go").is_empty(), "Without go, nothing is built");
        check_calls(rig, &log, true, false);
        rig.check(
            say.progress_lines().iter().any(|l| l == SKIP_LINE),
            "The skip message MUST be printed as a progress line, exactly",
        );
    });
}

#[test]
fn sync_skips_the_localswim_cli_when_the_checkout_is_missing() {
    // `go` is found, and the rig's localswim folder is not there.
    let rig = Rig::new(true);
    rig.make_ca();
    rig.without_checkout();
    rig.run(|rig| {
        rig.check(
            !rig.localswim().exists() && !built(rig),
            "The rig's localswim folder MUST be gone for this case",
        );
        let fp = fingerprint(rig);
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "A missing checkout is not a failure");
        let log = rig.log();
        rig.same_list(
            "The order of sync's messages and calls",
            &timeline_of(&log, |_| true),
            &expected_timeline(&fp, false, false),
        );
        rig.check(
            log.calls("go").is_empty(),
            "Without the checkout, go MUST NOT run",
        );
        check_calls(rig, &log, false, false);
        rig.check(
            say.progress_lines().iter().any(|l| l == SKIP_LINE),
            "The skip message MUST be printed as a progress line, exactly",
        );
        rig.check(
            !rig.localswim().exists(),
            "sync MUST NOT create the localswim checkout",
        );
    });
}

#[test]
fn sync_builds_the_localswim_cli_exactly_as_fb14_gives_it() {
    // The stand-in go runs in the rig's own localswim folder, and writes the output file.
    let rig = Rig::new(true);
    rig.make_ca();
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync");
        let log = rig.log();
        let go = log.calls("go");
        rig.check(go.len() == 1, "sync MUST run go once");
        // From v0.07 the build's output is in the command's own folder (operator.md, The operator's
        // folder, one per command).
        let out = rig
            .home
            .join(".agentspace")
            .join("operator")
            .join("localswim-linux-arm64");
        let built_in = resolve(
            rig,
            &go[0].args[3],
            Folder::PerCommand,
            "The build's output file",
        );
        rig.check(
            go[0].args.len() == 5
                && go[0].args[0] == "build"
                && go[0].args[1] == "-trimpath"
                && go[0].args[2] == "-o"
                && go[0].args[4] == "./cmd/localswim",
            "The build MUST be exactly `go build -trimpath -o <out> ./cmd/localswim`, with no other flag",
        );
        rig.same_path("The build's output file", &built_in, &out);
        rig.same_folder(
            "The folder the build runs in, the localswim checkout",
            &go[0].cwd,
            &rig.localswim(),
        );
        rig.check(
            listing(&rig.localswim()).is_empty(),
            "The build MUST write nothing into the checkout but what go itself writes: the output goes to the operator's folder",
        );
        rig.check(
            go[0].goos == "linux" && go[0].goarch == "arm64" && go[0].cgo == "0",
            "The build MUST see GOOS=linux, GOARCH=arm64 and CGO_ENABLED=0",
        );
        // The three variables are for go build only: nothing started afterwards sees them.
        let all = log.all_calls();
        let at = all.iter().position(|c| c.name == "go").unwrap();
        let leaked = all[at + 1..]
            .iter()
            .filter(|c| c.goos != "<unset>" || c.goarch != "<unset>" || c.cgo != "<unset>")
            .count();
        rig.check(
            leaked == 0,
            "GOOS, GOARCH and CGO_ENABLED MUST be for go build only, and none of them may reach a later program",
        );
        let before = all[..at]
            .iter()
            .filter(|c| c.goos != "<unset>" || c.goarch != "<unset>" || c.cgo != "<unset>")
            .count();
        rig.check(before == 0, "The three variables MUST NOT be set before the build");
        // And the build's progress line.
        rig.check(
            say.progress_lines()
                .iter()
                .any(|l| l == "Building the localswim CLI for linux/arm64..."),
            "sync MUST print `Building the localswim CLI for linux/arm64...`",
        );
    });
}

#[test]
fn the_pinned_sync_is_that_sync_and_skips_the_cli_when_no_go_is_on_path() {
    // The pinned entry point itself, which finds `go` on `PATH` and builds from the spec's fixed
    // folder. It runs only where no `go` can be found (the rig refuses otherwise), so it skips the
    // build without looking at that folder; everything else is the same sync: the real CA check,
    // the start-and-wait, the calls in order, and the forced push over `agentspace`.
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_claude_md();
    rig.enable_push();
    rig.run(|rig| {
        let fp = fingerprint(rig);
        let say = rig.say();
        rig.succeeded(rig.do_pinned_sync(&say), "The pinned sync");
        let log = rig.log();
        let mut want = expected_timeline(&fp, true, false);
        want.push("ssh#4".to_string());
        rig.same_list(
            "The order of the pinned sync's messages and calls, the push's ssh last",
            &timeline_of(&log, |_| true),
            &want,
        );
        rig.same_list(
            "The first AWS call",
            &log.calls("aws")[0].args,
            &describe_instances_args(ID),
        );
        let last = log.remote_calls().into_iter().last().unwrap();
        rig.check(
            last.args.len() == 6
                && last.args[..5]
                    == svec(&[
                        "-o",
                        "ClearAllForwardings=yes",
                        "-o",
                        "BatchMode=yes",
                        "agentspace",
                    ])[..],
            "The pinned sync's last call MUST be the push's ssh over `agentspace`",
        );
        rig.check(log.calls("go").is_empty(), "No go was on PATH, so none ran");
        rig.check(
            say.progress_lines().iter().any(|l| l == SKIP_LINE),
            "With no go on PATH the pinned sync MUST print the skip message",
        );
        check_environment(rig, &log);
        // Its push is forced: a second run pushes again, whatever the stamp says.
        rig.succeeded(rig.do_pinned_sync(&say), "The second pinned sync");
        let pushes = rig
            .log()
            .calls("ssh")
            .iter()
            .filter(|c| c.args.iter().any(|a| a == "BatchMode=yes"))
            .count();
        rig.check(
            pushes == 2,
            &format!("The pinned sync's push is forced, so two runs MUST push twice; they pushed {pushes} times"),
        );
    });
}

#[test]
fn every_program_sync_starts_gets_the_environment_rules() {
    let rig = Rig::new(true);
    rig.make_ca();
    rig.with_claude_md();
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync");
        let log = rig.log();
        check_environment(rig, &log);
        // The aws, ssh, scp and go stand-ins all ran, so every kind was checked.
        for name in ["aws", "ssh", "scp", "go"] {
            rig.check(
                !log.calls(name).is_empty(),
                &format!("No {name} call was seen"),
            );
        }
    });
}

#[test]
fn sync_passes_the_remote_steps_output_through() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.rule(
        "ssh",
        2,
        "",
        0,
        "Installed: stand-in line\\n",
        "install.sh: stand-in line\\n",
    );
    let out = rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync");
    });
    let Some(out) = out else { return };
    assert!(
        out.stdout.contains("Installed: stand-in line"),
        "The install's stdout MUST reach sync's stdout"
    );
    assert!(
        out.stderr.contains("install.sh: stand-in line"),
        "The install's stderr MUST reach sync's stderr"
    );
}

#[test]
fn sync_with_a_quiet_say_prints_nothing_of_its_own() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_claude_md();
    let Some(out) = rig.run(|rig| {
        let say = rig.quiet_say();
        rig.succeeded(rig.do_sync(&say), "sync");
    }) else {
        return;
    };
    for text in [
        "Copying box/system",
        "Copying the agentspace SSH CA",
        "Copying ~/.claude",
        "Skipping the localswim CLI",
    ] {
        assert!(
            !out.stdout.contains(text) && !out.stderr.contains(text),
            "sync MUST speak only through its Say: `{text}` reached the streams"
        );
    }
}

#[test]
fn the_box_system_folder_sync_copies_is_byte_identical_to_the_repositorys() {
    let rig = Rig::new(false);
    rig.make_ca();
    // What a v0.06 operator left in the operator's folder is left alone, and never reaches the
    // copy: the command writes into a folder of its own (operator.md, The operator's folder, one
    // per command: "Anything else there ... is left").
    rig.write_home(".agentspace/operator/system/stale-file.txt", b"old");
    rig.write_home(
        ".agentspace/operator/system/stale-dir/deeper/old.sh",
        b"old",
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync");
        let log = rig.log();
        let (arg, entries) = log
            .dir_snap("scp", 1)
            .expect("The first scp MUST copy a folder that exists when it starts");
        let source = resolve(rig, &arg, Folder::PerCommand, "The folder the first scp copies");
        rig.same_path(
            "The folder the first scp copies",
            &source,
            &rig.home.join(".agentspace").join("operator").join("system"),
        );
        rig.check(
            rig.home_path(".agentspace/operator/system/stale-file.txt").is_file()
                && rig
                    .home_path(".agentspace/operator/system/stale-dir/deeper/old.sh")
                    .is_file(),
            "What a v0.06 operator left in the operator's folder MUST be left alone",
        );
        let want = listing(&repo_root().join("box").join("system"));
        rig.check(!want.is_empty(), "The repository's box/system is empty here");
        rig.check(
            entries == want,
            &format!(
                "The copied folder MUST hold exactly the repository's box/system files, byte for byte; it held {} files, the repository has {}",
                entries.len(),
                want.len()
            ),
        );
    });
}

#[test]
fn the_ca_public_key_sync_copies_is_the_laptops_own() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync");
        let log = rig.log();
        let snap = log.file_snap("scp", 2);
        let on_disk = std::fs::read(
            rig.home
                .join(".agentspace")
                .join("ca")
                .join("agentspace_ca.pub"),
        )
        .unwrap();
        rig.check(
            snap.len() == 1 && snap[0].1 == on_disk.len() as u64 && snap[0].2 == fnv(&on_disk),
            "The second scp MUST copy the CA's public key file as it is",
        );
    });
}

#[test]
fn sync_pushes_the_token_forced_as_its_last_call() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_claude_md();
    rig.enable_push();
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync");
        let log = rig.log();
        let last = log.remote_calls().into_iter().last().unwrap();
        rig.check(
            last.name == "ssh" && last.args.len() == 6,
            "Call 9, the push's ssh, MUST be the last call, with six arguments",
        );
        rig.check(
            last.args[0] == "-o"
                && last.args[1] == "ClearAllForwardings=yes"
                && last.args[2] == "-o"
                && last.args[3] == "BatchMode=yes"
                && last.args[4] == "agentspace",
            "The push MUST be `ssh -o ClearAllForwardings=yes -o BatchMode=yes agentspace <remote command>`",
        );
        rig.check(
            last.args[5].contains(ZONE) && !last.args[5].contains(TOKEN),
            "The push's remote command MUST name the zone and MUST NOT carry the token",
        );
        let stdin = std::fs::read(rig.root.join("stdin.bin")).unwrap();
        rig.check(
            stdin == format!("LOCALSWIM_URL={BASE}\nLOCALSWIM_TOKEN={TOKEN}\n").as_bytes(),
            "The push's stdin MUST be exactly the two lines",
        );
        rig.check(
            !log.calls("aws").iter().any(|c| c.args.iter().any(|a| a.contains(TOKEN)))
                && !log
                    .all_calls()
                    .iter()
                    .any(|c| c.args.iter().any(|a| a.contains(TOKEN))),
            "The token MUST NOT be in any program's arguments",
        );
        rig.check(say.warnings().is_empty(), "A confirmed push prints no warning");
        let ssh_before = log.calls("ssh").len();
        // The push is confirmed and stamped now, so an unforced push would be skipped. sync's is
        // forced: the second run MUST push again.
        rig.succeeded(rig.do_sync(&say), "The second sync");
        let log = rig.log();
        let pushes = log
            .calls("ssh")
            .iter()
            .filter(|c| c.args.iter().any(|a| a == "BatchMode=yes"))
            .count();
        rig.check(
            pushes == 2 && log.calls("ssh").len() > ssh_before,
            &format!("sync's push is forced, so two runs MUST push twice; they pushed {pushes} times"),
        );
    });
}

#[test]
fn a_push_that_the_box_does_not_confirm_warns_and_does_not_fail_sync() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.enable_push();
    // The box prints no hash line.
    rig.rule("ssh", 0, "BatchMode=yes", 0, "", "");
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync MUST NOT fail on the push");
        let warnings = say.warnings();
        rig.check(
            warnings
                .iter()
                .any(|w| w.contains("The instance did not confirm the localswim token")),
            "An unconfirmed push MUST warn that the instance did not confirm the localswim token",
        );
    });
}
