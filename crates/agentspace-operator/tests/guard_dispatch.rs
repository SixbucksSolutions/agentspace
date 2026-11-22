//! `guard`'s dispatch, in process, over the seam the operator gives a test: `Reach::guard` is the
//! guard's run (the real one is `agentspace-guard`'s), and `Reach::lease_state_dir` is the state
//! folder it reads (docs/operator.md: guard, its steps 3 and 8, Its environment, Its streams,
//! exit codes and help; Environment variables). The program's exit code is the dispatch's
//! `Outcome::exit_code` for an `Ok`, and 1 for an `Err`, which `run` prints, so what is shown here
//! is what the dispatch returns and prints, and the exit code `run` maps it to.
//!
//! Nothing here reads or writes the process's environment: every variable is a value in a lookup,
//! and every path is in a throwaway folder.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agentspace_guard::{Input, Report as GuardReport};
use agentspace_operator::{
    Aws, CaEnv, CallerFailure, Clock, Context, Failure, Mode, Outcome, Parsed, Parts, Printer,
    Reach, Remote, Say, SystemReach, Target, dispatch, parse,
};
use agentspace_operator_api::{AwsError, Call};
use common::Capture;

// ---------------------------------------------------------------------------------------------
// The stand-ins
// ---------------------------------------------------------------------------------------------

/// An AWS CLI that must never be called: the guard's dispatch makes no call of its own.
struct NoAws;

impl Aws for NoAws {
    fn call(&self, _: &Call) -> Result<serde_json::Value, AwsError> {
        panic!("The dispatch of guard made an AWS call of its own");
    }
}

/// ssh and scp, which the guard never starts.
struct NoRemote;

impl Remote for NoRemote {
    fn ssh(&self, _: &[&str], _: Option<&str>) -> Result<i32, Failure> {
        panic!("The dispatch of guard started an ssh");
    }
    fn scp(&self, _: &[&str]) -> Result<i32, Failure> {
        panic!("The dispatch of guard started an scp");
    }
}

/// A clock whose wall time is fixed, with a fraction of a second.
struct Fixed(SystemTime);

impl Clock for Fixed {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
    fn utc_now(&self) -> SystemTime {
        self.0
    }
    fn sleep(&self, _: Duration) {
        panic!("The dispatch of guard waited");
    }
}

/// What a run was handed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Seen {
    home: PathBuf,
    state_dir: PathBuf,
    now: i64,
}

/// A `Reach` whose guard run is a stand-in: it records its input, prints one progress line through
/// the `Say` it is given, and answers as scripted. The state folder is the real reach's, so the
/// variable's reading is the program's own. Every other method is a command the dispatch of
/// `guard` never reaches.
struct Standin {
    real: SystemReach,
    seen: RefCell<Vec<Seen>>,
    outcome: Result<GuardReport, Failure>,
    /// What the run says through `warning`, in order, before it answers: a host's failure, a
    /// dropped log line.
    warnings: Vec<String>,
}

impl Standin {
    fn new(
        profile: &Path,
        env: &[(&str, &Path)],
        outcome: Result<GuardReport, Failure>,
    ) -> Standin {
        let mut pairs: Vec<(String, OsString)> = env
            .iter()
            .map(|(k, v)| (k.to_string(), v.as_os_str().to_os_string()))
            .collect();
        pairs.push((
            "USERPROFILE".to_string(),
            profile.as_os_str().to_os_string(),
        ));
        let lookup = move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        };
        Standin {
            real: SystemReach::new(agentspace_operator::Env::snapshot(&lookup)),
            seen: RefCell::default(),
            outcome,
            warnings: Vec::new(),
        }
    }
}

impl Reach for Standin {
    fn lease_state_dir(&self, ctx: &Context) -> PathBuf {
        self.real.lease_state_dir(ctx)
    }

