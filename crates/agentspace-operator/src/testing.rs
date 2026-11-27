//! Test helpers: a throwaway home under the temp folder, and fakes for the traits the commands
//! work against.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

use agentspace_box::Clock;
use agentspace_ca::CaEnv;
use agentspace_hosts::HostKey;
use agentspace_operator_api::{
    Aws, AwsError, Bootstrap, Call, CallerFailure, CallerLine, Context, Failure, HostPlatform,
    Mode, Output, PatchMessages, Remote, Say, SyncPlan,
};
use serde_json::Value;

use crate::{Reach, Report, Target};

/// A throwaway folder under the temp folder, removed when it drops.
pub(crate) struct TempHome(PathBuf);

impl TempHome {
    pub(crate) fn new(name: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "agentspace-operator-unit-{}-{}-{name}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempHome(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn ctx(&self) -> Context {
        self.ctx_nested(false)
    }

    pub(crate) fn ctx_nested(&self, nested: bool) -> Context {
        Context {
            home: self.0.join("home"),
            local_app_data: None,
            system_root: self.0.join("root"),
            nested,
        }
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A [`Say`] and [`Report`] that record every line, in order. `quiet` drops progress and
/// warnings, as `-Quiet` does, and never a report line.
pub(crate) struct FakeSay {
    pub(crate) quiet: bool,
    pub(crate) lines: RefCell<Vec<String>>,
}

impl FakeSay {
    pub(crate) fn new(quiet: bool) -> Self {
        FakeSay {
            quiet,
            lines: RefCell::default(),
        }
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        self.lines.borrow().clone()
    }
}

impl Say for FakeSay {
    fn mode(&self) -> Mode {
        Mode::Person
    }
    fn progress(&self, person: &str, _caller: Option<CallerLine>) {
        if !self.quiet {
            self.lines.borrow_mut().push(person.to_string());
        }
    }
    fn warning(&self, person: &str) {
        if !self.quiet {
            self.lines.borrow_mut().push(person.to_string());
        }
    }
    fn detail(&self, _text: &str) {}
}

impl Report for FakeSay {
    fn report(&self, line: &str) {
        self.lines.borrow_mut().push(line.to_string());
    }
    fn failure(&self, failure: &Failure) {
        self.lines.borrow_mut().push(failure.message.clone());
    }
}

/// An AWS CLI that answers from a script and records each call as `service operation args...`.
pub(crate) struct FakeAws {
    calls: RefCell<Vec<String>>,
    outputs: RefCell<Vec<Output>>,
    answers: RefCell<VecDeque<Result<Value, AwsError>>>,
}

impl FakeAws {
    pub(crate) fn new(answers: Vec<Result<Value, AwsError>>) -> Self {
        FakeAws {
            calls: RefCell::default(),
            outputs: RefCell::default(),
            answers: RefCell::new(answers.into()),
        }
    }

    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    /// How each call asked for its answer to be handled, in the order of [`FakeAws::calls`].
    pub(crate) fn outputs(&self) -> Vec<Output> {
        self.outputs.borrow().clone()
    }
}

impl Aws for FakeAws {
    fn call(&self, call: &Call) -> Result<Value, AwsError> {
        let mut line = format!("{} {}", call.service, call.operation);
        for arg in call.args {
            line.push(' ');
            line.push_str(arg);
        }
        self.calls.borrow_mut().push(line);
        self.outputs.borrow_mut().push(call.output);
        self.answers
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Err(AwsError::Failed("No answer was scripted.".to_string())))
    }
}

/// A clock whose sleeps are recorded and take no time.
pub(crate) struct FakeClock {
    pub(crate) slept: RefCell<Vec<Duration>>,
}

impl FakeClock {
    pub(crate) fn new() -> Self {
        FakeClock {
            slept: RefCell::default(),
        }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.slept.borrow().iter().sum()
    }
    fn utc_now(&self) -> SystemTime {
        SystemTime::now()
    }
    fn sleep(&self, duration: Duration) {
        self.slept.borrow_mut().push(duration);
    }
}

/// ssh and scp that record their arguments and exit as scripted.
pub(crate) struct FakeRemote {
    pub(crate) calls: RefCell<Vec<(Vec<String>, Option<String>)>>,
    pub(crate) exit: i32,
}

impl FakeRemote {
    pub(crate) fn new(exit: i32) -> Self {
        FakeRemote {
            calls: RefCell::default(),
            exit,
        }
    }
}

impl Remote for FakeRemote {
    fn ssh(&self, options: &[&str], remote_command: Option<&str>) -> Result<i32, Failure> {
        self.calls.borrow_mut().push((
            options.iter().map(|s| s.to_string()).collect(),
            remote_command.map(str::to_string),
        ));
        Ok(self.exit)
    }
    fn scp(&self, _args: &[&str]) -> Result<i32, Failure> {
        Ok(self.exit)
    }
}

/// The program's reach beyond the AWS CLI, ssh and scp, recorded.
#[derive(Default)]
pub(crate) struct FakeReach {
    pub(crate) calls: RefCell<Vec<String>>,
    /// The commands (by their recorded name) that fail, and the message each fails with.
    pub(crate) fail: Vec<(String, String)>,
    /// The seam every platform gets, in place of `agentspace_platforms::for_platform`'s.
    pub(crate) seam: Option<StandinSeam>,
}

/// A stand-in for one platform's seam: its online grow, its remote command form and, when
/// `patch_check` is set, its patch check's question are text a test can find, and every other row
/// fails. It stands in for a Windows row whose text `agentspace-windows` does not give yet.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StandinSeam {
    pub(crate) platform: &'static str,
    /// Whether `grow_script` fails, with [`STANDIN_GROW_FAILS`].
    pub(crate) grow_fails: bool,
    /// Whether `patch_check_command` gives [`STANDIN_PATCH_CHECK`]; otherwise it fails, with
    /// [`STANDIN_UNUSED`].
    pub(crate) patch_check: bool,
}

