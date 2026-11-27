//! `up`'s wait and the guard's horizon, by the host's platform (docs/operator.md, The platform seam:
//! the Session Manager document, the patch check's question, the patch messages and the horizon
//! command rows; What up waits for; guard, step 7; Calls by command; E0.12-T03-S03, #0256, pins P17
//! and P18 of v0.12-T02).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Nothing here touches a file or AWS: the
//! fake AWS answers by script and records each call.
//!
//! - **Both Linux platforms send what `start_and_wait` and `send_horizon` send**, call for call and
//!   line for line: `AWS-RunShellScript` and the shorthand `--parameters commands=<command>`.
//! - **A platform whose document is not `AWS-RunShellScript` is sent the JSON value
//!   `{"commands":["<command>"]}`**, never the shorthand, so quotes, brackets and commas reach the
//!   host whole. The tests' own PowerShell platform holds all four in its commands.
//! - **The phase message and the deadline's log command are the platform's.**
//! - **A row that fails ends the step before any call.**

mod common;
use common::*;

use agentspace_operator_api::{
    Bootstrap, CallerFailure, Failure as SeamFailure, HostPlatform, PatchMessages, SyncPlan,
};
use agentspace_platforms::{LINUX_AMD64, LINUX_ARM64};

const DESCRIBE: &str = "ec2 describe-instances";
const INFO: &str = "ssm describe-instance-information";
const SEND: &str = "ssm send-command";
const POLL: &str = "ssm get-command-invocation";

/// The question the tests' PowerShell platform sends: quotes, brackets, a comma and backslashes.
const QUESTION: &str = r#"& "C:\Program Files\PowerShell\7\pwsh.exe" -File C:\x\check.ps1, [y]"#;
const PHASE: &str = "Patching the test platform...";
const LOG: &str = "cat C:/x/patch.log";
const FAILED: &str = "agentspace: The test platform's row failed.";

/// A platform of the tests' own, on `AWS-RunPowerShellScript`.
struct PowerShell {
    fails: bool,
}

impl PowerShell {
    fn failure() -> SeamFailure {
        SeamFailure {
            message: FAILED.to_string(),
            caller: CallerFailure::NotAvailable,
        }
    }
}

impl HostPlatform for PowerShell {
    fn platform(&self) -> &'static str {
        "test/platform"
    }
    fn image_parameter(&self) -> Option<&'static str> {
        None
    }
    fn new_stack_instance_type(&self) -> Option<&'static str> {
        None
    }
    fn new_stack_extra_parameters(&self) -> &'static [&'static str] {
        &[]
    }
    fn login_user(&self) -> &'static str {
        "agent"
    }
    fn ssm_document(&self) -> &'static str {
        "AWS-RunPowerShellScript"
    }
    fn remote_command(&self, script: &str) -> Result<String, SeamFailure> {
        Ok(script.to_string())
    }
    fn bootstrap(&self) -> Result<Bootstrap, SeamFailure> {
        Err(PowerShell::failure())
    }
    fn sync_plan(&self) -> Result<SyncPlan, SeamFailure> {
        Err(PowerShell::failure())
    }
    fn patch_check_command(&self) -> Result<String, SeamFailure> {
        if self.fails {
            Err(PowerShell::failure())
        } else {
            Ok(QUESTION.to_string())
        }
    }
    fn patch_messages(&self) -> PatchMessages {
        PatchMessages {
            phase: PHASE.to_string(),
            log_command: LOG.to_string(),
        }
    }
    fn horizon_command(&self, horizon: u64) -> Result<String, SeamFailure> {
        if self.fails {
            Err(PowerShell::failure())
        } else {
            Ok(format!(
                r#"[IO.File]::WriteAllText("C:\x\lease-horizon", "{horizon}", [Text.Encoding]::ASCII)"#
            ))
        }
    }
    fn grow_script(&self) -> Result<String, SeamFailure> {
        Err(PowerShell::failure())
    }
    fn push_zone(&self, _: &str, _: &str) -> Result<String, SeamFailure> {
        Err(PowerShell::failure())
    }
    fn push_script(&self, _: &str) -> Result<String, SeamFailure> {
        Err(PowerShell::failure())
    }
}

/// A stopped host that comes up, whose patch check answers `words` in turn.
fn stopped_then(sim: &Sim, words: &[&str]) {
    sim.script(DESCRIBE, vec![state("stopped")]);
    sim.script("ec2 start-instances", vec![Ok(json!({}))]);
    online_with(sim, words);
}