    fn guard(&self, input: &Input, say: &dyn Say, _aws: &dyn Aws) -> Result<GuardReport, Failure> {
        self.seen.borrow_mut().push(Seen {
            home: input.ctx.home.clone(),
            state_dir: input.state_dir.to_path_buf(),
            now: input.now,
        });
        // A host's failure and a dropped log line come through `warning`, before the closing line.
        for warning in &self.warnings {
            say.warning(warning);
        }
        // The closing line of step 8, which `-Quiet` suppresses. A run that finds `guard.lock` held
        // prints nothing at all (step 1).
        if !matches!(&self.outcome, Ok(report) if report.lock_held) {
            say.progress("Hosts: 0; running 0, stopped 0, unexplained 0.", None);
        }
        self.outcome.clone()
    }

    fn push(&self, _: &Context, _: &dyn Say, _: &str, _: &str, _: Option<&str>) {
        panic!("guard pushed a token");
    }
    fn ca(&self, _: &Context, _: &CaEnv, _: &dyn Say) -> Result<(), Failure> {
        panic!("guard ran ca");
    }
    fn access(&self, _: &Context, _: &CaEnv, _: &str) -> Result<(), Failure> {
        panic!("guard signed a certificate or wrote an alias");
    }
    fn sync(
        &self,
        _: &Context,
        _: &CaEnv,
        _: Target<'_>,
        _: &dyn Say,
        _: &dyn Aws,
        _: &dyn Remote,
    ) -> Result<(), Failure> {
        panic!("guard ran sync");
    }
    fn sync_all(
        &self,
        _: &Context,
        _: &CaEnv,
        _: &dyn Say,
        _: &dyn Aws,
        _: &dyn Remote,
    ) -> Result<(), Failure> {
        panic!("guard ran sync -All");
    }
    fn deploy(
        &self,
        _: &Context,
        _: &CaEnv,
        _: Target<'_>,
        _: Option<&str>,
        _: &dyn Say,
        _: &dyn Aws,
        _: &dyn Remote,
    ) -> Result<(), Failure> {
        panic!("guard ran deploy");
    }
    fn bootstrap(
        &self,
        _: &Context,
        _: &CaEnv,
        _: Target<'_>,
        _: &dyn Say,
        _: &dyn Aws,
        _: &dyn Remote,
    ) -> Result<(), Failure> {
        panic!("guard ran bootstrap");
    }
    fn provision(
        &self,
        _: &Context,
        _: &CaEnv,
        _: &str,
        _: &dyn Say,
        _: &dyn Aws,
        _: &dyn Remote,
    ) -> Result<(), Failure> {
        panic!("guard ran provision");
    }
}

/// A throwaway home under the temp folder, removed when it drops.
struct Home(tempfile::TempDir);

