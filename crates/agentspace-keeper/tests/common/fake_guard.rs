//! The fake of the keeper's seam for the guard's start, `agentspace_keeper::GuardStarter`
//! (`Deps::guard`; docs/keeper.md, Starting the guard).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every in-process world of
//! these tests gives its keeper a `FakeGuard`, which **starts nothing**: it records each try of
//! `guard.lock` and each start (the program, its arguments, its working folder and the variables
//! it was given), and answers as it was told. A keeper started by a test therefore never starts
//! the build's real operator, whatever `AGENTSPACE_GUARD_CMD` holds; the world still sets it to a
//! stand-in in its throwaway home (AGENTS.md; the architect's ruling P15), and a case of the real
//! start runs the built program (`Bin`).
//!
//! `use_real_lock` makes the try the crate's real one, over files of the throwaway home: a test
//! then holds `guard.lock` from a second handle, or puts a folder at its path.

#![allow(dead_code)]

use agentspace_detach::NoDetach;
use agentspace_keeper::{DetachGuard, GuardLockTry, GuardStarter};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One start, as the keeper asked for it.
#[derive(Debug, Clone)]
pub struct GuardStartSeen {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, OsString)>,
}

impl GuardStartSeen {
    /// The value the start gives the variable `name`, the name compared without regard to case,
    /// the last pair of that name winning; `None` when no pair names it. An empty value is a
    /// variable made absent, and is `Some("")`.
    pub fn pair(&self, name: &str) -> Option<OsString> {
        self.env
            .iter()
            .rev()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    }

    /// The value the guard has for `name`: `Some` only for a variable that is set and not empty.
    pub fn given(&self, name: &str) -> Option<OsString> {
        self.pair(name).filter(|v| !v.is_empty())
    }

    /// Asserts that the guard gets `name` with exactly `want`, and reports a mismatch by the
    /// name alone, never by the value.
    pub fn assert_given(&self, name: &str, want: &OsStr) {
        match self.given(name) {
            None => panic!("The guard is not given {name}"),
            Some(v) => assert!(v == want, "The guard is given {name} with another value"),
        }
    }

    /// As `assert_given`, for a path, compared without regard to case.
    pub fn assert_given_path(&self, name: &str, want: &Path) {
        match self.given(name) {
            None => panic!("The guard is not given {name}"),
            Some(v) => assert!(
                v.to_string_lossy()
                    .eq_ignore_ascii_case(&want.to_string_lossy()),
                "The guard is given {name} with another value"
            ),
        }
    }

    /// Asserts that the guard is given no value for `name`.
    pub fn assert_not_given(&self, name: &str) {
        assert!(
            self.given(name).is_none(),
            "The guard is given {name}, which it must not be"
        );
    }

    /// Asserts that the start removes `name` from the logon environment: it names it with an
    /// empty value, so a value the logon environment holds never reaches the guard.
    pub fn assert_removed(&self, name: &str) {
        match self.pair(name) {
            Some(v) if v.is_empty() => {}
            Some(_) => panic!("The guard is given {name}, which the keeper does not have"),
            None => panic!("The start does not remove {name} from the logon environment"),
        }
    }
}

/// What the next try of `guard.lock` does.
#[derive(Debug, Clone)]
pub enum LockStep {
    Answer(GuardLockTry),
    /// The try panics with this message.
    Panic(String),
}

/// What the next start does.
#[derive(Debug, Clone)]
pub enum StartStep {
    /// Returns this answer.
    Answer(Result<Option<u32>, String>),
    /// Panics with this message.
    Panic(String),
}

/// The first process ID the fake gives; each start without a scripted answer gives the next.
pub const FIRST_PID: u32 = 31001;

pub struct FakeGuard {
    real_lock: Cell<bool>,
    locks: RefCell<VecDeque<LockStep>>,
    steps: RefCell<VecDeque<StartStep>>,
    seen: RefCell<Vec<GuardStartSeen>>,
    tries: RefCell<Vec<PathBuf>>,
    names: RefCell<Vec<OsString>>,
    next_pid: Cell<u32>,
    real: DetachGuard,
}

impl Default for FakeGuard {
    fn default() -> Self {
        FakeGuard::new()
    }
}

impl FakeGuard {
    pub fn new() -> FakeGuard {
        FakeGuard {
            real_lock: Cell::new(false),
            locks: RefCell::new(VecDeque::new()),
            steps: RefCell::new(VecDeque::new()),
            seen: RefCell::new(Vec::new()),
            tries: RefCell::new(Vec::new()),
            names: RefCell::new(Vec::new()),
            next_pid: Cell::new(FIRST_PID),
            real: DetachGuard::new(Arc::new(NoDetach), Vec::new()),
        }
    }

    /// The try of `guard.lock` is the crate's real one, over the throwaway home's files.
    pub fn use_real_lock(&self) {
        self.real_lock.set(true);
    }

    /// The next tries answer these, in order; then `Free`.
    pub fn script_locks(&self, steps: Vec<LockStep>) {
        self.locks.borrow_mut().extend(steps);
    }

    /// The next starts do these, in order; then each gives the next process ID.
    pub fn script_starts(&self, steps: Vec<StartStep>) {
        self.steps.borrow_mut().extend(steps);
    }

    /// Sets the names of the keeper's own variables the keeper is told of
    /// (`GuardStarter::variable_names`): the names of a world's variables, never their values.
    pub fn set_names<'a>(&self, names: impl IntoIterator<Item = &'a String>) {
        *self.names.borrow_mut() = names.into_iter().map(OsString::from).collect();
    }

    /// Every start so far, in order.
    pub fn starts(&self) -> Vec<GuardStartSeen> {
        self.seen.borrow().clone()
    }

    /// Every try of `guard.lock` so far, with the home it was given.
    pub fn tries(&self) -> Vec<PathBuf> {
        self.tries.borrow().clone()
    }
}

impl GuardStarter for FakeGuard {
    fn try_lock(&self, home: &Path) -> GuardLockTry {
        self.tries.borrow_mut().push(home.to_path_buf());
        let step = self.locks.borrow_mut().pop_front();
        match step {
            Some(LockStep::Panic(message)) => panic!("{message}"),
            Some(LockStep::Answer(answer)) => answer,
            None if self.real_lock.get() => self.real.try_lock(home),
            None => GuardLockTry::Free,
        }
    }

    fn start(
        &self,
        program: &Path,
        args: &[&str],
        cwd: &Path,
        env: &[(&str, &OsStr)],
    ) -> Result<Option<u32>, String> {
        self.seen.borrow_mut().push(GuardStartSeen {
            program: program.to_path_buf(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            cwd: cwd.to_path_buf(),
            env: env
                .iter()
                .map(|(n, v)| ((*n).to_string(), (*v).to_os_string()))
                .collect(),
        });
        let step = self.steps.borrow_mut().pop_front();
        match step {
            Some(StartStep::Panic(message)) => panic!("{message}"),
            Some(StartStep::Answer(answer)) => answer,
            None => {
                let pid = self.next_pid.get();
                self.next_pid.set(pid + 1);
                Ok(Some(pid))
            }
        }
    }

    fn variable_names(&self) -> Vec<OsString> {
        self.names.borrow().clone()
    }
}
