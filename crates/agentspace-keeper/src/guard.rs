//! The keeper's start of the guard (docs/keeper.md, Starting the guard; guard.lock: tried, never
//! held; D56, D63).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! **This module decides what an attempt does; `watch` decides when.** An attempt is four steps
//! (keeper.md, How the keeper starts it): try `guard.lock`, find the guard's program, start it
//! detached with the one argument `guard`, and log one line. This module gives the first three
//! and the line's text, as an [`Outcome`]; the keeper's loop makes the attempt at its due points,
//! logs the line and keeps the time of the last attempt (keeper.md, When the keeper starts it).
//!
//! **The seam is [`GuardStarter`]**, one of [`crate::Deps`]: the try of `guard.lock` and the
//! detached start. A test gives a fake, so no test starts a process or opens a real lock unless it
//! means to. The real one is [`DetachGuard`], over `agentspace-detach` (D56, D63), which the binary
//! builds; it bounds the start at 10 seconds itself (keeper.md, pin P19).
//!
//! **The keeper never talks to AWS or a host itself** (keeper.md, Where it runs): it starts a
//! program and never waits for it, tracks it, reads `guard.log` or learns how it exited.

use crate::args::is_batch;
use agentspace_detach::Detach;
use std::ffi::{OsStr, OsString};
use std::fs::OpenOptions;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::Duration;

/// The variable that names a stand-in for the guard's program (D63).
pub const GUARD_CMD: &str = "AGENTSPACE_GUARD_CMD";

/// The operator's program, beside the keeper's own (keeper.md, The program).
const OPERATOR_FILE: &str = "agentspace-operator.exe";

/// The one argument the guard's program receives (D63).
const GUARD_ARGUMENT: &str = "guard";

/// How long the detached start may take before it is abandoned (keeper.md, How the keeper starts
/// it; pin P19).
pub const START_BOUND: Duration = Duration::from_secs(10);

/// The seams of the guard's environment that the keeper passes or removes by name, beside any
/// other `AGENTSPACE_*_CMD` its own environment holds (keeper.md, The guard's environment). Each is
/// one the specs name.
const KNOWN_SEAMS: [&str; 9] = [
    "AGENTSPACE_AWS_CMD",
    "AGENTSPACE_SSH_CMD",
    "AGENTSPACE_SCP_CMD",
    "AGENTSPACE_GUARD_CMD",
    "AGENTSPACE_OPERATOR_CMD",
    "AGENTSPACE_PROVISION_CMD",
    "AGENTSPACE_KEEPER_CMD",
    "AGENTSPACE_PEER_CMD",
    "AGENTSPACE_SSH_PROXY_CMD",
];

// The `<why>` texts of keeper.md, The keeper's lines for it, byte for byte. Each names no path.

/// `AGENTSPACE_GUARD_CMD` names no existing file.
pub(crate) const NAMED_MISSING: &str =
    "The program that AGENTSPACE_GUARD_CMD names does not exist.";
/// `AGENTSPACE_GUARD_CMD` unset or empty, and no operator beside the keeper.
pub(crate) const NOT_BESIDE: &str = "The operator program is not installed beside this program.";
/// A `.cmd` or `.bat` program, with `SystemRoot` unset or empty.
pub(crate) const NO_INTERPRETER: &str =
    "Windows' command interpreter cannot be found, because SystemRoot is not set.";

/// What one try of `guard.lock` found (keeper.md, guard.lock: tried, never held).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardLockTry {
    /// No guard holds it: the file or its folder does not exist, or it was opened and its handle
    /// has been closed.
    Free,
    /// Another process has it open: a guard runs.
    Held,
    /// It could not be opened for another reason, such as access denied or a folder at its path:
    /// the Windows error number, `None` when the system gave none. It is not a free lock.
    Failed(Option<i32>),
}

/// The keeper's seam for the guard's start: the try of `guard.lock` and the detached start.
///
/// The real one is [`DetachGuard`]. A test gives its own, which starts nothing.
pub trait GuardStarter {
    /// Tries `guard.lock` under `home` once, without waiting, and lets it go at once if it got it
    /// (keeper.md, guard.lock: tried, never held). It MUST NOT create the file or its folder, write
    /// or remove it, or keep a handle on it after it returns.
    fn try_lock(&self, home: &Path) -> GuardLockTry;

    /// Starts `program` with `args`, detached, in the working folder `cwd`, with each
    /// `(name, value)` of `env` set on top of the logon environment (a pair whose value is empty
    /// makes that variable absent), and returns once the process is created, never waiting for it.
    ///
    /// `Ok(Some(pid))` is the process ID the start returned; `Ok(None)` a start that created the
    /// process but gave no process ID. `Err` is the detached start's own error, in agentspace's
    /// words, with no path. The call MUST return within [`START_BOUND`], with
    /// `The call did not return within 10 seconds.` when it reached that bound.
    fn start(
        &self,
        program: &Path,
        args: &[&str],
        cwd: &Path,
        env: &[(&str, &OsStr)],
    ) -> Result<Option<u32>, String>;

    /// The names of every variable of the keeper's own environment. The keeper passes the guard
    /// each one whose name is `AGENTSPACE_*_CMD` (keeper.md, The guard's environment), and
    /// [`crate::Env`] reads a variable by name but cannot list them. No value is read here. The
    /// default is none, which leaves the seams the specs name.
    fn variable_names(&self) -> Vec<OsString> {
        Vec::new()
    }
}

