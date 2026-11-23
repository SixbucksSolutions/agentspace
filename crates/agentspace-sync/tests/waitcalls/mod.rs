//! What the tests of `sync`'s patch wait share (E0.08-T03-S05, #0181; operator.md, The patch
//! wait; What up waits for; Calls by command; AWS failures).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Written from docs/operator.md alone.
//!
//! - **A script of checks** (`Rig::script_checks`): the wait's checks, one after another, each as
//!   the three stand-in answers it needs (`ssm describe-instance-information`, `ssm send-command`,
//!   `ssm get-command-invocation`), so a test says "pending, then patched" and the stand-in `aws`
//!   answers each call in turn. A host's script is keyed by its instance ID, so `sync -All` can
//!   script each host on its own.
//! - **A fake clock** (`FakeClock`) whose `sleep` returns at once and moves its time on, so the
//!   wait's 5-second pauses and its 30 minutes cost nothing. It reaches the wait through the one
//!   function that uses the seam, `with_clock`.
//! - **A timeline** (`wait_timeline`) of the ssh and scp calls, the `ssm` calls and the messages in
//!   one order, which the stand-ins' log and the `Say` events share.
//!
//! **The seam** (the one place it is used is `with_clock`): `Seams::real_except_go_and_clock(find_go,
//! localswim_checkout, clock, run)`, which is `Seams::real_except_go` with the clock inserted before
//! `run`.
#![cfg(windows)]
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::time::{Duration, SystemTime};

use agentspace_box::Clock;
use agentspace_operator_api::Failure;
use agentspace_sync::{Seams, Target};

use crate::common::{Item, Log, Rec, RecSay, Rig, ca_env, json_invocation, json_online, json_sent};

/// The text the AWS CLI prints for an expired or missing sign-in (#0034; operator.md, AWS failures).
pub const SIGN_IN_TEXT: &str =
    "Unable to locate credentials. You can configure credentials by running \"aws login\".";
/// The refusal a host that is rebooting gives `ssm send-command`.
pub const SEND_REFUSED: &str = "An error occurred (InvalidInstanceId) when calling the SendCommand operation: Instances not in a valid state for account";
/// The refusal of `ssm describe-instance-information` that is not a sign-in failure.
pub const DESCRIBE_REFUSED: &str = "An error occurred (AccessDeniedException) when calling the DescribeInstanceInformation operation: User is not authorized";

/// The wait's comment (operator.md, The patch wait, step 2).
pub const COMMENT: &str = "agentspace sync: patch check";
/// The command ID the stand-in answers `ssm send-command` with.
pub const COMMAND_ID: &str = "00000000-0000-0000-0000-000000000000";

// ------------------------------------------------------------------ The fake clock

/// A clock whose `sleep` takes no time: it moves its own time on and remembers the pause.
pub struct FakeClock {
    elapsed: Cell<Duration>,
    sleeps: RefCell<Vec<Duration>>,
    wall: SystemTime,
}

impl FakeClock {
    pub fn new() -> FakeClock {
        FakeClock {
            elapsed: Cell::new(Duration::ZERO),
            sleeps: RefCell::new(Vec::new()),
            wall: SystemTime::now(),
        }
    }

    /// Every pause, in order.
    pub fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.borrow().clone()
    }

    /// The pauses of the wait's checks: the ones of 2 seconds or more, so a last pause cut short to
    /// land on the deadline is counted too (the invocation's polls are a second apart).
    pub fn check_pauses(&self) -> Vec<Duration> {
        self.sleeps()
            .into_iter()
            .filter(|d| *d >= Duration::from_secs(2))
            .collect()
    }

    /// The time that has passed on this clock.
    pub fn elapsed(&self) -> Duration {
        self.elapsed.get()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.elapsed.get()
    }
    fn utc_now(&self) -> SystemTime {
        self.wall + self.elapsed.get()
    }
    fn sleep(&self, duration: Duration) {
        self.sleeps.borrow_mut().push(duration);
        self.elapsed.set(self.elapsed.get() + duration);
    }
}

// ------------------------------------------------------------------ Running with the fake clock

impl Rig {
    /// Runs `run` over the seams the operator uses, but for `go`, the localswim folder and `clock`.
    /// **The one use of the seam `Seams::real_except_go_and_clock`.**
    fn with_clock<T>(&self, clock: &FakeClock, run: impl FnOnce(&Seams) -> T) -> T {
        let find_go = || self.found_go();
        Seams::real_except_go_and_clock(&find_go, self.localswim(), clock, run)
    }