impl Home {
    fn new() -> Home {
        Home(tempfile::tempdir().expect("a throwaway home"))
    }
    fn profile(&self) -> PathBuf {
        let p = self.0.path().join("profile");
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn ctx(&self) -> Context {
        Context {
            home: self.profile(),
            local_app_data: None,
            system_root: self.0.path().join("systemroot"),
            nested: false,
        }
    }
}

/// What one dispatch of `guard` did.
struct Done {
    result: Result<Outcome, Failure>,
    seen: Vec<Seen>,
    printed: String,
}

impl Done {
    /// The program's exit code for this dispatch, as `run` maps it: an outcome's own, and 1 for a
    /// failure, which `go` has already printed.
    fn exit_code(&self) -> i32 {
        match &self.result {
            Ok(outcome) => outcome.exit_code(),
            Err(_) => 1,
        }
    }
}

/// Dispatches the command line `args` (which must parse to a run) with the stand-in guard run,
/// the clock fixed at `seconds` and `fraction_ms` past the epoch, and `env` as the program's
/// environment.
fn go(
    home: &Home,
    args: &[&str],
    env: &[(&str, &Path)],
    outcome: Result<GuardReport, Failure>,
    seconds: u64,
    fraction_ms: u64,
) -> Done {
    go_saying(home, args, env, outcome, &[], seconds, fraction_ms)
}

/// [`go`], with a run that says `warnings` through the `Say` it is given before it answers.
fn go_saying(
    home: &Home,
    args: &[&str],
    env: &[(&str, &Path)],
    outcome: Result<GuardReport, Failure>,
    warnings: &[&str],
    seconds: u64,
    fraction_ms: u64,
) -> Done {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let Parsed::Run(request) = parse(&args) else {
        panic!("the line is a run");
    };
    let capture = Capture::default();
    let say = Printer::new(
        Mode::Person,
        request.quiet,
        false,
        Box::new(capture.clone()),
    );
    let mut reach = Standin::new(&home.profile(), env, outcome);
    reach.warnings = warnings.iter().map(|w| w.to_string()).collect();
    let ctx = home.ctx();
    let clock = Fixed(UNIX_EPOCH + Duration::new(seconds, (fraction_ms * 1_000_000) as u32));
    let ca_env = CaEnv::default();
    let result = dispatch(
        &request,
        &Parts {
            ctx: &ctx,
            ca_env: &ca_env,
            say: &say,
            report: &say,
            aws: &NoAws,
            remote: &NoRemote,
            clock: &clock,
            reach: &reach,
        },
    );
    if let Err(failure) = &result {
        say.failure(failure);
    }
    Done {
        result,
        seen: reach.seen.take(),
        printed: capture.text(),
    }
}

fn clean() -> Result<GuardReport, Failure> {
    Ok(GuardReport {
        hosts: 3,
        running: 2,
        ..GuardReport::default()
    })
}

// ---------------------------------------------------------------------------------------------
// What the run is handed
// ---------------------------------------------------------------------------------------------

#[test]
fn the_run_gets_the_home_the_state_folder_and_the_clocks_time_to_the_whole_second() {
    let home = Home::new();
    let moved = home.0.path().join("moved state");
    let done = go(
        &home,
        &["guard"],
        &[("AGENTSPACE_STATE_DIR", moved.as_path())],
        clean(),
        1_700_000_000,
        999,
    );
    assert_eq!(done.result, Ok(Outcome::Done));
    assert_eq!(
        done.seen,
        [Seen {
            home: home.profile(),
            state_dir: moved,
            now: 1_700_000_000,
        }],
        "one run, at the clock's reading truncated, never rounded, to the second"
    );
}

#[test]
fn the_state_folder_is_agentspace_state_dir_when_it_is_set_and_not_empty() {
    let home = Home::new();
    let moved = home.0.path().join("somewhere else");
    let done = go(
        &home,
        &["guard"],
        &[("AGENTSPACE_STATE_DIR", moved.as_path())],
        clean(),
        5,
        0,
    );
    assert_eq!(done.seen.len(), 1);
    assert_eq!(done.seen[0].state_dir, moved);
}

#[test]
fn the_state_folder_is_the_homes_agentspace_folder_when_the_variable_is_unset() {
    let home = Home::new();
    let done = go(&home, &["guard"], &[], clean(), 5, 0);
    assert_eq!(done.seen.len(), 1);
    assert_eq!(done.seen[0].state_dir, home.profile().join(".agentspace"));
}

#[test]
fn the_state_folder_is_the_homes_agentspace_folder_when_the_variable_is_empty() {
    let home = Home::new();
    let done = go(
        &home,
        &["guard"],
        &[("AGENTSPACE_STATE_DIR", Path::new(""))],
        clean(),
        5,
        0,
    );
    assert_eq!(done.seen.len(), 1);
    assert_eq!(done.seen[0].state_dir, home.profile().join(".agentspace"));
}

#[test]
fn a_relative_state_folder_is_made_full() {
    // operator.md, Environment variables: a relative value is resolved against the working folder.
    let home = Home::new();
    let done = go(
        &home,
        &["guard"],
        &[("AGENTSPACE_STATE_DIR", Path::new("relative-state"))],
        clean(),
        5,
        0,
    );
    assert_eq!(done.seen.len(), 1);
    let dir = &done.seen[0].state_dir;
    assert!(dir.is_absolute(), "{dir:?}");
    assert!(dir.ends_with("relative-state"), "{dir:?}");
}

#[test]
fn the_state_folder_never_moves_the_homes_own_files() {
    // The registry's folder is the home's, whatever AGENTSPACE_STATE_DIR names: the run is handed
    // the home for `hosts.json`, `guard.lock` and `guard.log`, and the state folder for the leases.
    let home = Home::new();
    let moved = home.0.path().join("elsewhere");
    let done = go(
        &home,
        &["guard"],
        &[("AGENTSPACE_STATE_DIR", moved.as_path())],
        clean(),
        5,
        0,
    );
    assert_eq!(done.seen[0].home, home.profile());
    assert_ne!(
        done.seen[0].state_dir,
        done.seen[0].home.join(".agentspace")
    );
}

// ---------------------------------------------------------------------------------------------
// What it does with the run's answer
// ---------------------------------------------------------------------------------------------

#[test]
fn a_run_that_failed_no_host_is_success_and_prints_nothing_more() {
    let home = Home::new();
    let done = go(&home, &["guard"], &[], clean(), 5, 0);
    assert_eq!(done.result, Ok(Outcome::Done), "exit 0");
    assert_eq!(
        done.printed, "Hosts: 0; running 0, stopped 0, unexplained 0.\n",
        "only what the run printed through the Say it was given"
    );
    // A run that went to its end with hosts stopped and unexplained is success too.
    let report = GuardReport {
        hosts: 4,
        running: 3,
        stopped: 1,
        unexplained: 2,
        ..GuardReport::default()
    };
    let done = go(&home, &["guard"], &[], Ok(report), 5, 0);
    assert_eq!(done.result, Ok(Outcome::Done));
}

#[test]
fn a_run_that_found_guard_lock_held_is_success_and_prints_nothing() {
    // operator.md, guard, step 1: exit 0, nothing printed. The stand-in prints nothing for a held
    // lock, as the real run does not; the dispatch adds nothing of its own, and in particular
    // no count line and no closing line.
    let home = Home::new();
    for args in [&["guard", "-Quiet"][..], &["guard"][..]] {
        let held = GuardReport {
            lock_held: true,
            ..GuardReport::default()
        };
        let done = go(&home, args, &[], Ok(held), 5, 0);
        assert_eq!(done.result, Ok(Outcome::Done), "{args:?}: exit 0");
        assert_eq!(done.printed, "", "{args:?}: nothing printed");
        assert_eq!(done.seen.len(), 1, "{args:?}: the run was reached");
    }
}

#[test]
fn failed_hosts_are_the_count_line_and_a_failure() {
    // operator.md, guard, step 8: `agentspace: <n> of <m> hosts failed.`, `-All`'s line, where
    // `<m>` is the registry's records.
    let home = Home::new();
    for (failed, hosts, line) in [
        (1, 1, "agentspace: 1 of 1 hosts failed."),
        (1, 4, "agentspace: 1 of 4 hosts failed."),
        (3, 3, "agentspace: 3 of 3 hosts failed."),
        (12, 40, "agentspace: 12 of 40 hosts failed."),
    ] {
        let report = GuardReport {
            hosts,
            running: hosts,
            failed,
            ..GuardReport::default()
        };
        let done = go(&home, &["guard"], &[], Ok(report), 5, 0);
        let failure = done.result.expect_err("a failed host fails the command");
        assert_eq!(failure.message, line);
        assert_eq!(failure.caller, CallerFailure::Same);
        assert_eq!(
            done.printed,
            format!("Hosts: 0; running 0, stopped 0, unexplained 0.\n{line}\n"),
            "the run's closing line, then the count line"
        );
    }
}

#[test]
fn the_count_line_prints_whatever_quiet_says() {
    let home = Home::new();
    let report = GuardReport {
        hosts: 2,
        running: 1,
        failed: 1,
        ..GuardReport::default()
    };
    let done = go(&home, &["guard", "-Quiet"], &[], Ok(report), 5, 0);
    assert!(done.result.is_err());
    assert_eq!(
        done.printed, "agentspace: 1 of 2 hosts failed.\n",
        "the run's progress line is suppressed by -Quiet, the count line is not"
    );
}

#[test]
fn a_failure_that_ends_the_run_is_printed_unchanged() {
    let home = Home::new();
    for failure in [
        Failure {
            message: "agentspace: The lease state file C:\\somewhere\\leases.json cannot be read: the file does not exist.".to_string(),
            caller: CallerFailure::Same,
        },
        Failure {
            message: "agentspace: Cannot take C:\\somewhere\\guard.lock: access denied.".to_string(),
            caller: CallerFailure::CouldNotReach,
        },
        Failure {
            message: "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry.".to_string(),
            caller: CallerFailure::Same,
        },
    ] {
        let done = go(&home, &["guard"], &[], Err(failure.clone()), 5, 0);
        assert_eq!(done.result, Err(failure.clone()), "the failure passes through whole");
        assert_eq!(
            done.printed,
            format!(
                "Hosts: 0; running 0, stopped 0, unexplained 0.\n{}\n",
                failure.message
            )
        );
    }
}

// ---------------------------------------------------------------------------------------------
// -Quiet reaches the run's Say; the dispatch does nothing of its own
// ---------------------------------------------------------------------------------------------

#[test]
fn quiet_is_the_runs_own_say_so_its_progress_lines_are_suppressed() {
    let home = Home::new();
    let done = go(&home, &["guard", "-Quiet"], &[], clean(), 5, 0);
    assert_eq!(done.result, Ok(Outcome::Done));
    assert_eq!(done.printed, "", "-Quiet suppresses the progress lines");
    let done = go(&home, &["-Quiet", "guard"], &[], clean(), 5, 0);
    assert_eq!(done.printed, "");
    let done = go(&home, &["guard"], &[], clean(), 5, 0);
    assert_eq!(
        done.printed, "Hosts: 0; running 0, stopped 0, unexplained 0.\n",
        "without -Quiet they print"
    );
}

#[test]
fn guard_is_not_all_it_prints_no_host_line_and_reads_no_registry_of_its_own() {
    // The home's host registry is not valid: a command that read it before its run would fail with
    // the registry's message. `guard` reads it itself (its step 2), inside the run.
    let home = Home::new();
    let state = home.profile().join(".agentspace");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("hosts.json"), b"not json").unwrap();
    let done = go(&home, &["guard"], &[], clean(), 5, 0);
    assert_eq!(done.result, Ok(Outcome::Done));
    assert_eq!(done.seen.len(), 1, "the run was reached");
    assert!(!done.printed.contains("Host      "), "{:?}", done.printed);
    // And the dispatch wrote nothing in the home: no `guard.lock`, no `guard.log`.
    let mut names: Vec<String> = std::fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["hosts.json"]);
}

