//! The start-and-wait step (operator.md, Starting the box and waiting for it; What up waits for).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

use std::time::Duration;

use agentspace_operator_api::{
    Aws, Call, CallerFailure, CallerLine, Failure, HostPlatform, Output, PatchMessages, Say,
};
use serde_json::Value;

use crate::answer::{
    self, COMMAND_ID, INSTANCE_INFORMATION_LIST, LAUNCH_TIME, PING_STATUS, STATE_NAME,
};
use crate::clock::Clock;
use crate::launch_time;
use crate::run_command;

/// The comment the patch question is sent with (operator.md, What up waits for).
pub const PATCH_COMMENT: &str = "agentspace up: patch check";

/// The patch question, run as root by `AWS-RunShellScript` on today's box, and asked by `up` and by
/// `sync`'s patch wait alike (operator.md, What up waits for; P21, P24): `patched` when this boot's
/// marker exists; otherwise, for a unit in systemd's `failed` state, `pending` when a job that
/// will run it is queued for it and `failed` when none is; otherwise `pending` when the patch unit
/// is enabled, and `unknown` when it is not. It reads the marker, the unit's state, whether such a
/// job is queued and whether the unit is enabled, in that order.
///
/// A queued job that will run the unit is a line of `systemctl list-jobs` for the unit that holds
/// the word `start` or the word `restart`, so a check made between the install's `systemctl start
/// --no-block` and the run's start never reads the earlier run's failure as this run's (pin P24),
/// and a queued `restart` counts as a start (architect, 2026-11-22, for v0.08-T03). A job of
/// another type, such as `stop` or `reload`, does not.
///
/// It MUST hold no `,`, `[`, `]` or `=`, on which the AWS CLI's shorthand syntax for
/// `--parameters` splits, and none of what FA-15 forbids: no `%`, no double quote, no control
/// character, and no backslash at its end (operator.md, What the laptop sends). A test checks it,
/// and another runs it under bash with a stand-in `systemctl`.
pub const PATCH_QUESTION: &str = "if test -e /run/agentspace/patched; then echo patched; \
elif systemctl is-failed --quiet agentspace-patch.service; then \
if systemctl list-jobs --no-legend agentspace-patch.service | grep -qw -e start -e restart; \
then echo pending; else echo failed; fi; \
elif systemctl is-enabled --quiet agentspace-patch.service; then echo pending; \
else echo unknown; fi";

/// The wait's limit, from its start (step 3).
const WAIT_LIMIT: Duration = Duration::from_secs(15 * 60);
/// The pause between two checks (step 3).
const CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// How long a box MUST have been running to go unasked (step 2).
const RUNNING_LONG_ENOUGH: Duration = Duration::from_secs(20 * 60);
/// The pause between two polls of the invocation (What up waits for).
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// The most polls of one invocation; that many without an end count as `pending`.
const POLL_LIMIT: usize = 20;

const WAITING_FOR_SESSION_MANAGER: &str = "Waiting for Session Manager...";
/// Today's `linux/arm64` phase message, which the platform's patch messages now give (the seam's
/// patch messages row); the tests still name it.
#[cfg_attr(not(test), allow(dead_code))]
const PATCHING: &str = "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...";

const DESCRIBE_INSTANCES: &str = "ec2 describe-instances";
const DESCRIBE_INSTANCE_INFORMATION: &str = "ssm describe-instance-information";
const SEND_COMMAND: &str = "ssm send-command";

/// The start-and-wait step that `up`, `sync` and every nested `up` share, for `instance_id`
/// (operator.md, Starting the box and waiting for it):
///
/// 1. Reads the instance's state. `running` and `pending` start nothing. `stopped` prints
///    `Starting <id>...` and starts it. `stopping` prints `Waiting for <id> to finish
///    stopping...`, runs `ec2 wait instance-stopped` without checking how it ended, then prints
///    `Starting <id>...` and starts it. Any other state fails with `agentspace: Instance <id> is
///    <state>.`
/// 2. Waits unless the state was `running` and the launch time, read by a second
///    `ec2 describe-instances` that runs only for a `running` box (FB-09), is 20 minutes ago or
///    more.
/// 3. Waits for at most 15 minutes, checking every 5 seconds, printing each message only when it
///    differs from the last one printed.
/// 4. Prints `Ready: <id>`.
///
/// In caller mode [`Say`] prints the caller's lines in place of these, and each [`Failure`]
/// carries the caller's line in place of its message.
///
/// It is `linux/arm64`'s: this crate's `start_and_wait_for_on` on `LINUX_ARM64`.
pub fn start_and_wait_for(
    aws: &dyn Aws,
    say: &dyn Say,
    clock: &dyn Clock,
    instance_id: &str,
) -> Result<(), Failure> {
    start_and_wait_for_on(
        aws,
        say,
        clock,
        instance_id,
        &agentspace_platforms::LINUX_ARM64,
    )
}

/// A platform's patch check, read from its rows before the step makes any call, so a row that
/// fails ends the step with nothing sent (operator.md, The platform seam).
pub(crate) struct PatchCheck {
    /// The Session Manager document the question is sent through.
    document: &'static str,
    /// The `--parameters` value that carries the question.
    parameters: String,
    /// The phase message and the deadline's log command.
    messages: PatchMessages,
}