/// The real [`GuardStarter`]: `guard.lock` tried with one open, and the start through an
/// `agentspace-detach` start (D56, D63), bounded at [`START_BOUND`] on a thread of its own.
pub struct DetachGuard {
    detach: Arc<dyn Detach + Send + Sync>,
    names: Vec<OsString>,
    bound: Duration,
}

impl DetachGuard {
    /// The guard's start over `detach`, `WmiDetach` on Windows, with `names` the names of the
    /// keeper's own environment variables.
    pub fn new(detach: Arc<dyn Detach + Send + Sync>, names: Vec<OsString>) -> DetachGuard {
        DetachGuard {
            detach,
            names,
            bound: START_BOUND,
        }
    }

    /// The same with another bound, for the crate's tests of it.
    #[cfg(test)]
    fn with_bound(detach: Arc<dyn Detach + Send + Sync>, bound: Duration) -> DetachGuard {
        DetachGuard {
            detach,
            names: Vec::new(),
            bound,
        }
    }
}

impl GuardStarter for DetachGuard {
    fn try_lock(&self, home: &Path) -> GuardLockTry {
        try_guard_lock_once(&agentspace_hosts::guard_lock_path(home))
    }

    fn start(
        &self,
        program: &Path,
        args: &[&str],
        cwd: &Path,
        env: &[(&str, &OsStr)],
    ) -> Result<Option<u32>, String> {
        // The call runs on a thread of its own, which owns all it uses, so a call that reaches
        // the bound is abandoned: its thread is never joined, and its answer never read.
        let detach = Arc::clone(&self.detach);
        let program = program.to_path_buf();
        let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
        let cwd = cwd.to_path_buf();
        let env: Vec<(String, OsString)> = env
            .iter()
            .map(|(n, v)| ((*n).to_string(), (*v).to_os_string()))
            .collect();
        let (answer, receive) = mpsc::channel();
        std::thread::Builder::new()
            .name("agentspace-keeper-guard-start".to_string())
            .spawn(move || {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let env: Vec<(&str, &OsStr)> = env
                    .iter()
                    .map(|(n, v)| (n.as_str(), v.as_os_str()))
                    .collect();
                // Nobody reads the answer of an abandoned call.
                let _ = answer.send(detach.start_pid(&program, &args, &cwd, &env));
            })
            .map_err(|e| format!("A thread for the start could not be made: {e}"))?;
        match receive.recv_timeout(self.bound) {
            // `Some(pid)` from `WmiDetach` (keeper.md, Starting the guard, step 4); `None` only
            // from a start whose process ID is unknown, which the line then writes as `-`.
            Ok(Ok(pid)) => Ok(pid),
            Ok(Err(error)) => Err(detach_error(&error, self.bound)),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(not_within(self.bound)),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("The start ended without an answer.".to_string())
            }
        }
    }

    fn variable_names(&self) -> Vec<OsString> {
        self.names.clone()
    }
}

/// `The call did not return within <n> seconds.`, for the bound `bound`.
fn not_within(bound: Duration) -> String {
    format!(
        "The call did not return within {} seconds.",
        bound.as_secs_f64()
    )
}

/// The detached start's own error, as the line gives it. `agentspace-detach` bounds its own call
/// at the same 10 seconds, and words that bound after its own prefix; either bound gives the one
/// text keeper.md pins.
fn detach_error(error: &str, bound: Duration) -> String {
    let pinned = not_within(bound);
    if error.ends_with(&pinned) || error.ends_with(&not_within(START_BOUND)) {
        pinned
    } else {
        error.to_string()
    }
}

/// One open of `guard.lock`, without waiting: for reading and writing, sharing nothing, the file
/// only if it exists, with a handle that is not inheritable (the standard library's are not),
/// closed before this returns (keeper.md, guard.lock: tried, never held).
fn try_guard_lock_once(path: &Path) -> GuardLockTry {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    match options.open(path) {
        Ok(file) => free_unless_locked(file),
        Err(e) => read_open_error(&e),
    }
}

/// What an open that failed says: held on a sharing or lock violation, free when the file or its
/// folder does not exist, and failed otherwise.
fn read_open_error(e: &io::Error) -> GuardLockTry {
    /// `ERROR_SHARING_VIOLATION` and `ERROR_LOCK_VIOLATION`.
    const SHARING_VIOLATION: i32 = 32;
    const LOCK_VIOLATION: i32 = 33;
    match e.raw_os_error() {
        Some(SHARING_VIOLATION | LOCK_VIOLATION) if cfg!(windows) => GuardLockTry::Held,
        _ if e.kind() == io::ErrorKind::NotFound => GuardLockTry::Free,
        code => GuardLockTry::Failed(code),
    }
}

/// On Windows an open that shares nothing is the whole test. Elsewhere, where the guard's lock is
/// an advisory one (agentspace-hosts), the file is tried for that lock too.
#[cfg(windows)]
fn free_unless_locked(file: std::fs::File) -> GuardLockTry {
    drop(file);
    GuardLockTry::Free
}

#[cfg(not(windows))]
fn free_unless_locked(file: std::fs::File) -> GuardLockTry {
    match file.try_lock() {
        Ok(()) => GuardLockTry::Free,
        Err(std::fs::TryLockError::WouldBlock) => GuardLockTry::Held,
        Err(std::fs::TryLockError::Error(e)) => GuardLockTry::Failed(e.raw_os_error()),
    }
}