#[test]
fn the_dispatch_makes_no_aws_call_starts_no_ssh_and_waits_for_nothing() {
    // `NoAws`, `NoRemote`, `Fixed::sleep` and every other `Standin` method panic when reached, so
    // each run above shows it; this one names the claim, with a run that fails.
    let home = Home::new();
    let report = GuardReport {
        hosts: 1,
        failed: 1,
        ..GuardReport::default()
    };
    let done = go(&home, &["guard"], &[], Ok(report), 5, 0);
    assert!(done.result.is_err());
}

#[test]
fn each_dispatch_is_one_run() {
    let home = Home::new();
    let done = go(&home, &["guard", "-GiB", "0"], &[], clean(), 5, 0);
    assert_eq!(done.seen.len(), 1, "one run per dispatch, -GiB ignored");
}

// ---------------------------------------------------------------------------------------------
// The guard's failure channel: `warning` prints whatever -Quiet says (architect, 2026-11-22)
// ---------------------------------------------------------------------------------------------

const HOST_FAILED: &str =
    "agentspace: Host 'example': Could not stop i-0123456789abcdef0: it was refused.";
const LOG_FAILED: &str =
    "agentspace: Cannot write the guard's log C:\\somewhere\\guard.log: access denied.";
const CLOSING: &str = "Hosts: 0; running 0, stopped 0, unexplained 0.";