/// The stand-in's patch check's question, when it gives one.
pub(crate) const STANDIN_PATCH_CHECK: &str = "patch-check-of-the-stand-in";

/// The failure of the stand-in's rows that a test does not use.
pub(crate) const STANDIN_UNUSED: &str = "agentspace: The stand-in seam has no such row.";
/// The failure of the stand-in's `grow_script`, when it fails.
pub(crate) const STANDIN_GROW_FAILS: &str = "agentspace: The stand-in seam's grow failed.";

fn unused() -> Failure {
    Failure {
        message: STANDIN_UNUSED.to_string(),
        caller: CallerFailure::Same,
    }
}

impl HostPlatform for StandinSeam {
    fn platform(&self) -> &'static str {
        self.platform
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
    fn remote_command(&self, script: &str) -> Result<String, Failure> {
        Ok(format!("remote-form[{script}]"))
    }
    fn bootstrap(&self) -> Result<Bootstrap, Failure> {
        Err(unused())
    }
    fn sync_plan(&self) -> Result<SyncPlan, Failure> {
        Err(unused())
    }
    fn patch_check_command(&self) -> Result<String, Failure> {
        if self.patch_check {
            Ok(STANDIN_PATCH_CHECK.to_string())
        } else {
            Err(unused())
        }
    }
    fn patch_messages(&self) -> PatchMessages {
        PatchMessages {
            phase: String::new(),
            log_command: String::new(),
        }
    }
    fn horizon_command(&self, _horizon: u64) -> Result<String, Failure> {
        Err(unused())
    }
    fn grow_script(&self) -> Result<String, Failure> {
        if self.grow_fails {
            Err(Failure {
                message: STANDIN_GROW_FAILS.to_string(),
                caller: CallerFailure::Same,
            })
        } else {
            Ok(format!("grow-script-of-{}", self.platform))
        }
    }
    fn push_zone(&self, _windows_zone_id: &str, _iana: &str) -> Result<String, Failure> {
        Err(unused())
    }
    fn push_script(&self, _zone: &str) -> Result<String, Failure> {
        Err(unused())
    }
}

impl FakeReach {
    fn record(&self, text: &str) -> Result<(), Failure> {
        self.calls.borrow_mut().push(text.to_string());
        let name = text.split(' ').next().unwrap_or_default();
        match self.fail.iter().find(|(n, _)| n == name) {
            Some((_, message)) => Err(Failure {
                message: message.clone(),
                caller: CallerFailure::Same,
            }),
            None => Ok(()),
        }
    }

    pub(crate) fn failing(mut self, name: &str, message: &str) -> Self {
        self.fail.push((name.to_string(), message.to_string()));
        self
    }

    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    pub(crate) fn with_seam(mut self, seam: StandinSeam) -> Self {
        self.seam = Some(seam);
        self
    }
}

/// A host of another platform than `linux/arm64`, as a keyed call records it: `key:<host name>`.
fn key_text(key: &HostKey) -> String {
    format!("key:{}", key.host_name())
}