/// What the keeper resolves once, when its loop starts, for every attempt (keeper.md, The program;
/// The guard's environment). The program's file is looked for at each attempt. It has no `Debug`,
/// since it holds the values of the keeper's variables, which MUST NOT reach a log or a failed
/// assertion.
pub(crate) struct GuardSetup {
    /// `AGENTSPACE_GUARD_CMD`, set and not empty, made full against the keeper's working folder.
    named: Option<PathBuf>,
    /// The operator beside the keeper's own program, when its folder is known.
    beside: Option<PathBuf>,
    /// `SystemRoot`, set and not empty, without a `\` or `/` at its end.
    system_root: Option<String>,
    /// The home folder: the guard's working folder, and where `guard.lock` is tried.
    pub(crate) home: PathBuf,
    /// The guard's environment, as pairs on top of the logon environment; an empty value removes
    /// the variable.
    env: Vec<(String, OsString)>,
}

/// A value of the keeper's environment that is set and not empty.
fn non_empty(var: &dyn Fn(&str) -> Option<OsString>, name: &str) -> Option<OsString> {
    var(name).filter(|v| !v.is_empty())
}

/// A relative path made full against `cwd`, as agentspace-state makes a relative folder full: a
/// path with a drive or a root but not both is completed by the system, since joining it would
/// drop `cwd`.
fn full(path: PathBuf, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    let has_prefix = matches!(path.components().next(), Some(Component::Prefix(_)));
    if (has_prefix || path.has_root())
        && let Ok(done) = std::path::absolute(&path)
    {
        return done;
    }
    cwd.join(path)
}

/// Whether `name` is `AGENTSPACE_`, at least one character, then `_CMD`, without regard to case.
fn is_seam(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.len() > "AGENTSPACE_".len() + "_CMD".len()
        && upper.starts_with("AGENTSPACE_")
        && upper.ends_with("_CMD")
}

impl GuardSetup {
    /// Resolves the program's inputs and the guard's environment from the keeper's own (keeper.md,
    /// The program; The guard's environment). `home` is the keeper's `USERPROFILE`, made full.
    /// `names` are the names of the keeper's environment variables, as [`GuardStarter`] lists them.
    pub(crate) fn new(
        var: &dyn Fn(&str) -> Option<OsString>,
        cwd: &Path,
        own_exe: &Path,
        home: &Path,
        names: &[OsString],
    ) -> GuardSetup {
        let named = non_empty(var, GUARD_CMD).map(|v| full(PathBuf::from(v), cwd));
        let beside = own_exe
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.join(OPERATOR_FILE));
        let system_root = non_empty(var, "SystemRoot")
            .map(|v| {
                v.to_string_lossy()
                    .trim_end_matches(['\\', '/'])
                    .to_string()
            })
            .filter(|r| !r.is_empty());

        // The two folders, each made full as state.md makes a relative folder full, and passed
        // only when the keeper has it set and not empty.
        let folder = |name: &str| {
            non_empty(var, name)
                .map(|v| full(PathBuf::from(v), cwd).into_os_string())
                .unwrap_or_default()
        };
        let mut env: Vec<(String, OsString)> = vec![
            ("USERPROFILE".to_string(), home.as_os_str().to_os_string()),
            (
                "AGENTSPACE_STATE_DIR".to_string(),
                folder("AGENTSPACE_STATE_DIR"),
            ),
            (
                "AGENTSPACE_SSH_DIR".to_string(),
                folder("AGENTSPACE_SSH_DIR"),
            ),
            // A command line, passed exactly as the keeper has it, never made full.
            (
                "AGENTSPACE_PROXY_COMMAND".to_string(),
                non_empty(var, "AGENTSPACE_PROXY_COMMAND").unwrap_or_default(),
            ),
        ];

        // Every AGENTSPACE_*_CMD, as given: the seams the specs name, so a logon value of one the
        // keeper does not have is removed, then any other of the keeper's own, once each without
        // regard to case.
        let mut seams: Vec<String> = Vec::new();
        let own = names
            .iter()
            .filter_map(|n| n.to_str())
            .filter(|n| is_seam(n));
        for name in KNOWN_SEAMS.into_iter().chain(own) {
            if !seams.iter().any(|s| s.eq_ignore_ascii_case(name)) {
                seams.push(name.to_string());
            }
        }
        for name in seams {
            let value = non_empty(var, &name).unwrap_or_default();
            env.push((name, value));
        }

        GuardSetup {
            named,
            beside,
            system_root,
            home: home.to_path_buf(),
            env,
        }
    }

    /// Step 2: the program and its arguments, or the `<why>` of a program that cannot be found
    /// (keeper.md, The program). The file is looked for now, at each attempt.
    fn program(&self) -> Result<(PathBuf, Vec<String>), &'static str> {
        let (path, missing) = match (&self.named, &self.beside) {
            (Some(named), _) => (named, NAMED_MISSING),
            (None, Some(beside)) => (beside, NOT_BESIDE),
            (None, None) => return Err(NOT_BESIDE),
        };
        if !path.is_file() {
            return Err(missing);
        }
        if !is_batch(&path.to_string_lossy()) {
            return Ok((path.clone(), vec![GUARD_ARGUMENT.to_string()]));
        }
        let Some(root) = self.system_root.as_deref() else {
            return Err(NO_INTERPRETER);
        };
        // The program is an argument of cmd.exe, which a command line carries only as text.
        let Some(text) = path.to_str() else {
            return Err(missing);
        };
        Ok((
            PathBuf::from(format!("{root}\\System32\\cmd.exe")),
            vec![
                "/d".to_string(),
                "/c".to_string(),
                text.to_string(),
                GUARD_ARGUMENT.to_string(),
            ],
        ))
    }
}