#[test]
fn a_failure_the_run_says_through_warning_prints_under_quiet_and_its_progress_does_not() {
    let home = Home::new();
    let report = GuardReport {
        hosts: 2,
        running: 1,
        failed: 1,
        ..GuardReport::default()
    };
    let done = go_saying(
        &home,
        &["guard", "-Quiet"],
        &[],
        Ok(report.clone()),
        &[HOST_FAILED],
        5,
        0,
    );
    assert!(done.result.is_err());
    assert_eq!(
        done.printed,
        format!("{HOST_FAILED}\nagentspace: 1 of 2 hosts failed.\n"),
        "-Quiet: the failure line and the count line print, the closing line does not"
    );
    let done = go_saying(&home, &["guard"], &[], Ok(report), &[HOST_FAILED], 5, 0);
    assert_eq!(
        done.printed,
        format!("{HOST_FAILED}\n{CLOSING}\nagentspace: 1 of 2 hosts failed.\n"),
        "Without -Quiet: the failure line, the closing line, the count line"
    );
}

// ---------------------------------------------------------------------------------------------
// `log_dropped` (architect, 2026-11-22, for v0.08-T03): exit 1, and the count line only when
// hosts failed
// ---------------------------------------------------------------------------------------------

#[test]
fn a_dropped_log_line_with_no_failed_host_exits_1_and_prints_nothing_more() {
    let home = Home::new();
    let report = GuardReport {
        hosts: 2,
        running: 1,
        log_dropped: true,
        ..GuardReport::default()
    };
    let done = go_saying(
        &home,
        &["guard"],
        &[],
        Ok(report.clone()),
        &[LOG_FAILED],
        5,
        0,
    );
    assert_eq!(
        done.result,
        Ok(Outcome::FailedAfterSaying),
        "A log line dropped, no host failed"
    );
    assert_eq!(done.exit_code(), 1, "Exit 1");
    assert_eq!(
        done.printed,
        format!("{LOG_FAILED}\n{CLOSING}\n"),
        "What the run said through warning and progress, and no count line of the operator's own"
    );
    let done = go_saying(
        &home,
        &["guard", "-Quiet"],
        &[],
        Ok(report),
        &[LOG_FAILED],
        5,
        0,
    );
    assert_eq!(done.result, Ok(Outcome::FailedAfterSaying));
    assert_eq!(done.exit_code(), 1, "Exit 1 under -Quiet too");
    assert_eq!(done.printed, format!("{LOG_FAILED}\n"));
}