fn target_text(target: Target<'_>) -> String {
    match target {
        Target::Box => "box".to_string(),
        Target::Host(project) => format!("host:{project}"),
    }
}

impl Reach for FakeReach {
    fn push(
        &self,
        _ctx: &Context,
        _say: &dyn Say,
        instance_id: &str,
        alias: &str,
        host: Option<&str>,
    ) {
        self.calls.borrow_mut().push(format!(
            "push {instance_id} {alias} {}",
            host.unwrap_or("-")
        ));
    }
    fn ca(&self, _ctx: &Context, _env: &CaEnv, _say: &dyn Say) -> Result<(), Failure> {
        self.record("ca")
    }
    fn access(&self, _ctx: &Context, _env: &CaEnv, project: &str) -> Result<(), Failure> {
        self.record(&format!("access {project}"))
    }
    fn seam(&self, platform: &str) -> Option<&dyn HostPlatform> {
        match &self.seam {
            Some(seam) => Some(seam),
            None => agentspace_platforms::for_platform(platform),
        }
    }
    // Each keyed call of a `linux/arm64` host goes to the project's call, as the trait's default
    // does, so every test before v0.12 keeps its calls; a host of another platform is recorded by
    // its key.
    fn access_keyed(&self, ctx: &Context, env: &CaEnv, key: &HostKey) -> Result<(), Failure> {
        if key.is_linux_arm64() {
            self.access(ctx, env, &key.project)
        } else {
            self.record(&format!("access {}", key_text(key)))
        }
    }
    fn sync_keyed(
        &self,
        ctx: &Context,
        env: &CaEnv,
        key: &HostKey,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        match crate::commands::linux_arm64_target(key) {
            Some(target) => self.sync(ctx, env, target, say, aws, remote),
            None => self.record(&format!("sync {}", key_text(key))),
        }
    }
    fn deploy_keyed(
        &self,
        ctx: &Context,
        env: &CaEnv,
        key: &HostKey,
        instance_type: Option<&str>,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        match crate::commands::linux_arm64_target(key) {
            Some(target) => self.deploy(ctx, env, target, instance_type, say, aws, remote),
            None => self.record(&format!(
                "deploy {} {}",
                key_text(key),
                instance_type.unwrap_or("-")
            )),
        }
    }
    fn bootstrap_keyed(
        &self,
        ctx: &Context,
        env: &CaEnv,
        key: &HostKey,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        match crate::commands::linux_arm64_target(key) {
            Some(target) => self.bootstrap(ctx, env, target, say, aws, remote),
            None => self.record(&format!("bootstrap {}", key_text(key))),
        }
    }
    fn provision_keyed(
        &self,
        ctx: &Context,
        env: &CaEnv,
        key: &HostKey,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        if key.is_linux_arm64() {
            self.provision(ctx, env, &key.project, say, aws, remote)
        } else {
            self.record(&format!("provision {}", key_text(key)))
        }
    }
    fn sync(
        &self,
        _ctx: &Context,
        _env: &CaEnv,
        target: Target<'_>,
        _say: &dyn Say,
        _aws: &dyn Aws,
        _remote: &dyn Remote,
    ) -> Result<(), Failure> {
        self.record(&format!("sync {}", target_text(target)))
    }
    fn sync_all(
        &self,
        _ctx: &Context,
        _env: &CaEnv,
        _say: &dyn Say,
        _aws: &dyn Aws,
        _remote: &dyn Remote,
    ) -> Result<(), Failure> {
        self.record("sync-all")
    }
    fn deploy(
        &self,
        _ctx: &Context,
        _env: &CaEnv,
        target: Target<'_>,
        instance_type: Option<&str>,
        _say: &dyn Say,
        _aws: &dyn Aws,
        _remote: &dyn Remote,
    ) -> Result<(), Failure> {
        self.record(&format!(
            "deploy {} {}",
            target_text(target),
            instance_type.unwrap_or("-")
        ))
    }
    fn bootstrap(
        &self,
        _ctx: &Context,
        _env: &CaEnv,
        target: Target<'_>,
        _say: &dyn Say,
        _aws: &dyn Aws,
        _remote: &dyn Remote,
    ) -> Result<(), Failure> {
        self.record(&format!("bootstrap {}", target_text(target)))
    }
    fn provision(
        &self,
        _ctx: &Context,
        _env: &CaEnv,
        project: &str,
        _say: &dyn Say,
        _aws: &dyn Aws,
        _remote: &dyn Remote,
    ) -> Result<(), Failure> {
        self.record(&format!("provision {project}"))
    }
}