/// What an attempt came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// `guard.lock` is held: nothing was started, nothing is logged, and it is not an attempt.
    Held,
    /// The guard was started; the process ID the start returned, if it returned one.
    Started(Option<u32>),
    /// The attempt failed: the `<why>` of its line.
    Failed(String),
}

impl Outcome {
    /// The line's text after its time, or `None` for a lock found held (keeper.md, The keeper's
    /// lines for it). A start whose process ID is unknown writes `-`; `WmiDetach` gives the ID,
    /// so the real start on Windows never does.
    pub(crate) fn text(&self) -> Option<String> {
        match self {
            Outcome::Held => None,
            Outcome::Started(pid) => Some(format!(
                "guard started pid={}",
                pid.map_or_else(|| "-".to_string(), |p| p.to_string())
            )),
            Outcome::Failed(why) => Some(failed_reason(why)),
        }
    }
}

/// The reason of a guard's `failed` line: `The guard could not be started: <why>`.
pub(crate) fn failed_reason(why: &str) -> String {
    format!("The guard could not be started: {why}")
}

/// The `<why>` of an error that escaped the attempt, a panic included.
pub(crate) fn escaped(error: &str) -> String {
    format!("The start stopped on an error: {error}")
}

/// Steps 1 to 3 of an attempt (keeper.md, How the keeper starts it): try `guard.lock`, find the
/// program, start it detached with the argument `guard` in the home folder and the guard's
/// environment. Step 4, the line, is the caller's.
pub(crate) fn attempt(setup: &GuardSetup, starter: &dyn GuardStarter) -> Outcome {
    match starter.try_lock(&setup.home) {
        GuardLockTry::Held => return Outcome::Held,
        GuardLockTry::Failed(code) => {
            let number = code.map_or_else(|| "unknown".to_string(), |n| n.to_string());
            return Outcome::Failed(format!(
                "guard.lock could not be opened: Windows error {number}."
            ));
        }
        GuardLockTry::Free => {}
    }
    let (program, args) = match setup.program() {
        Ok(found) => found,
        Err(why) => return Outcome::Failed(why.to_string()),
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let env: Vec<(&str, &OsStr)> = setup
        .env
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_os_str()))
        .collect();
    match starter.start(&program, &args, &setup.home, &env) {
        Ok(pid) => Outcome::Started(pid),
        Err(error) => Outcome::Failed(format!("The detached start failed: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn tempdir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("agentspace-keeper-unit-guard-")
            .tempdir()
            .expect("A throwaway folder can be made")
    }

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |name| map.get(name).cloned()
    }

    /// The value the setup passes for `name`, or `None` when it passes no pair of that name.
    fn passed(setup: &GuardSetup, name: &str) -> Option<OsString> {
        setup
            .env
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    }

    /// One start a fake was asked for. It has no `Debug`, since it holds an environment's values.
    struct Asked {
        program: PathBuf,
        args: Vec<String>,
        cwd: PathBuf,
        env: Vec<(String, OsString)>,
    }

    struct Fake {
        lock: GuardLockTry,
        answer: Result<Option<u32>, String>,
        asked: RefCell<Vec<Asked>>,
        tried: RefCell<Vec<PathBuf>>,
    }

    impl Fake {
        fn new(lock: GuardLockTry, answer: Result<Option<u32>, String>) -> Fake {
            Fake {
                lock,
                answer,
                asked: RefCell::new(Vec::new()),
                tried: RefCell::new(Vec::new()),
            }
        }
    }

    impl GuardStarter for Fake {
        fn try_lock(&self, home: &Path) -> GuardLockTry {
            self.tried.borrow_mut().push(home.to_path_buf());
            self.lock
        }
        fn start(
            &self,
            program: &Path,
            args: &[&str],
            cwd: &Path,
            env: &[(&str, &OsStr)],
        ) -> Result<Option<u32>, String> {
            self.asked.borrow_mut().push(Asked {
                program: program.to_path_buf(),
                args: args.iter().map(|a| (*a).to_string()).collect(),
                cwd: cwd.to_path_buf(),
                env: env
                    .iter()
                    .map(|(n, v)| ((*n).to_string(), (*v).to_os_string()))
                    .collect(),
            });
            self.answer.clone()
        }
    }

    /// A throwaway home with a stand-in guard in it, and the setup for a keeper whose own program
    /// is beside nothing.
    struct Place {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        home: PathBuf,
        stand_in: PathBuf,
    }

    fn place() -> Place {
        let tmp = tempdir();
        let root = tmp.path().canonicalize().unwrap();
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let stand_in = home.join("guard-stand-in.exe");
        std::fs::write(&stand_in, b"stand-in").unwrap();
        Place {
            _tmp: tmp,
            root,
            home,
            stand_in,
        }
    }

    fn setup_with(p: &Place, pairs: &[(&str, &str)], names: &[&str]) -> GuardSetup {
        let names: Vec<OsString> = names.iter().map(OsString::from).collect();
        GuardSetup::new(
            &vars(pairs),
            &p.root,
            &p.root.join("bin").join("agentspace-keeper.exe"),
            &p.home,
            &names,
        )
    }

    #[test]
    fn a_free_lock_starts_the_named_program_with_the_one_argument_in_the_home_folder() {
        let p = place();
        let stand_in = p.stand_in.to_string_lossy().into_owned();
        let setup = setup_with(&p, &[(GUARD_CMD, &stand_in)], &[]);
        let fake = Fake::new(GuardLockTry::Free, Ok(Some(31244)));
        assert_eq!(attempt(&setup, &fake), Outcome::Started(Some(31244)));
        assert_eq!(*fake.tried.borrow(), vec![p.home.clone()]);
        let asked = fake.asked.borrow();
        assert_eq!(asked.len(), 1, "One start per attempt");
        assert_eq!(asked[0].program, p.stand_in);
        assert_eq!(asked[0].args, vec!["guard"], "Exactly the one argument");
        assert_eq!(
            asked[0].cwd, p.home,
            "The guard's working folder is the home"
        );
        // The start is given the setup's environment, the home first. Compared without
        // formatting a value (AGENTS.md: a test never prints an environment's values).
        assert!(
            asked[0].env == setup.env,
            "The start was not given the setup's environment"
        );
        assert_eq!(asked[0].env[0].0, "USERPROFILE");
        assert!(
            asked[0].env[0].1 == p.home.as_os_str(),
            "USERPROFILE is not the home"
        );
        assert_eq!(
            Outcome::Started(Some(31244)).text().as_deref(),
            Some("guard started pid=31244")
        );
    }

    #[test]
    fn a_held_lock_starts_nothing_and_logs_nothing() {
        let p = place();
        let stand_in = p.stand_in.to_string_lossy().into_owned();
        let setup = setup_with(&p, &[(GUARD_CMD, &stand_in)], &[]);
        let fake = Fake::new(GuardLockTry::Held, Ok(Some(1)));
        assert_eq!(attempt(&setup, &fake), Outcome::Held);
        assert!(fake.asked.borrow().is_empty());
        assert_eq!(Outcome::Held.text(), None);
    }

    #[test]
    fn a_lock_that_cannot_be_opened_is_a_failed_attempt_with_the_number_alone() {
        let p = place();
        let stand_in = p.stand_in.to_string_lossy().into_owned();
        let setup = setup_with(&p, &[(GUARD_CMD, &stand_in)], &[]);
        let fake = Fake::new(GuardLockTry::Failed(Some(5)), Ok(Some(1)));
        let outcome = attempt(&setup, &fake);
        assert!(fake.asked.borrow().is_empty(), "Nothing is started");
        assert_eq!(
            outcome.text().as_deref(),
            Some(
                "The guard could not be started: guard.lock could not be opened: Windows error 5."
            )
        );
    }

    #[test]
    fn a_named_program_that_does_not_exist_fails_the_attempt() {
        let p = place();
        let missing = p.home.join("no-such-guard.exe");
        let setup = setup_with(&p, &[(GUARD_CMD, &missing.to_string_lossy())], &[]);
        let fake = Fake::new(GuardLockTry::Free, Ok(Some(1)));
        assert_eq!(
            attempt(&setup, &fake),
            Outcome::Failed(NAMED_MISSING.into())
        );
        assert!(fake.asked.borrow().is_empty());
        // A folder is not a file either.
        let setup = setup_with(&p, &[(GUARD_CMD, &p.home.to_string_lossy())], &[]);
        assert_eq!(
            attempt(&setup, &fake),
            Outcome::Failed(NAMED_MISSING.into())
        );
        assert_eq!(
            Outcome::Failed(NAMED_MISSING.into()).text().as_deref(),
            Some(
                "The guard could not be started: The program that AGENTSPACE_GUARD_CMD names \
                 does not exist."
            )
        );
    }

    /// keeper.md, Conformance cases for the guard's start: this `<why>` is the crate's own test.
    #[test]
    fn with_no_stand_in_and_no_operator_beside_the_keeper_the_operator_is_not_installed() {
        let p = place();
        let fake = Fake::new(GuardLockTry::Free, Ok(Some(1)));
        for unset in [vec![], vec![(GUARD_CMD, "")]] {
            let setup = setup_with(&p, &unset, &[]);
            assert_eq!(attempt(&setup, &fake), Outcome::Failed(NOT_BESIDE.into()));
        }
        assert!(fake.asked.borrow().is_empty());
        assert_eq!(
            Outcome::Failed(NOT_BESIDE.into()).text().as_deref(),
            Some(
                "The guard could not be started: The operator program is not installed beside \
                 this program."
            )
        );
        // A keeper whose own program has no folder is beside nothing.
        let setup = GuardSetup::new(&vars(&[]), &p.root, Path::new("keeper"), &p.home, &[]);
        assert_eq!(attempt(&setup, &fake), Outcome::Failed(NOT_BESIDE.into()));
    }

    #[test]
    fn with_no_stand_in_the_operator_beside_the_keeper_is_started_and_looked_for_each_time() {
        let p = place();
        let setup = setup_with(&p, &[], &[]);
        let fake = Fake::new(GuardLockTry::Free, Ok(Some(9)));
        assert_eq!(attempt(&setup, &fake), Outcome::Failed(NOT_BESIDE.into()));
        // An install that adds the operator is used from the next attempt on. The file is a
        // throwaway one beside a keeper path in the throwaway folder; nothing runs it.
        let bin = p.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join(OPERATOR_FILE), b"not a program").unwrap();
        assert_eq!(attempt(&setup, &fake), Outcome::Started(Some(9)));
        assert_eq!(fake.asked.borrow()[0].program, bin.join(OPERATOR_FILE));
        assert_eq!(fake.asked.borrow()[0].args, vec!["guard"]);
    }

    #[test]
    fn a_relative_stand_in_is_made_full_against_the_keepers_working_folder() {
        let p = place();
        let setup = setup_with(&p, &[(GUARD_CMD, r"home/guard-stand-in.exe")], &[]);
        let fake = Fake::new(GuardLockTry::Free, Ok(Some(3)));
        assert_eq!(attempt(&setup, &fake), Outcome::Started(Some(3)));
        assert_eq!(
            fake.asked.borrow()[0].program,
            p.root.join(r"home/guard-stand-in.exe")
        );
        // The variable itself reaches the guard as given, not made full (pin P18).
        assert_eq!(
            passed(&setup, GUARD_CMD),
            Some(OsString::from(r"home/guard-stand-in.exe"))
        );
    }

    #[test]
    fn a_batch_runs_through_cmd_exe_with_the_argument_after_its_path() {
        let p = place();
        let spaced = p.home.join("a folder");
        std::fs::create_dir_all(&spaced).unwrap();
        for name in ["guard stand-in.cmd", "GUARD.BAT"] {
            let batch = spaced.join(name);
            std::fs::write(&batch, b"@exit /b 0\r\n").unwrap();
            let text = batch.to_string_lossy().into_owned();
            let setup = setup_with(
                &p,
                &[(GUARD_CMD, &text), ("SystemRoot", r"C:\Windows\")],
                &[],
            );
            let fake = Fake::new(GuardLockTry::Free, Ok(Some(4)));
            assert_eq!(attempt(&setup, &fake), Outcome::Started(Some(4)));
            let asked = fake.asked.borrow();
            assert_eq!(
                asked[0].program,
                PathBuf::from(r"C:\Windows\System32\cmd.exe")
            );
            assert_eq!(asked[0].args, vec!["/d", "/c", text.as_str(), "guard"]);
        }
    }

    /// keeper.md, Conformance cases for the guard's start: this `<why>` is the crate's own test.
    #[test]
    fn a_batch_with_no_system_root_is_a_program_that_cannot_be_found() {
        let p = place();
        let batch = p.home.join("stand-in.cmd");
        std::fs::write(&batch, b"@exit /b 0\r\n").unwrap();
        let text = batch.to_string_lossy().into_owned();
        let fake = Fake::new(GuardLockTry::Free, Ok(Some(4)));
        for root in [None, Some(""), Some(r"\")] {
            let mut pairs = vec![(GUARD_CMD, text.as_str())];
            if let Some(r) = root {
                pairs.push(("SystemRoot", r));
            }
            let setup = setup_with(&p, &pairs, &[]);
            assert_eq!(
                attempt(&setup, &fake),
                Outcome::Failed(NO_INTERPRETER.into())
            );
        }
        assert!(fake.asked.borrow().is_empty());
        assert_eq!(
            Outcome::Failed(NO_INTERPRETER.into()).text().as_deref(),
            Some(
                "The guard could not be started: Windows' command interpreter cannot be found, \
                 because SystemRoot is not set."
            )
        );
    }

    /// keeper.md, Conformance cases for the guard's start: this `<why>` is the crate's own test.
    #[test]
    fn a_detached_start_that_fails_gives_its_own_error() {
        let p = place();
        let stand_in = p.stand_in.to_string_lossy().into_owned();
        let setup = setup_with(&p, &[(GUARD_CMD, &stand_in)], &[]);
        let fake = Fake::new(
            GuardLockTry::Free,
            Err("Win32_Process.Create returned 9: the path was not found.".into()),
        );
        assert_eq!(
            attempt(&setup, &fake).text().as_deref(),
            Some(
                "The guard could not be started: The detached start failed: \
                 Win32_Process.Create returned 9: the path was not found."
            )
        );
        assert_eq!(
            fake.asked.borrow().len(),
            1,
            "Not tried again in the attempt"
        );
    }

    /// A start whose process ID is unknown (the default `start_pid`, `NoDetach` off Windows): the
    /// line writes `-`. The real start on Windows gives the ID (below).
    #[test]
    fn a_start_whose_process_id_is_unknown_writes_a_dash() {
        assert_eq!(
            Outcome::Started(None).text().as_deref(),
            Some("guard started pid=-")
        );
    }

    #[test]
    fn an_escaped_error_has_its_own_why() {
        assert_eq!(
            failed_reason(&escaped("Boom.")),
            "The guard could not be started: The start stopped on an error: Boom."
        );
    }

    #[test]
    fn the_guard_gets_the_home_the_folders_the_proxy_and_every_seam_and_nothing_else() {
        let p = place();
        let pairs = [
            (
                "USERPROFILE",
                "ignored: the home is the keeper's, made full",
            ),
            ("AGENTSPACE_STATE_DIR", "state"),
            ("AGENTSPACE_SSH_DIR", r"ssh"),
            ("AGENTSPACE_PROXY_COMMAND", r"proxy %h"),
            ("AGENTSPACE_AWS_CMD", r"aws-stand-in.cmd"),
            ("AGENTSPACE_GUARD_CMD", r"home\guard-stand-in.exe"),
            ("AGENTSPACE_EXTRA_CMD", r"C:\x\extra.cmd"),
            ("AGENTSPACE_TIMEZONE", "Etc/UTC"),
            ("AGENTSPACE_DIAGNOSTIC", "1"),
            ("CASE_OWN_VARIABLE", "x"),
        ];
        let names = [
            "USERPROFILE",
            "AGENTSPACE_STATE_DIR",
            "AGENTSPACE_EXTRA_CMD",
            "agentspace_aws_cmd",
            "AGENTSPACE__CMD",
            "AGENTSPACE_CMD",
            "AGENTSPACE_TIMEZONE",
            "AGENTSPACE_DIAGNOSTIC",
            "CASE_OWN_VARIABLE",
        ];
        let setup = setup_with(&p, &pairs, &names);
        let get = |n: &str| passed(&setup, n);
        assert_eq!(get("USERPROFILE"), Some(p.home.clone().into_os_string()));
        assert_eq!(
            get("AGENTSPACE_STATE_DIR"),
            Some(p.root.join("state").into_os_string()),
            "Made full against the keeper's working folder"
        );
        assert_eq!(
            get("AGENTSPACE_SSH_DIR"),
            Some(p.root.join("ssh").into_os_string())
        );
        assert_eq!(get("AGENTSPACE_PROXY_COMMAND"), Some("proxy %h".into()));
        assert_eq!(get("AGENTSPACE_AWS_CMD"), Some("aws-stand-in.cmd".into()));
        assert_eq!(
            get("AGENTSPACE_GUARD_CMD"),
            Some(r"home\guard-stand-in.exe".into())
        );
        assert_eq!(get("AGENTSPACE_EXTRA_CMD"), Some(r"C:\x\extra.cmd".into()));
        for absent in [
            "AGENTSPACE_TIMEZONE",
            "AGENTSPACE_DIAGNOSTIC",
            "CASE_OWN_VARIABLE",
            "AGENTSPACE__CMD",
            "AGENTSPACE_CMD",
        ] {
            assert_eq!(get(absent), None, "{absent} MUST NOT reach the guard");
        }
        // One pair per name, whatever its case.
        let aws = setup
            .env
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case("AGENTSPACE_AWS_CMD"))
            .count();
        assert_eq!(aws, 1);
    }

    #[test]
    fn what_the_keeper_does_not_have_is_removed_from_the_logon_environment() {
        let p = place();
        let setup = setup_with(&p, &[("AGENTSPACE_SSH_DIR", "")], &[]);
        for name in [
            "AGENTSPACE_STATE_DIR",
            "AGENTSPACE_SSH_DIR",
            "AGENTSPACE_PROXY_COMMAND",
            "AGENTSPACE_AWS_CMD",
            "AGENTSPACE_SSH_CMD",
            "AGENTSPACE_SCP_CMD",
            "AGENTSPACE_GUARD_CMD",
            "AGENTSPACE_OPERATOR_CMD",
        ] {
            // An empty value makes the variable absent (agentspace-detach, The environment).
            assert_eq!(passed(&setup, name), Some(OsString::new()), "{name}");
        }
    }

    #[test]
    fn a_seam_is_agentspace_then_something_then_cmd() {
        for yes in [
            "AGENTSPACE_AWS_CMD",
            "agentspace_x_cmd",
            "AGENTSPACE_A_B_CMD",
        ] {
            assert!(is_seam(yes), "{yes}");
        }
        for no in [
            "AGENTSPACE__CMD",
            "AGENTSPACE_CMD",
            "AGENTSPACE_AWS",
            "X_AGENTSPACE_AWS_CMD",
            "AGENTSPACE_AWS_CMDX",
        ] {
            assert!(!is_seam(no), "{no}");
        }
    }

    // The real lock try, on real files in a throwaway home.

    #[test]
    fn a_missing_lock_or_folder_is_free_and_the_try_creates_neither() {
        let t = tempdir();
        let real = DetachGuard::new(Arc::new(agentspace_detach::NoDetach), Vec::new());
        assert_eq!(real.try_lock(t.path()), GuardLockTry::Free);
        assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 0);
        std::fs::create_dir_all(t.path().join(".agentspace")).unwrap();
        assert_eq!(real.try_lock(t.path()), GuardLockTry::Free);
        assert!(!agentspace_hosts::guard_lock_path(t.path()).exists());
    }

    #[test]
    fn a_lock_nobody_holds_is_free_and_left_as_it_was() {
        let t = tempdir();
        let lock = agentspace_hosts::guard_lock_path(t.path());
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::write(&lock, b"").unwrap();
        let real = DetachGuard::new(Arc::new(agentspace_detach::NoDetach), Vec::new());
        assert_eq!(real.try_lock(t.path()), GuardLockTry::Free);
        // The handle was closed: a guard can take it at once.
        let taken = agentspace_hosts::try_guard_lock(t.path()).unwrap();
        assert!(taken.is_some(), "The try left no handle open");
        assert_eq!(std::fs::metadata(&lock).unwrap().len(), 0);
    }

    #[test]
    fn a_lock_a_guard_holds_is_held() {
        let t = tempdir();
        let _held = agentspace_hosts::try_guard_lock(t.path()).unwrap().unwrap();
        let real = DetachGuard::new(Arc::new(agentspace_detach::NoDetach), Vec::new());
        assert_eq!(real.try_lock(t.path()), GuardLockTry::Held);
    }

    #[cfg(windows)]
    #[test]
    fn a_folder_at_the_lock_is_a_failure_with_its_windows_error() {
        let t = tempdir();
        let lock = agentspace_hosts::guard_lock_path(t.path());
        std::fs::create_dir_all(&lock).unwrap();
        let real = DetachGuard::new(Arc::new(agentspace_detach::NoDetach), Vec::new());
        // ERROR_ACCESS_DENIED: a folder cannot be opened as a file.
        assert_eq!(real.try_lock(t.path()), GuardLockTry::Failed(Some(5)));
        assert!(lock.is_dir(), "Left as it was");
    }

    // The real start's bound, over a fake detach: nothing is started.

    /// What a fake detach was asked: the program, the arguments, the working folder and the
    /// environment's names.
    type Call = (PathBuf, Vec<String>, PathBuf, Vec<String>);

    /// A detach that records its call and answers, or blocks first.
    #[derive(Default)]
    struct FakeDetach {
        asked: Mutex<Vec<Call>>,
        answer: Mutex<Option<Result<(), String>>>,
        block: Option<Duration>,
    }

    impl Detach for FakeDetach {
        fn start(
            &self,
            program: &Path,
            args: &[&str],
            cwd: &Path,
            env: &[(&str, &OsStr)],
        ) -> Result<(), String> {
            self.asked.lock().unwrap().push((
                program.to_path_buf(),
                args.iter().map(|a| (*a).to_string()).collect(),
                cwd.to_path_buf(),
                env.iter().map(|(n, _)| (*n).to_string()).collect(),
            ));
            if let Some(d) = self.block {
                std::thread::sleep(d);
            }
            self.answer.lock().unwrap().clone().unwrap_or(Ok(()))
        }
    }

    /// A detach whose `start_pid` gives a process ID, as `WmiDetach`'s does, and whose `start` is
    /// never called.
    struct FakeDetachWithPid(u32);

    impl Detach for FakeDetachWithPid {
        fn start(
            &self,
            _: &Path,
            _: &[&str],
            _: &Path,
            _: &[(&str, &OsStr)],
        ) -> Result<(), String> {
            panic!("The keeper's real start goes through start_pid.");
        }

        fn start_pid(
            &self,
            _: &Path,
            _: &[&str],
            _: &Path,
            _: &[(&str, &OsStr)],
        ) -> Result<Option<u32>, String> {
            Ok(Some(self.0))
        }
    }

    /// keeper.md, Starting the guard, step 4: the line names the process ID the detached start
    /// returned, so the real start gives it through, and the line is `guard started pid=<pid>`.
    #[test]
    fn the_real_start_gives_the_process_id_the_detached_start_returned() {
        let real = DetachGuard::new(Arc::new(FakeDetachWithPid(31244)), Vec::new());
        let answer = real.start(
            Path::new(r"C:\x\g.exe"),
            &["guard"],
            Path::new(r"C:\h"),
            &[],
        );
        assert_eq!(answer, Ok(Some(31244)));
        assert_eq!(
            Outcome::Started(answer.unwrap()).text().as_deref(),
            Some("guard started pid=31244")
        );
    }

    #[test]
    fn the_real_start_passes_its_inputs_through_and_answers_no_process_id_when_none_is_known() {
        let fake = Arc::new(FakeDetach::default());
        let real = DetachGuard::new(fake.clone(), Vec::new());
        let value = OsString::from("v");
        let answer = real.start(
            Path::new(r"C:\x\g.exe"),
            &["guard"],
            Path::new(r"C:\h"),
            &[("A", value.as_os_str())],
        );
        assert_eq!(answer, Ok(None));
        let asked = fake.asked.lock().unwrap();
        assert_eq!(
            asked[0],
            (
                PathBuf::from(r"C:\x\g.exe"),
                vec!["guard".to_string()],
                PathBuf::from(r"C:\h"),
                vec!["A".to_string()]
            )
        );
    }

    #[test]
    fn the_real_start_gives_the_detached_starts_own_error() {
        let fake = Arc::new(FakeDetach::default());
        *fake.answer.lock().unwrap() = Some(Err(
            "Win32_Process.Create returned 2: access was denied.".into(),
        ));
        let real = DetachGuard::new(fake, Vec::new());
        assert_eq!(
            real.start(Path::new(r"C:\g.exe"), &["guard"], Path::new(r"C:\h"), &[]),
            Err("Win32_Process.Create returned 2: access was denied.".to_string())
        );
    }

    /// keeper.md, How the keeper starts it (pin P19): a call that has not returned by the bound is
    /// abandoned. The fake blocks for 3 seconds, so an unbounded wait would answer `Ok` after it.
    #[test]
    fn a_start_that_does_not_return_within_the_bound_is_abandoned() {
        let fake = Arc::new(FakeDetach {
            block: Some(Duration::from_secs(3)),
            ..FakeDetach::default()
        });
        let real = DetachGuard::with_bound(fake, Duration::from_millis(100));
        let begun = std::time::Instant::now();
        let answer = real.start(Path::new(r"C:\g.exe"), &["guard"], Path::new(r"C:\h"), &[]);
        let took = begun.elapsed();
        assert_eq!(
            answer,
            Err("The call did not return within 0.1 seconds.".to_string())
        );
        assert!(took < Duration::from_secs(2), "{took:?}");
    }

    #[test]
    fn the_bound_is_ten_seconds_and_either_bound_gives_keeper_mds_text() {
        assert_eq!(START_BOUND, Duration::from_secs(10));
        assert_eq!(
            not_within(START_BOUND),
            "The call did not return within 10 seconds."
        );
        // agentspace-detach's own bound, after its own prefix.
        assert_eq!(
            detach_error(
                "Win32_Process.Create could not be called: The call did not return within 10 \
                 seconds.",
                START_BOUND
            ),
            "The call did not return within 10 seconds."
        );
        assert_eq!(
            failed_reason(&format!(
                "The detached start failed: {}",
                not_within(START_BOUND)
            )),
            "The guard could not be started: The detached start failed: The call did not return \
             within 10 seconds."
        );
        assert_eq!(detach_error("Other.", START_BOUND), "Other.");
    }

    #[test]
    fn the_real_starter_lists_the_names_it_was_given() {
        let names = vec![OsString::from("AGENTSPACE_X_CMD")];
        let real = DetachGuard::new(Arc::new(agentspace_detach::NoDetach), names.clone());
        assert_eq!(real.variable_names(), names);
    }
}