impl PatchCheck {
    /// The patch check of `platform`: [`HostPlatform::patch_check_command`] through
    /// [`HostPlatform::ssm_document`], in the parameters [`run_command::parameters`] gives (on
    /// Linux, `commands=<question>`, as before v0.12), and [`HostPlatform::patch_messages`].
    pub(crate) fn of(platform: &dyn HostPlatform) -> Result<PatchCheck, Failure> {
        let question = platform.patch_check_command()?;
        Ok(PatchCheck {
            document: platform.ssm_document(),
            parameters: run_command::parameters(platform, &question),
            messages: platform.patch_messages(),
        })
    }
}

/// [`start_and_wait_for`] on the host's `platform`: its patch check sends
/// [`HostPlatform::patch_check_command`] through [`HostPlatform::ssm_document`], and its phase
/// message and deadline are worded with [`HostPlatform::patch_messages`] (operator.md, The
/// platform seam). On both Linux platforms every call and message is today's, byte for byte. A
/// platform whose patch check fails ends the step with that failure before any call.
pub(crate) fn start_and_wait_for_on(
    aws: &dyn Aws,
    say: &dyn Say,
    clock: &dyn Clock,
    instance_id: &str,
    platform: &dyn HostPlatform,
) -> Result<(), Failure> {
    let check = PatchCheck::of(platform)?;
    start_and_wait_with(aws, say, clock, instance_id, &check)
}

/// [`start_and_wait_for_on`] with the platform's patch check already read.
pub(crate) fn start_and_wait_with(
    aws: &dyn Aws,
    say: &dyn Say,
    clock: &dyn Clock,
    instance_id: &str,
    check: &PatchCheck,
) -> Result<(), Failure> {
    let id = instance_id;
    let state = describe_instance(aws, id, STATE_NAME)?;
    match state.as_str() {
        "running" | "pending" => {}
        "stopped" => {
            say.progress(&format!("Starting {id}..."), Some(CallerLine::Starting));
            start_instance(aws, id)?;
        }
        "stopping" => {
            say.progress(
                &format!("Waiting for {id} to finish stopping..."),
                Some(CallerLine::Starting),
            );
            // The wait's exit code is not checked (B-34): the start request after it is the check.
            let _ = aws.call(&Call {
                service: "ec2",
                operation: "wait",
                args: &["instance-stopped", "--instance-ids", id],
                output: Output::Ignore,
            });
            say.progress(&format!("Starting {id}..."), Some(CallerLine::Starting));
            start_instance(aws, id)?;
        }
        other => {
            return Err(Failure {
                message: format!("agentspace: Instance {id} is {other}."),
                caller: CallerFailure::NotAvailable,
            });
        }
    }

    if state != "running" || !running_long_enough(aws, clock, id)? {
        wait(aws, say, clock, id, check)?;
    }

    say.progress(&format!("Ready: {id}"), Some(CallerLine::Ready));
    Ok(())
}

/// One `ec2 describe-instances --instance-ids <id>` (JSON), and the one member this step reads
/// from it (FB-08).
fn describe_instance(aws: &dyn Aws, id: &str, member: answer::Member) -> Result<String, Failure> {
    let answer = aws.call(&Call {
        service: "ec2",
        operation: "describe-instances",
        args: &["--instance-ids", id],
        output: Output::Json,
    })?;
    answer::string(&answer, DESCRIBE_INSTANCES, member).map(str::to_string)
}

/// `ec2 start-instances --instance-ids <id>` (JSON), read no further than its exit code.
fn start_instance(aws: &dyn Aws, id: &str) -> Result<(), Failure> {
    aws.call(&Call {
        service: "ec2",
        operation: "start-instances",
        args: &["--instance-ids", id],
        output: Output::Checked,
    })?;
    Ok(())
}

/// Step 2's second `ec2 describe-instances`: whether the launch time is 20 minutes ago or more,
/// compared in UTC. A launch time in the future is not.
fn running_long_enough(aws: &dyn Aws, clock: &dyn Clock, id: &str) -> Result<bool, Failure> {
    let text = describe_instance(aws, id, LAUNCH_TIME)?;
    let launched = launch_time::parse(&text)
        .ok_or_else(|| answer::unexpected(DESCRIBE_INSTANCES, LAUNCH_TIME))?;
    let now = launch_time::nanos_since_epoch(clock.utc_now());
    Ok(now - launched >= RUNNING_LONG_ENOUGH.as_nanos() as i128)
}

/// Step 3: checks until the box is ready, or fails at the 15-minute deadline.
///
/// The timing is the architect's ruling of 2026-11-19 for E0.06-T01-S03 (operator.md, Calls by
/// command): the first check is at once, and the 5 seconds are a pause after each check; the
/// deadline is tested after each check that was not ready, so a check that began before it and
/// found the box ready still ends the wait; and the last pause is cut short to end on the
/// deadline, so the last check begins at it.
fn wait(
    aws: &dyn Aws,
    say: &dyn Say,
    clock: &dyn Clock,
    id: &str,
    check: &PatchCheck,
) -> Result<(), Failure> {
    let messages = &check.messages;
    let started = clock.now();
    let mut last: Option<&str> = None;
    loop {
        let message = if !session_manager_online(aws, id)? {
            WAITING_FOR_SESSION_MANAGER
        } else if patch_pending(aws, clock, id, check)? {
            messages.phase.as_str()
        } else {
            return Ok(());
        };
        if last != Some(message) {
            say.progress(message, Some(CallerLine::Waiting));
            last = Some(message);
        }
        let elapsed = clock.now().saturating_sub(started);
        if elapsed >= WAIT_LIMIT {
            let log_command = &messages.log_command;
            return Err(Failure {
                message: format!(
                    "agentspace: {id} was not ready within 15 minutes; check: ssh agentspace {log_command}"
                ),
                caller: CallerFailure::NotReady,
            });
        }
        // The last pause never runs past the deadline, so the last check is made at it.
        clock.sleep(CHECK_INTERVAL.min(WAIT_LIMIT - elapsed));
    }
}

