//! Test helpers: a throwaway home under the temp folder, and fakes for the traits the commands
//! work against.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

use agentspace_box::Clock;
use agentspace_ca::CaEnv;
use agentspace_operator_api::{
    Aws, AwsError, Call, CallerFailure, CallerLine, Context, Failure, Mode, Output, Remote, Say,
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