fn cache(sim: &Sim) -> FakeCache {
    FakeCache::new(
        sim,
        CacheFile::Bytes(format!(r#"{{"version":0,"instance_id":"{ID}"}}"#).into_bytes()),
    )
}

/// `start_and_wait_on` for `platform`, on the box's stack.
fn run_on(sim: &Sim, platform: &dyn HostPlatform) -> Result<String, Failure> {
    agentspace_box::start_and_wait_on(
        &sim.aws,
        &sim.say,
        &*sim.clock,
        &cache(sim),
        "agentbox",
        platform,
    )
}

// ---------------------------------------------------------------- Both Linux platforms

#[test]
fn both_linux_platforms_make_the_calls_and_say_the_lines_start_and_wait_does() {
    let reference = Sim::new(Mode::Person);
    stopped_then(&reference, &["pending", "patched"]);
    let id = agentspace_box::start_and_wait(
        &reference.aws,
        &reference.say,
        &*reference.clock,
        &cache(&reference),
        "agentbox",
    )
    .unwrap();
    assert_eq!(id, ID);
    assert!(
        reference.count(SEND) == 2,
        "The reference run asks the patch question twice"
    );
    for (name, platform) in [
        ("linux/arm64", &LINUX_ARM64 as &dyn HostPlatform),
        ("linux/amd64", &LINUX_AMD64 as &dyn HostPlatform),
    ] {
        let sim = Sim::new(Mode::Person);
        stopped_then(&sim, &["pending", "patched"]);
        assert_eq!(run_on(&sim, platform), Ok(ID.to_string()), "{name}");
        assert_eq!(
            sim.calls(),
            reference.calls(),
            "{name}: every call, byte for byte"
        );
        assert_eq!(
            sim.person_lines(),
            reference.person_lines(),
            "{name}: every line"
        );
    }
}

#[test]
fn both_linux_platforms_send_the_question_on_the_shell_document_with_the_shorthand() {
    for (name, platform) in [
        ("linux/arm64", &LINUX_ARM64 as &dyn HostPlatform),
        ("linux/amd64", &LINUX_AMD64 as &dyn HostPlatform),
    ] {
        let sim = Sim::new(Mode::Person);
        stopped_then(&sim, &["patched"]);
        run_on(&sim, platform).unwrap();
        let send = &sim.calls_of(SEND)[0];
        assert_eq!(
            send.args[..7].to_vec(),
            vec![
                "--instance-ids",
                ID,
                "--document-name",
                "AWS-RunShellScript",
                "--comment",
                "agentspace up: patch check",
                "--parameters",
            ],
            "{name}"
        );
        let want = format!("commands={}", platform.patch_check_command().unwrap());
        assert_eq!(send.args[7], want, "{name}: the shorthand, byte for byte");
        assert!(!send.args[7].starts_with('{'), "{name}");
    }
}

// ---------------------------------------------------------------- A PowerShell document

#[test]
fn a_powershell_platform_gets_its_question_as_a_json_value_of_parameters() {
    let sim = Sim::new(Mode::Person);
    stopped_then(&sim, &["patched"]);
    run_on(&sim, &PowerShell { fails: false }).unwrap();
    let send = &sim.calls_of(SEND)[0];
    assert_eq!(
        send.args[..7].to_vec(),
        vec![
            "--instance-ids",
            ID,
            "--document-name",
            "AWS-RunPowerShellScript",
            "--comment",
            "agentspace up: patch check",
            "--parameters",
        ]
    );
    assert_eq!(send.args.len(), 8);
    assert!(
        !send.args[7].starts_with("commands="),
        "The shorthand MUST NOT be used: {:?}",
        send.args[7]
    );
    let value: Value = serde_json::from_str(&send.args[7]).expect("The parameters are JSON");
    assert_eq!(
        value,
        json!({ "commands": [QUESTION] }),
        "The JSON value {{\"commands\":[\"<command>\"]}} holds the question whole"
    );
}

#[test]
fn a_powershell_platform_says_its_own_phase_while_the_patch_is_pending() {
    let sim = Sim::new(Mode::Person);
    stopped_then(&sim, &["pending", "patched"]);
    run_on(&sim, &PowerShell { fails: false }).unwrap();
    let lines = sim.person_lines();
    assert!(
        lines.iter().any(|l| l == PHASE),
        "The phase message is the platform's: {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("Patching Ubuntu")),
        "No Linux phase on another platform: {lines:?}"
    );
}

#[test]
fn the_deadline_names_the_platforms_log_command_and_a_linux_platform_names_journalctl() {
    for (name, platform, log) in [
        (
            "test",
            &PowerShell { fails: false } as &dyn HostPlatform,
            LOG,
        ),
        (
            "linux/arm64",
            &LINUX_ARM64 as &dyn HostPlatform,
            "journalctl -u agentspace-patch",
        ),
        (
            "linux/amd64",
            &LINUX_AMD64 as &dyn HostPlatform,
            "journalctl -u agentspace-patch",
        ),
    ] {
        let sim = Sim::new(Mode::Person);
        sim.script(DESCRIBE, vec![state("pending")]);
        online_with(&sim, &["pending"]);
        let failure = run_on(&sim, platform).unwrap_err();
        assert_eq!(
            failure.message,
            format!(
                "agentspace: {ID} was not ready within 15 minutes; check: ssh agentspace {log}"
            ),
            "{name}"
        );
        assert!(sim.count(POLL) > 0 && sim.count(INFO) > 0, "{name}");
    }
}

#[test]
fn a_row_that_fails_ends_the_step_before_any_call_or_line() {
    let sim = Sim::new(Mode::Person);
    stopped_then(&sim, &["patched"]);
    let failure = run_on(&sim, &PowerShell { fails: true }).unwrap_err();
    assert_eq!(failure.message, FAILED);
    assert!(sim.calls().is_empty(), "No call: {:?}", sim.keys());
    assert!(sim.person_lines().is_empty());
}

// ---------------------------------------------------------------- The guard's horizon

const HORIZON: i64 = 1_795_089_600;

fn send_horizon(sim: &Sim, platform: &dyn HostPlatform, horizon: i64) -> Result<(), Failure> {
    sim.script(SEND, vec![sent("cmd-0")]);
    agentspace_box::send_horizon_on(&sim.aws, ID, platform, horizon)
}

#[test]
fn both_linux_platforms_send_the_horizon_as_send_horizon_does() {
    let reference = Sim::new(Mode::Person);
    reference.script(SEND, vec![sent("cmd-0")]);
    agentspace_box::send_horizon(&reference.aws, ID, HORIZON).unwrap();
    for (name, platform) in [
        ("linux/arm64", &LINUX_ARM64 as &dyn HostPlatform),
        ("linux/amd64", &LINUX_AMD64 as &dyn HostPlatform),
    ] {
        let sim = Sim::new(Mode::Person);
        send_horizon(&sim, platform, HORIZON).unwrap();
        assert_eq!(sim.calls(), reference.calls(), "{name}");
        let send = &sim.calls_of(SEND)[0];
        assert_eq!(send.args[3], "AWS-RunShellScript", "{name}");
        assert_eq!(send.args[5], "agentspace guard: lease horizon", "{name}");
        assert!(
            send.args[7].starts_with("commands=umask 022; mkdir -p -m 755 /run/agentspace"),
            "{name}: {:?}",
            send.args[7]
        );
        assert!(
            send.args[7].contains(&format!("echo {HORIZON} > ")),
            "{name}"
        );
    }
}

#[test]
fn a_powershell_platform_gets_the_horizon_as_a_json_value_of_parameters() {
    let sim = Sim::new(Mode::Person);
    send_horizon(&sim, &PowerShell { fails: false }, HORIZON).unwrap();
    let send = &sim.calls_of(SEND)[0];
    assert_eq!(
        send.args[..7].to_vec(),
        vec![
            "--instance-ids",
            ID,
            "--document-name",
            "AWS-RunPowerShellScript",
            "--comment",
            "agentspace guard: lease horizon",
            "--parameters",
        ]
    );
    assert!(!send.args[7].starts_with("commands="), "{:?}", send.args[7]);
    let value: Value = serde_json::from_str(&send.args[7]).expect("The parameters are JSON");
    let command = format!(
        r#"[IO.File]::WriteAllText("C:\x\lease-horizon", "{HORIZON}", [Text.Encoding]::ASCII)"#
    );
    assert_eq!(value, json!({ "commands": [command] }));
}

#[test]
fn a_negative_horizon_and_a_failing_row_make_no_call() {
    let sim = Sim::new(Mode::Person);
    let failure = send_horizon(&sim, &LINUX_AMD64, -1).unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: The lease horizon cannot be negative."
    );
    assert!(sim.calls().is_empty());

    let sim = Sim::new(Mode::Person);
    let failure = send_horizon(&sim, &PowerShell { fails: true }, HORIZON).unwrap_err();
    assert_eq!(failure.message, FAILED);
    assert!(sim.calls().is_empty());
}