/// `ssm describe-instance-information --filters Key=InstanceIds,Values=<id>` (JSON): online only
/// when `InstanceInformationList[0].PingStatus` is `Online`, and an empty list is not online.
fn session_manager_online(aws: &dyn Aws, id: &str) -> Result<bool, Failure> {
    let filter = format!("Key=InstanceIds,Values={id}");
    let answer = aws.call(&Call {
        service: "ssm",
        operation: "describe-instance-information",
        args: &["--filters", &filter],
        output: Output::Json,
    })?;
    // An answer with no list has no `InstanceInformationList[0].PingStatus`, the member's path as
    // Calls by command writes it; only a list that is there and empty is "not online".
    let list = answer
        .pointer(INSTANCE_INFORMATION_LIST)
        .and_then(Value::as_array)
        .ok_or_else(|| answer::unexpected(DESCRIBE_INSTANCE_INFORMATION, PING_STATUS))?;
    if list.is_empty() {
        return Ok(false);
    }
    Ok(answer::string(&answer, DESCRIBE_INSTANCE_INFORMATION, PING_STATUS)? == "Online")
}

/// Asks the box whether its patch run has finished (operator.md, What up waits for), and whether
/// the answer is `pending`. Every other answer counts as ready: `patched`, `unknown`, `failed` (a
/// patch run that ended this boot without the marker; P24), and an empty or unexpected answer from
/// a failed check (Q6).
///
/// The invocation is polled at once and then each second, at most 20 times. A poll that fails, or
/// whose status is `Pending`, `InProgress` or `Delayed`, polls again; any other status ends the
/// poll, and the answer is `StandardOutputContent`, trimmed. 20 polls without an end count as
/// `pending`.
///
/// The poll never gives the "Unexpected answer" failure (operator.md, Calls by command, the
/// architect's rulings of 2026-11-19 for E0.06-T01-S03): an answer with no string `Status` is not
/// readable yet, and is polled again; one that has ended with no `StandardOutputContent` is the
/// empty answer, which counts as ready (Q6).
///
/// The question is the platform's, as [`PatchCheck::of`] read it: on Linux,
/// `commands=<question>` through `AWS-RunShellScript`, as before v0.12.
fn patch_pending(
    aws: &dyn Aws,
    clock: &dyn Clock,
    id: &str,
    check: &PatchCheck,
) -> Result<bool, Failure> {
    let sent = aws.call(&Call {
        service: "ssm",
        operation: "send-command",
        args: &[
            "--instance-ids",
            id,
            "--document-name",
            check.document,
            "--comment",
            PATCH_COMMENT,
            "--parameters",
            &check.parameters,
        ],
        output: Output::Json,
    })?;
    let command_id = answer::string(&sent, SEND_COMMAND, COMMAND_ID)?;

    for poll in 0..POLL_LIMIT {
        // The first poll is at once, and each pause of a second is between two polls: 20 polls
        // take 19 pauses, and none follows the last.
        if poll > 0 {
            clock.sleep(POLL_INTERVAL);
        }
        // A failed poll is polled again: the invocation is often not readable for a moment after
        // send-command (operator.md, Calls by command).
        let Ok(invocation) = aws.call(&Call {
            service: "ssm",
            operation: "get-command-invocation",
            args: &["--command-id", command_id, "--instance-id", id],
            output: Output::Json,
        }) else {
            continue;
        };
        let status = invocation.get("Status").and_then(Value::as_str);
        if matches!(status, None | Some("Pending" | "InProgress" | "Delayed")) {
            continue;
        }
        let stdout = invocation
            .get("StandardOutputContent")
            .and_then(Value::as_str)
            .unwrap_or("");
        return Ok(stdout.trim() == "pending");
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_operator_api::{AwsError, Mode};
    use serde_json::json;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::time::SystemTime;

    const ID: &str = "i-0123456789abcdef0";

    #[test]
    fn the_patch_question_holds_nothing_forbidden() {
        for c in PATCH_QUESTION.chars() {
            assert!(!matches!(c, ',' | '[' | ']' | '=' | '%' | '"'), "{c:?}");
            assert!(!c.is_control(), "{c:?}");
        }
        assert!(!PATCH_QUESTION.ends_with('\\'));
        assert!(PATCH_QUESTION.is_ascii());
    }

    /// A throwaway folder under the temp folder, removed when it drops.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let unique = format!(
                "agentspace-box-unit-{name}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            );
            let path = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&path).unwrap();
            Scratch(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The marker's path in the question, which the test of its text replaces with a file of its
    /// own throwaway folder, so that no real host's marker is ever read.
    const MARKER: &str = "/run/agentspace/patched";

    /// A stand-in `systemctl`: it records each call's arguments in `calls`, and answers from the
    /// files of the working folder: `is-failed` succeeds when `unit-failed` exists, `is-enabled`
    /// when `unit-enabled` exists, and `list-jobs` prints the line in `unit-job` when it exists.
    const STAND_IN_SYSTEMCTL: &str = "#!/bin/sh\n\
echo \"$*\" >> calls\n\
case \"$1\" in\n\
is-failed) test -e unit-failed ;;\n\
is-enabled) test -e unit-enabled ;;\n\
list-jobs) if test -e unit-job; then cat unit-job; fi ;;\n\
*) exit 3 ;;\n\
esac\n";

    /// Whether `bash` is one this test may run: under the Git for Windows install `root`, and on no
    /// path that holds `System32`, `Windows` or `WindowsApps`, where `bash.exe` is WSL's, whose
    /// start leaves a virtual machine running after the test (architect, 2026-11-22, for
    /// v0.08-T03).
    #[cfg_attr(not(windows), allow(dead_code))]
    fn acceptable_bash(root: &std::path::Path, bash: &std::path::Path) -> bool {
        let barred = |p: &std::path::Path| {
            p.components().any(|c| {
                let name = c.as_os_str().to_string_lossy().to_ascii_lowercase();
                matches!(name.as_str(), "system32" | "windows" | "windowsapps")
            })
        };
        bash.starts_with(root)
            && !barred(root)
            && !barred(bash)
            && bash
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("bash.exe"))
    }

    /// Git for Windows' `bash.exe`, found from the `git` on `PATH`: `git --exec-path` is inside the
    /// install, so the first of its ancestors that holds `bin\bash.exe` and `usr\bin\bash.exe` is
    /// the install's root, and `bin\bash.exe` is the one used. `None` when there is no `git`, no
    /// such root, or a bash that [`acceptable_bash`] refuses. It never looks for `bash` on `PATH`,
    /// which from PowerShell is WSL's.
    #[cfg(windows)]
    fn find_bash() -> Option<std::path::PathBuf> {
        let output = std::process::Command::new("git")
            .arg("--exec-path")
            .current_dir(std::env::temp_dir())
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let exec = std::path::PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
        let exec = exec.canonicalize().ok()?;
        exec.ancestors().skip(1).find_map(|root| {
            let bash = root.join("bin").join("bash.exe");
            if !bash.is_file() || !root.join("usr").join("bin").join("bash.exe").is_file() {
                return None;
            }
            let bash = bash.canonicalize().ok()?;
            acceptable_bash(root, &bash).then_some(bash)
        })
    }

    /// Off Windows, the `bash` of `PATH` is the system's own.
    #[cfg(not(windows))]
    fn find_bash() -> Option<std::path::PathBuf> {
        Some(std::path::PathBuf::from("bash"))
    }

    #[test]
    fn only_a_bash_under_the_git_root_and_off_the_windows_folders_is_acceptable() {
        use std::path::Path;
        let root = Path::new("C:/Program Files/Git");
        assert!(acceptable_bash(
            root,
            Path::new("C:/Program Files/Git/bin/bash.exe")
        ));
        assert!(acceptable_bash(
            root,
            Path::new("C:/Program Files/Git/usr/bin/bash.exe")
        ));
        for bash in [
            "C:/Windows/System32/bash.exe",
            "C:/Users/someone/AppData/Local/Microsoft/WindowsApps/bash.exe",
            "C:/Program Files/Other/bin/bash.exe",
            "C:/Program Files/Git/bin/wsl.exe",
        ] {
            assert!(!acceptable_bash(root, Path::new(bash)), "{bash}");
        }
        // A root that is itself under a barred folder is refused too.
        assert!(!acceptable_bash(
            Path::new("C:/Windows/System32"),
            Path::new("C:/Windows/System32/bash.exe")
        ));
    }

    /// Runs `<bash> <script>` in `dir`, with no argument but the script's name, so that a `bash`
    /// that hands its arguments to another shell reads them the same; its stdout, or `None` when
    /// it could not be run or failed.
    fn bash(bash: &std::path::Path, dir: &std::path::Path, script: &str) -> Option<String> {
        let output = std::process::Command::new(bash)
            .arg(script)
            .current_dir(dir)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8(output.stdout).ok()
    }

    /// The conformance table's test of the command's text (E0.08-T03-S05, #0181): run under bash
    /// with a stand-in `systemctl` first on `PATH`, it answers `failed` for a unit in systemd's
    /// `failed` state without a `start` or `restart` job queued (a `stop` or `reload` job does not
    /// count), `pending` for one with either queued, `pending` for an enabled unit otherwise, and
    /// `unknown` for one not enabled; `patched` when the marker exists, whatever the unit's state.
    ///
    /// **It runs under Git for Windows' `bash.exe` alone** ([`find_bash`]), never WSL's, whose
    /// start leaves a virtual machine running after the test. It is skipped, saying so, when there
    /// is none, or when it cannot run a script.
    #[test]
    fn the_patch_question_gives_each_answer_under_bash() {
        let Some(shell) = find_bash() else {
            eprintln!(
                "Skipped: no Git for Windows bash was found, so the patch question's text was not run."
            );
            return;
        };
        let dir = Scratch::new("patch-question");
        std::fs::write(dir.0.join("probe.sh"), "echo bash-runs\n").unwrap();
        if bash(&shell, &dir.0, "probe.sh").as_deref().map(str::trim) != Some("bash-runs") {
            eprintln!(
                "Skipped: the Git for Windows bash could not run a script, so the patch question's text was not run."
            );
            return;
        }
        assert_eq!(PATCH_QUESTION.matches(MARKER).count(), 1);
        let question = PATCH_QUESTION.replace(MARKER, "marker");
        let stub = dir.0.join("stub");
        std::fs::create_dir_all(&stub).unwrap();
        let systemctl = stub.join("systemctl");
        std::fs::write(&systemctl, STAND_IN_SYSTEMCTL).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&systemctl, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(
            dir.0.join("question.sh"),
            format!("PATH=\"$PWD/stub:$PATH\"\n{question}\n"),
        )
        .unwrap();

        const START_JOB: &str = "  123 agentspace-patch.service start waiting\n";
        const STOP_JOB: &str = "  124 agentspace-patch.service stop waiting\n";
        const RESTART_JOB: &str = "  125 agentspace-patch.service restart waiting\n";
        const RELOAD_JOB: &str = "  126 agentspace-patch.service reload waiting\n";
        // (marker, failed, enabled, job line, answer, systemctl calls in order)
        type Case = (
            bool,
            bool,
            bool,
            Option<&'static str>,
            &'static str,
            &'static [&'static str],
        );
        let cases: [Case; 13] = [
            (true, true, true, Some(START_JOB), "patched", &[]),
            (
                false,
                true,
                true,
                None,
                "failed",
                &["is-failed", "list-jobs"],
            ),
            (
                false,
                true,
                false,
                None,
                "failed",
                &["is-failed", "list-jobs"],
            ),
            // A job that is not a start job leaves a failed unit failed.
            (
                false,
                true,
                true,
                Some(STOP_JOB),
                "failed",
                &["is-failed", "list-jobs"],
            ),
            // A queued `reload` job does not run a failed unit either.
            (
                false,
                true,
                true,
                Some(RELOAD_JOB),
                "failed",
                &["is-failed", "list-jobs"],
            ),
            // A queued `stop` job, with the unit not enabled.
            (
                false,
                true,
                false,
                Some(STOP_JOB),
                "failed",
                &["is-failed", "list-jobs"],
            ),
            // A queued `restart` job counts as a start.
            (
                false,
                true,
                true,
                Some(RESTART_JOB),
                "pending",
                &["is-failed", "list-jobs"],
            ),
            (
                false,
                true,
                false,
                Some(RESTART_JOB),
                "pending",
                &["is-failed", "list-jobs"],
            ),
            (
                false,
                true,
                true,
                Some(START_JOB),
                "pending",
                &["is-failed", "list-jobs"],
            ),
            (
                false,
                true,
                false,
                Some(START_JOB),
                "pending",
                &["is-failed", "list-jobs"],
            ),
            (
                false,
                false,
                true,
                None,
                "pending",
                &["is-failed", "is-enabled"],
            ),
            (
                false,
                false,
                true,
                Some(START_JOB),
                "pending",
                &["is-failed", "is-enabled"],
            ),
            (
                false,
                false,
                false,
                None,
                "unknown",
                &["is-failed", "is-enabled"],
            ),
        ];
        for (n, (marker, failed, enabled, job, answer, calls)) in cases.into_iter().enumerate() {
            for (name, present) in [
                ("marker", marker),
                ("unit-failed", failed),
                ("unit-enabled", enabled),
            ] {
                let path = dir.0.join(name);
                if present {
                    std::fs::write(&path, "").unwrap();
                } else {
                    let _ = std::fs::remove_file(&path);
                }
            }
            let job_path = dir.0.join("unit-job");
            match job {
                Some(line) => std::fs::write(&job_path, line).unwrap(),
                None => {
                    let _ = std::fs::remove_file(&job_path);
                }
            }
            let _ = std::fs::remove_file(dir.0.join("calls"));

            let out =
                bash(&shell, &dir.0, "question.sh").unwrap_or_else(|| panic!("Case {n} failed"));
            assert_eq!(out, format!("{answer}\n"), "Case {n}");
            let recorded = std::fs::read_to_string(dir.0.join("calls")).unwrap_or_default();
            let recorded: Vec<&str> = recorded.lines().collect();
            assert_eq!(recorded.len(), calls.len(), "Case {n}");
            for (line, verb) in recorded.iter().zip(calls) {
                let words: Vec<&str> = line.split_whitespace().collect();
                assert_eq!(words.first(), Some(verb), "Case {n}");
                assert_eq!(words.last(), Some(&"agentspace-patch.service"), "Case {n}");
            }
        }
    }

    /// `up` reads `failed` as it reads `unknown`: its wait ends with `Ready: <id>` and no message
    /// of its own (P21; pin P24), on a host launched less than 20 minutes ago as on one it starts.
    #[test]
    fn a_failed_answer_ends_ups_wait_with_ready_and_no_message() {
        let aws = Script::new(vec![
            state("running"),
            launched("2026-11-19T07:52:35Z"),
            ping(Some("Online")),
            sent(),
            invocation("Success", "failed\n"),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(*f.lines.borrow(), vec![format!("Ready: {ID}")]);
        assert!(aws.answers.borrow().is_empty());

        let aws = Script::new(vec![
            state("stopped"),
            ("start-instances", Ok(json!({}))),
            ping(Some("Online")),
            sent(),
            invocation("Success", "pending"),
            ping(Some("Online")),
            sent(),
            invocation("Success", "failed"),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(
            *f.lines.borrow(),
            vec![
                format!("Starting {ID}..."),
                PATCHING.to_string(),
                format!("Ready: {ID}"),
            ]
        );
        assert!(aws.answers.borrow().is_empty());
    }

    /// A fake AWS that answers each call, in order, from a script keyed by its operation.
    struct Script {
        answers: RefCell<VecDeque<(&'static str, Result<Value, AwsError>)>>,
        calls: RefCell<Vec<String>>,
    }

    impl Script {
        fn new(answers: Vec<(&'static str, Result<Value, AwsError>)>) -> Self {
            Script {
                answers: RefCell::new(answers.into()),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Aws for Script {
        fn call(&self, call: &Call) -> Result<Value, AwsError> {
            let mut line = vec![call.service, call.operation];
            line.extend(call.args);
            self.calls.borrow_mut().push(line.join(" "));
            let (operation, answer) = self
                .answers
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| panic!("Unscripted call {}", line.join(" ")));
            assert_eq!(operation, call.operation);
            answer
        }
    }

    struct Fake {
        now: Cell<Duration>,
        utc: SystemTime,
        lines: RefCell<Vec<String>>,
    }

    impl Clock for Fake {
        fn now(&self) -> Duration {
            self.now.get()
        }
        fn utc_now(&self) -> SystemTime {
            self.utc + self.now.get()
        }
        fn sleep(&self, d: Duration) {
            self.now.set(self.now.get() + d);
        }
    }

    impl Say for Fake {
        fn mode(&self) -> Mode {
            Mode::Person
        }
        fn progress(&self, person: &str, _: Option<CallerLine>) {
            self.lines.borrow_mut().push(person.to_string());
        }
        fn warning(&self, person: &str) {
            self.lines.borrow_mut().push(person.to_string());
        }
        fn detail(&self, _: &str) {}
    }

    fn fake() -> Fake {
        Fake {
            now: Cell::new(Duration::ZERO),
            utc: SystemTime::UNIX_EPOCH + Duration::from_secs(1_795_075_954),
            lines: RefCell::new(Vec::new()),
        }
    }

    fn state(name: &str) -> (&'static str, Result<Value, AwsError>) {
        (
            "describe-instances",
            Ok(json!({"Reservations":[{"Instances":[{"State":{"Name":name}}]}]})),
        )
    }

    fn launched(time: &str) -> (&'static str, Result<Value, AwsError>) {
        (
            "describe-instances",
            Ok(json!({"Reservations":[{"Instances":[{"LaunchTime":time}]}]})),
        )
    }

    fn ping(status: Option<&str>) -> (&'static str, Result<Value, AwsError>) {
        let list = match status {
            Some(s) => json!([{"PingStatus": s}]),
            None => json!([]),
        };
        (
            "describe-instance-information",
            Ok(json!({ "InstanceInformationList": list })),
        )
    }

    fn sent() -> (&'static str, Result<Value, AwsError>) {
        (
            "send-command",
            Ok(json!({"Command":{"CommandId":"c-0123"}})),
        )
    }

    fn invocation(status: &str, out: &str) -> (&'static str, Result<Value, AwsError>) {
        (
            "get-command-invocation",
            Ok(json!({"Status":status,"StandardOutputContent":out})),
        )
    }

    #[test]
    fn a_box_running_long_enough_is_not_asked() {
        let aws = Script::new(vec![state("running"), launched("2026-11-19T07:52:34Z")]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(*f.lines.borrow(), vec![format!("Ready: {ID}")]);
        assert_eq!(aws.calls.borrow().len(), 2);
    }

    #[test]
    fn a_stopped_box_is_started_and_waited_for() {
        let aws = Script::new(vec![
            state("stopped"),
            ("start-instances", Ok(json!({}))),
            ping(None),
            ping(Some("ConnectionLost")),
            ping(Some("Online")),
            sent(),
            ("get-command-invocation", Err(AwsError::Failed("x".into()))),
            invocation("InProgress", ""),
            invocation("Success", "pending\n"),
            ping(Some("Online")),
            sent(),
            invocation("Success", " patched \n"),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(
            *f.lines.borrow(),
            vec![
                format!("Starting {ID}..."),
                WAITING_FOR_SESSION_MANAGER.to_string(),
                PATCHING.to_string(),
                format!("Ready: {ID}"),
            ]
        );
        assert!(aws.answers.borrow().is_empty());
        // 3 pauses of 5 seconds between checks, and 2 of 1 between the first question's 3 polls.
        assert_eq!(f.now.get(), Duration::from_secs(17));
    }

    #[test]
    fn the_deadline_fails_the_wait() {
        let mut answers = vec![state("pending")];
        for _ in 0..181 {
            answers.push(ping(None));
        }
        let aws = Script::new(answers);
        let f = fake();
        let failure = start_and_wait_for(&aws, &f, &f, ID).unwrap_err();
        assert_eq!(failure.caller, CallerFailure::NotReady);
        assert_eq!(
            failure.message,
            format!(
                "agentspace: {ID} was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch"
            )
        );
        assert_eq!(f.now.get(), WAIT_LIMIT);
        assert_eq!(*f.lines.borrow(), vec![WAITING_FOR_SESSION_MANAGER]);
        assert!(aws.answers.borrow().is_empty());
    }

    #[test]
    fn a_stopping_box_waits_unchecked_then_starts() {
        let aws = Script::new(vec![
            state("stopping"),
            (
                "wait",
                Err(AwsError::Failed("Waiter InstanceStopped failed".into())),
            ),
            ("start-instances", Ok(Value::Null)),
            ping(Some("Online")),
            sent(),
            invocation("Success", "unknown"),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(
            *f.lines.borrow(),
            vec![
                format!("Waiting for {ID} to finish stopping..."),
                format!("Starting {ID}..."),
                format!("Ready: {ID}"),
            ]
        );
        assert_eq!(
            aws.calls.borrow()[1],
            format!("ec2 wait instance-stopped --instance-ids {ID}")
        );
    }

    #[test]
    fn a_failed_start_fails() {
        let aws = Script::new(vec![
            state("stopped"),
            ("start-instances", Err(AwsError::SignIn)),
        ]);
        let f = fake();
        let failure = start_and_wait_for(&aws, &f, &f, ID).unwrap_err();
        assert_eq!(failure.caller, CallerFailure::Same);
        assert_eq!(*f.lines.borrow(), vec![format!("Starting {ID}...")]);
    }

    #[test]
    fn any_other_state_fails() {
        for name in ["terminated", "shutting-down", ""] {
            let aws = Script::new(vec![state(name)]);
            let f = fake();
            let failure = start_and_wait_for(&aws, &f, &f, ID).unwrap_err();
            assert_eq!(
                failure.message,
                format!("agentspace: Instance {ID} is {name}.")
            );
            assert_eq!(failure.caller, CallerFailure::NotAvailable);
            assert!(f.lines.borrow().is_empty());
        }
    }

    #[test]
    fn only_a_running_box_has_its_launch_time_read() {
        // A pending box waits with no second describe-instances (FB-09).
        let aws = Script::new(vec![
            state("pending"),
            ping(Some("Online")),
            sent(),
            invocation("Success", "patched"),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        let describes = aws
            .calls
            .borrow()
            .iter()
            .filter(|c| c.starts_with("ec2 describe-instances"))
            .count();
        assert_eq!(describes, 1);
    }

    #[test]
    fn the_twenty_minute_rule() {
        // Exactly 20 minutes ago is long enough.
        let aws = Script::new(vec![state("running"), launched("2026-11-19T07:52:34Z")]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        // A second less waits, and so does a launch time in the future.
        for time in ["2026-11-19T07:52:35Z", "2026-11-19T09:00:00+00:00"] {
            let aws = Script::new(vec![
                state("running"),
                launched(time),
                ping(Some("Online")),
                sent(),
                invocation("Success", "patched"),
            ]);
            let f = fake();
            start_and_wait_for(&aws, &f, &f, ID).unwrap();
            assert!(aws.answers.borrow().is_empty());
        }
    }

    #[test]
    fn a_bad_launch_time_is_an_unexpected_answer() {
        for answer in [
            json!({"Reservations":[{"Instances":[{}]}]}),
            json!({"Reservations":[{"Instances":[{"LaunchTime":"yesterday"}]}]}),
            json!({"Reservations":[{"Instances":[{"LaunchTime":12}]}]}),
        ] {
            let aws = Script::new(vec![state("running"), ("describe-instances", Ok(answer))]);
            let f = fake();
            let failure = start_and_wait_for(&aws, &f, &f, ID).unwrap_err();
            assert_eq!(
                failure.message,
                "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].LaunchTime."
            );
            assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        }
    }

    #[test]
    fn twenty_polls_without_an_end_count_as_pending() {
        let mut answers = vec![state("pending"), ping(Some("Online")), sent()];
        for i in 0..POLL_LIMIT {
            answers.push(match i % 3 {
                0 => (
                    "get-command-invocation",
                    Err(AwsError::Failed("InvocationDoesNotExist".into())),
                ),
                1 => invocation("Pending", ""),
                _ => invocation("Delayed", ""),
            });
        }
        answers.extend([ping(Some("Online")), sent(), invocation("Failed", "")]);
        let aws = Script::new(answers);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(
            *f.lines.borrow(),
            vec![PATCHING.to_string(), format!("Ready: {ID}")]
        );
        // 19 pauses between 20 polls, then 5 seconds before the next check, whose poll is at once.
        assert_eq!(f.now.get(), Duration::from_secs(19 + 5));
    }

    #[test]
    fn an_invocation_with_no_status_polls_again_and_no_stdout_is_ready() {
        let aws = Script::new(vec![
            state("pending"),
            ping(Some("Online")),
            sent(),
            ("get-command-invocation", Ok(json!({}))),
            ("get-command-invocation", Ok(json!({"Status":"Success"}))),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(*f.lines.borrow(), vec![format!("Ready: {ID}")]);
        assert!(aws.answers.borrow().is_empty());
    }

    #[test]
    fn a_missing_member_is_an_unexpected_answer() {
        type Answers = Vec<(&'static str, Result<Value, AwsError>)>;
        let cases: Vec<(Answers, &str)> = vec![
            // The path is the one Calls by command's table writes for this call, as FB-08's rule
            // gives it ("its path as this table writes it"), with or without the list.
            (
                vec![("describe-instance-information", Ok(json!({})))],
                "ssm describe-instance-information: no InstanceInformationList[0].PingStatus.",
            ),
            (
                vec![(
                    "describe-instance-information",
                    Ok(json!({"InstanceInformationList":{}})),
                )],
                "ssm describe-instance-information: no InstanceInformationList[0].PingStatus.",
            ),
            (
                vec![(
                    "describe-instance-information",
                    Ok(json!({"InstanceInformationList":[{}]})),
                )],
                "ssm describe-instance-information: no InstanceInformationList[0].PingStatus.",
            ),
            (
                vec![
                    ping(Some("Online")),
                    ("send-command", Ok(json!({"Command":{}}))),
                ],
                "ssm send-command: no Command.CommandId.",
            ),
        ];
        for (mut answers, tail) in cases {
            answers.insert(0, state("pending"));
            let aws = Script::new(answers);
            let f = fake();
            let failure = start_and_wait_for(&aws, &f, &f, ID).unwrap_err();
            assert_eq!(
                failure.message,
                format!("agentspace: Unexpected answer from {tail}")
            );
            assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        }
    }

    #[test]
    fn the_calls_carry_their_arguments_in_order() {
        let aws = Script::new(vec![
            state("pending"),
            ping(Some("Online")),
            sent(),
            invocation("Success", "patched"),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(
            *aws.calls.borrow(),
            vec![
                format!("ec2 describe-instances --instance-ids {ID}"),
                format!("ssm describe-instance-information --filters Key=InstanceIds,Values={ID}"),
                format!(
                    "ssm send-command --instance-ids {ID} --document-name AWS-RunShellScript --comment agentspace up: patch check --parameters commands={PATCH_QUESTION}"
                ),
                format!("ssm get-command-invocation --command-id c-0123 --instance-id {ID}"),
            ]
        );
    }

    // The platform seam (E0.12-T03-S03, part 1): the wait on a host's platform.

    use crate::run_command::fake::{self, TestPlatform};

    fn patched_once() -> Script {
        Script::new(vec![
            state("pending"),
            ping(Some("Online")),
            sent(),
            invocation("Success", "pending"),
            ping(Some("Online")),
            sent(),
            invocation("Success", "patched"),
        ])
    }

    /// Both Linux platforms make today's calls and print today's lines, byte for byte, with
    /// `commands=<question>` through `AWS-RunShellScript`.
    #[test]
    fn both_linux_platforms_wait_with_todays_calls_and_lines() {
        let today = patched_once();
        let f = fake();
        start_and_wait_for(&today, &f, &f, ID).unwrap();
        let today_lines = f.lines.borrow().clone();
        assert_eq!(
            today_lines,
            vec![PATCHING.to_string(), format!("Ready: {ID}")]
        );
        for platform in [
            &agentspace_platforms::LINUX_ARM64,
            &agentspace_platforms::LINUX_AMD64,
        ] {
            let aws = patched_once();
            let f = fake();
            start_and_wait_for_on(&aws, &f, &f, ID, platform).unwrap();
            assert_eq!(*aws.calls.borrow(), *today.calls.borrow());
            assert_eq!(*f.lines.borrow(), today_lines);
        }
        assert!(today.calls.borrow().contains(&format!(
            "ssm send-command --instance-ids {ID} --document-name AWS-RunShellScript --comment agentspace up: patch check --parameters commands={PATCH_QUESTION}"
        )));
    }

    /// A platform whose document is `AWS-RunPowerShellScript` gets its own question as the JSON
    /// value, never the shorthand, and its own phase message.
    #[test]
    fn a_powershell_platform_gets_its_question_as_json_and_its_own_phase() {
        let aws = patched_once();
        let f = fake();
        start_and_wait_for_on(&aws, &f, &f, ID, &TestPlatform::powershell()).unwrap();
        let json = serde_json::json!({ "commands": [fake::QUESTION] }).to_string();
        let send = format!(
            "ssm send-command --instance-ids {ID} --document-name AWS-RunPowerShellScript --comment agentspace up: patch check --parameters {json}"
        );
        let calls = aws.calls.borrow();
        assert_eq!(calls.iter().filter(|c| **c == send).count(), 2, "{calls:?}");
        assert!(!calls.iter().any(|c| c.contains("commands=")));
        assert_eq!(
            *f.lines.borrow(),
            vec![fake::PHASE.to_string(), format!("Ready: {ID}")]
        );
    }

    /// The deadline names the platform's log command.
    #[test]
    fn the_deadline_names_the_platforms_log_command() {
        let mut answers = vec![state("pending")];
        for _ in 0..181 {
            answers.push(ping(None));
        }
        let aws = Script::new(answers);
        let f = fake();
        let failure =
            start_and_wait_for_on(&aws, &f, &f, ID, &TestPlatform::powershell()).unwrap_err();
        assert_eq!(failure.caller, CallerFailure::NotReady);
        assert_eq!(
            failure.message,
            format!(
                "agentspace: {ID} was not ready within 15 minutes; check: ssh agentspace {}",
                fake::LOG
            )
        );
    }

    /// A platform whose patch check fails ends the step with that failure before any call: the
    /// instance is neither read nor started, and nothing is printed.
    #[test]
    fn a_failed_patch_check_command_ends_the_step_before_any_call() {
        let aws = Script::new(vec![state("stopped")]);
        let f = fake();
        let platform = TestPlatform {
            document: "AWS-RunPowerShellScript",
            fails: true,
        };
        let failure = start_and_wait_for_on(&aws, &f, &f, ID, &platform).unwrap_err();
        assert_eq!(failure.message, fake::FAILED);
        assert!(aws.calls.borrow().is_empty(), "{:?}", aws.calls.borrow());
        assert_eq!(aws.answers.borrow().len(), 1);
        assert!(f.lines.borrow().is_empty());
    }
}