    /// `sync` for `target` with the fake clock.
    pub fn do_sync_clocked(
        &self,
        say: &RecSay,
        target: Target<'_>,
        clock: &FakeClock,
    ) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        self.with_clock(clock, |seams| {
            agentspace_sync::sync_target_using(&ctx, &ca_env(), target, say, &run, &run, seams)
        })
    }

    /// `sync -All` with the fake clock.
    pub fn do_sync_all_clocked(&self, say: &RecSay, clock: &FakeClock) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        self.with_clock(clock, |seams| {
            agentspace_sync::sync_all_using(&ctx, &ca_env(), say, &run, &run, seams)
        })
    }

    /// `provision` for `project` with the fake clock.
    pub fn do_provision_clocked(
        &self,
        say: &RecSay,
        project: &str,
        clock: &FakeClock,
    ) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        self.with_clock(clock, |seams| {
            agentspace_sync::provision_using(&ctx, &ca_env(), project, say, &run, &run, seams)
        })
    }
}

// ------------------------------------------------------------------ The script of checks

/// One check of the wait, as the stand-in `aws` answers it.
#[derive(Clone, Debug)]
pub enum Check {
    /// Online, a command sent, and an invocation that ended in `Success` with this output (and a
    /// line feed, which the wait trims).
    Answer(&'static str),
    /// Online, a command sent, and an invocation that ended in `Success` with no output member.
    NoOutput,
    /// Online, a command sent, and an invocation that ended in this status with this output.
    Invocation(&'static str, &'static str),
    /// Online, a command sent, and an invocation that is `InProgress` at each of the 20 polls.
    NoEnd,
    /// The agent is registered and not `Online`.
    Offline,
    /// The agent's list is empty.
    EmptyList,
    /// Online, and `ssm send-command` is refused as a rebooting host refuses it.
    SendFails,
    /// Online, and `ssm send-command` fails with the AWS sign-in's text (#0034).
    SendSignIn,
    /// Online, and `ssm send-command` exits 0 with this raw output: empty, not JSON, or JSON with
    /// no `Command.CommandId`.
    SendAnswers(&'static str),
    /// Online, a command sent, and each of the 20 polls exits 0 with this raw output: empty, not
    /// JSON, or JSON with no `Status`.
    PollAnswers(&'static str),
    /// `ssm describe-instance-information` fails, and not as a sign-in failure.
    DescribeFails,
    /// `ssm describe-instance-information` fails with the AWS sign-in's text (#0034).
    DescribeSignIn,
}

/// The three keys that tell one host's `ssm` calls from another's, in `aws`'s joined arguments.
fn keys(id: &str) -> (String, String, String) {
    (
        format!("Key=InstanceIds,Values={id}"),
        format!("--instance-ids {id} --document-name"),
        format!("--instance-id {id}"),
    )
}

impl Rig {
    /// Scripts the wait of the host whose instance is `id`: the `n`th check is `checks[n]`, and a
    /// call past the script's last answer is answered as the last was.
    pub fn script_checks(&self, id: &str, checks: &[Check]) {
        let (describe, send, get) = keys(id);
        for check in checks {
            match check {
                Check::Offline => {
                    self.seq(
                        "aws",
                        &describe,
                        0,
                        "{\"InstanceInformationList\":[{\"PingStatus\":\"ConnectionLost\"}]}",
                        "",
                    );
                    continue;
                }
                Check::EmptyList => {
                    self.seq("aws", &describe, 0, "{\"InstanceInformationList\":[]}", "");
                    continue;
                }
                Check::DescribeFails => {
                    self.seq("aws", &describe, 254, "", &format!("{DESCRIBE_REFUSED}\\n"));
                    continue;
                }
                Check::DescribeSignIn => {
                    self.seq("aws", &describe, 255, "", &format!("{SIGN_IN_TEXT}\\n"));
                    continue;
                }
                _ => self.seq("aws", &describe, 0, &json_online(), ""),
            }
            match check {
                Check::SendFails => {
                    self.seq("aws", &send, 254, "", &format!("{SEND_REFUSED}\\n"));
                    continue;
                }
                Check::SendSignIn => {
                    self.seq("aws", &send, 255, "", &format!("{SIGN_IN_TEXT}\\n"));
                    continue;
                }
                Check::SendAnswers(out) => {
                    self.seq("aws", &send, 0, out, "");
                    continue;
                }
                _ => self.seq("aws", &send, 0, &json_sent(), ""),
            }
            match check {
                Check::Answer(out) => {
                    self.seq("aws", &get, 0, &json_invocation("Success", Some(out)), "")
                }
                Check::NoOutput => self.seq("aws", &get, 0, &json_invocation("Success", None), ""),
                Check::Invocation(status, out) => {
                    self.seq("aws", &get, 0, &json_invocation(status, Some(out)), "")
                }
                Check::NoEnd => {
                    for _ in 0..20 {
                        self.seq("aws", &get, 0, &json_invocation("InProgress", None), "");
                    }
                }
                Check::PollAnswers(out) => {
                    for _ in 0..20 {
                        self.seq("aws", &get, 0, out, "");
                    }
                }
                _ => {}
            }
        }
    }
}

// ------------------------------------------------------------------ The timeline

pub const DESCRIBE: &str = "aws ssm describe-instance-information";
pub const SEND: &str = "aws ssm send-command";
pub const GET: &str = "aws ssm get-command-invocation";

pub fn checking(id: &str) -> String {
    format!("say: Checking that {id} has finished this boot's patch run...")
}
pub const WAITING: &str = "say: Waiting for Session Manager...";
pub const PATCHING: &str = "say: Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...";
pub fn patched(id: &str) -> String {
    format!("say: Patched: {id}")
}

/// The warning of a patch run that failed (operator.md, The patch wait, step 4), as `Say::warning`
/// is given it. `alias` is `agentspace` for the box.
pub fn failed_warning_text(id: &str, alias: &str) -> String {
    format!(
        "agentspace: The patch run on {id} failed; sync goes on without it. Check: ssh {alias} journalctl -u agentspace-patch"
    )
}
pub fn failed_warning(id: &str, alias: &str) -> String {
    format!("warn: {}", failed_warning_text(id, alias))
}

/// The deadline's failure message (operator.md, The patch wait, step 2).
pub fn deadline_message(id: &str, alias: &str) -> String {
    format!(
        "agentspace: {id} did not finish its patch run within 30 minutes; check: ssh {alias} journalctl -u agentspace-patch"
    )
}

/// What a script of checks makes the stand-in `aws` see, and `Say` say, in one order: the ssh, scp
/// and go calls, the `ssm` calls, the progress lines of `sync` and of the wait, and the warnings.
pub fn wait_timeline(log: &Log) -> Vec<String> {
    log.items
        .iter()
        .filter_map(|i| match i {
            Item::Call(r) if r.name != "aws" => Some(format!("{}#{}", r.name, r.n)),
            Item::Call(r) if r.args.first().map(String::as_str) == Some("ssm") => Some(format!(
                "aws ssm {}",
                r.args.get(1).map(String::as_str).unwrap_or("")
            )),
            Item::Say(k, t) if k == "progress" => {
                let keep = t.starts_with("Copying ")
                    || t.starts_with("Building ")
                    || t.starts_with("Skipping ")
                    || t.starts_with("Checking that ")
                    || t.starts_with("Waiting for Session Manager")
                    || t.starts_with("Patching Ubuntu")
                    || t.starts_with("Patched: ");
                keep.then(|| format!("say: {t}"))
            }
            Item::Say(k, t) if k == "warning" => Some(format!("warn: {t}")),
            _ => None,
        })
        .collect()
}

/// The wait's expected lines, built one by one.
#[derive(Default)]
pub struct Want(pub Vec<String>);

impl Want {
    pub fn new() -> Want {
        Want(Vec::new())
    }
    pub fn line(mut self, text: impl Into<String>) -> Want {
        self.0.push(text.into());
        self
    }
    /// The calls one check makes.
    pub fn check(mut self, check: &Check) -> Want {
        let calls: Vec<&str> = match check {
            Check::Offline | Check::EmptyList | Check::DescribeFails | Check::DescribeSignIn => {
                vec![DESCRIBE]
            }
            Check::SendFails | Check::SendSignIn | Check::SendAnswers(_) => vec![DESCRIBE, SEND],
            Check::NoEnd | Check::PollAnswers(_) => {
                let mut v = vec![DESCRIBE, SEND];
                v.extend(std::iter::repeat_n(GET, 20));
                v
            }
            _ => vec![DESCRIBE, SEND, GET],
        };
        self.0.extend(calls.into_iter().map(str::to_string));
        self
    }
}

/// `base`, `sync`'s own timeline, with the wait's lines put right after its install, `ssh#2`.
pub fn with_wait(base: Vec<String>, wait: Want) -> Vec<String> {
    let at = base
        .iter()
        .position(|l| l == "ssh#2")
        .expect("The base timeline has the install's ssh")
        + 1;
    let mut v = base[..at].to_vec();
    v.extend(wait.0);
    v.extend(base[at..].iter().cloned());
    v
}

/// `base` cut after the install, `ssh#2`: what a run that fails at the wait has done.
pub fn up_to_install(base: Vec<String>) -> Vec<String> {
    let at = base
        .iter()
        .position(|l| l == "ssh#2")
        .expect("The base timeline has the install's ssh")
        + 1;
    base[..at].to_vec()
}

// ------------------------------------------------------------------ Reading the calls

/// The `ssm` calls of `op` (such as `send-command`), in order.
pub fn ssm_calls<'a>(log: &'a Log, op: &str) -> Vec<&'a Rec> {
    log.calls("aws")
        .into_iter()
        .filter(|r| {
            r.args.first().map(String::as_str) == Some("ssm")
                && r.args.get(1).map(String::as_str) == Some(op)
        })
        .collect()
}

/// How many `ssm` calls the run made, all operations.
pub fn ssm_count(log: &Log) -> usize {
    log.calls("aws")
        .iter()
        .filter(|r| r.args.first().map(String::as_str) == Some("ssm"))
        .count()
}

/// The arguments of a call of `describe-instance-information`, `send-command` and
/// `get-command-invocation` for `id`, but for `send-command`'s question (operator.md, Calls by
/// command; AWS failures: the service, the operation, `--region us-west-2`, `--output json`, then
/// the operation's own arguments).
pub fn describe_args(id: &str) -> Vec<String> {
    crate::common::svec(&[
        "ssm",
        "describe-instance-information",
        "--region",
        "us-west-2",
        "--output",
        "json",
        "--filters",
        &format!("Key=InstanceIds,Values={id}"),
    ])
}

pub fn send_args_before_question(id: &str) -> Vec<String> {
    crate::common::svec(&[
        "ssm",
        "send-command",
        "--region",
        "us-west-2",
        "--output",
        "json",
        "--instance-ids",
        id,
        "--document-name",
        "AWS-RunShellScript",
        "--comment",
        COMMENT,
        "--parameters",
    ])
}

pub fn get_args(id: &str) -> Vec<String> {
    crate::common::svec(&[
        "ssm",
        "get-command-invocation",
        "--region",
        "us-west-2",
        "--output",
        "json",
        "--command-id",
        COMMAND_ID,
        "--instance-id",
        id,
    ])
}

impl Rig {
    /// Checks every `ssm` call of the run for `id`: the arguments, in the order and form Calls by
    /// command gives them, with the comment `agentspace sync: patch check`, and the question the
    /// host is asked.
    pub fn check_ssm_args(&self, log: &Log, id: &str) {
        for r in ssm_calls(log, "describe-instance-information") {
            self.same_list("The describe call's arguments", &r.args, &describe_args(id));
        }
        for r in ssm_calls(log, "send-command") {
            let before = send_args_before_question(id);
            self.check(
                r.args.len() == before.len() + 1 && r.args[..before.len()] == before[..],
                &format!(
                    "The send call MUST be `ssm send-command --region us-west-2 --output json --instance-ids <id> --document-name AWS-RunShellScript --comment \"{COMMENT}\" --parameters commands=<question>`; it was {:?}",
                    &r.args[..r.args.len().min(before.len())]
                ),
            );
            let question = r.args.last().cloned().unwrap_or_default();
            self.check(
                question.starts_with("commands="),
                "The question MUST be sent as `commands=<question>`",
            );
            // The question is `up`'s (What up waits for): it names the marker, the unit and the
            // four words it answers with.
            for word in [
                "/run/agentspace/patched",
                "agentspace-patch.service",
                "patched",
                "failed",
                "pending",
                "unknown",
            ] {
                self.check(
                    question.contains(word),
                    &format!("The question MUST be `up`'s four-answer question: it lacks `{word}`"),
                );
            }
        }
        for r in ssm_calls(log, "get-command-invocation") {
            self.same_list("The poll's arguments", &r.args, &get_args(id));
        }
    }

    /// Checks that no `ec2` call was made after the first ssh or scp: the wait never reads the
    /// launch time or the state (operator.md, Calls by command).
    pub fn check_no_ec2_after_the_start(&self, log: &Log) {
        let first_remote = log
            .items
            .iter()
            .position(|i| matches!(i, Item::Call(r) if r.name != "aws"));
        let Some(first_remote) = first_remote else {
            return;
        };
        let late = log.items[first_remote..]
            .iter()
            .filter(|i| matches!(i, Item::Call(r) if r.name == "aws" && r.args.first().map(String::as_str) == Some("ec2")))
            .count();
        self.check(
            late == 0,
            &format!("The patch wait MUST make no `ec2` call of its own: {late} followed the first ssh or scp"),
        );
    }

    /// The remote calls (ssh, scp and go) of the run, as `ssh#2`.
    pub fn remote_names(&self, log: &Log) -> Vec<String> {
        log.remote_calls()
            .iter()
            .map(|r| format!("{}#{}", r.name, r.n))
            .collect()
    }
}