#[test]
fn failed_hosts_and_a_dropped_log_line_print_the_count_line_once_and_exit_1() {
    let home = Home::new();
    let report = GuardReport {
        hosts: 3,
        running: 2,
        failed: 2,
        log_dropped: true,
        ..GuardReport::default()
    };
    let done = go_saying(
        &home,
        &["guard", "-Quiet"],
        &[],
        Ok(report),
        &[LOG_FAILED],
        5,
        0,
    );
    assert_eq!(done.exit_code(), 1, "Exit 1");
    let failure = done.result.expect_err("The count line is a failure");
    assert_eq!(failure.message, "agentspace: 2 of 3 hosts failed.");
    assert_eq!(
        done.printed,
        format!("{LOG_FAILED}\nagentspace: 2 of 3 hosts failed.\n"),
        "The count line once"
    );
}

#[test]
fn only_a_failed_host_a_dropped_line_or_a_failure_is_exit_1() {
    let home = Home::new();
    // Neither: exit 0. The lock held: exit 0.
    let neither = go(&home, &["guard"], &[], clean(), 5, 0);
    assert_eq!(neither.result, Ok(Outcome::Done));
    assert_eq!(neither.exit_code(), 0);
    let held = GuardReport {
        lock_held: true,
        ..GuardReport::default()
    };
    let held = go(&home, &["guard"], &[], Ok(held), 5, 0);
    assert_eq!(held.result, Ok(Outcome::Done));
    assert_eq!(held.exit_code(), 0);
    // Each of the others: exit 1.
    let failed = GuardReport {
        hosts: 1,
        failed: 1,
        ..GuardReport::default()
    };
    let dropped = GuardReport {
        hosts: 1,
        log_dropped: true,
        ..GuardReport::default()
    };
    for report in [failed, dropped] {
        let done = go(&home, &["guard"], &[], Ok(report), 5, 0);
        assert_eq!(done.exit_code(), 1);
    }
    let failure = Failure {
        message: "agentspace: A failure that ended the run.".to_string(),
        caller: CallerFailure::Same,
    };
    assert_eq!(
        go(&home, &["guard"], &[], Err(failure), 5, 0).exit_code(),
        1
    );
}
