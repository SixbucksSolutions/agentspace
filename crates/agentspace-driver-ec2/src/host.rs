//! Where a lease is placed, by its project's host, and the start of a provisioning run
//! (docs/driver.md, "Where it places a lease", "Starting a provisioning run" and "check_pending";
//! docs/state.md, "The host registry", "The project list" and "The provisioning run's start"; D48,
//! D54, D55).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **Place MUST decide support before it reads any file**, and an unsupported request reads
//!   neither the host registry nor the project list.
//! - **The registry MUST be read under `hosts.lock`, and the lock MUST be released before a run is
//!   started.** The reads that decide come first, then the program, then the one write, so a
//!   failure before the write leaves the registry as it was.
//! - **The project list MUST be read only for a project with no record.**
//! - **A `pending` answer from place MUST check the CA first**, and `check_pending` runs no CA check.
//! - **The driver MUST NOT change the `state` of a `ready` record.**
//! - **A run MUST NOT be started while the host's `provision.lock` is held, or when the record's
//!   `last_attempt_at` is under 5 minutes old.** The run lock is only tried, never waited for.
//! - **`check_pending` MUST NOT wait for `hosts.lock`, write the lease state file or take its lock,
//!   and MUST end within 20 seconds.**
//! - **The driver MUST NOT open or make `provision.log` or `provision.lock`**; the host's folder is
//!   the one thing in it the driver makes.
//! - **From v0.09, place MUST decide a `ready` record by D66's table** (driver.md, "On a `ready`
//!   host"): the size asked for, the record's `instance_type` and the project's leases, read from
//!   the lease state file read-only, under the lease state lock its caller holds, which the driver
//!   MUST NOT take. Place makes no AWS call.
//! - **Place MUST decide from `agentspace_hosts::read_for_placing`, and MUST start every write of
//!   `hosts.json` from a fresh plain `read` under the same `hosts.lock`** (pins P31 and P38), so
//!   that the `m9g.xlarge` a missing file implies is never written.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use agentspace_credential::{ca_failure_message, check_ca};
use agentspace_detach::Detach;
use agentspace_driver_api::{Context, Failure, LeaseRef, PlaceRequest, Placed, Reason};
use agentspace_hosts::{self as hosts, Host, HostState, Hosts, HostsError, HostsLock};
use agentspace_state::{Record as Lease, format_time, is_live, now, parse_time};

use crate::ops::{self, HostKey};
use crate::reasons::reasons;
use crate::sizes::{SIZES, instance_type_for, meets, resize_target};
use crate::text::{
    OPERATOR_NAMED_MISSING, OPERATOR_NO_INTERPRETER, OPERATOR_NOT_BESIDE, failure,
    host_lookup_failed, no_capacity, no_capacity_at_size,
};

/// The detached start the driver holds, shared with the thread that bounds a start.
pub type SharedDetach = Arc<dyn Detach + Send + Sync>;

/// A run is due only when the last one started at least this long before now (D48).
const RUN_EVERY_SECONDS: i64 = 5 * 60;

/// How long place waits for a start to return. `WmiDetach` bounds itself at 10 seconds; this keeps
/// place well under the lease state lock's 30-second wait whatever the start does.
const PLACE_START_BOUND: Duration = Duration::from_secs(15);

/// How long `check_pending` takes at most, the start included: under driver.md's 20 seconds.
const CHECK_BOUND: Duration = Duration::from_secs(18);

/// The operator program's file name beside the CLI (driver.md, The operator program).
const OPERATOR_FILE: &str = "agentspace-operator.exe";

/// Whether the project already has a record, or is listed and gets one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Record {
    Existing,
    New,
}

/// A run to start: the program and its arguments, the command line driver.md gives.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Run {
    program: PathBuf,
    args: Vec<String>,
}

fn lookup(why: &str) -> Failure {
    failure(host_lookup_failed(why), Vec::new())
}

/// The `<why>` of a failure of the host registry or its lock, naming the file by what it is.
pub fn registry_why(e: HostsError) -> &'static str {
    match e {
        HostsError::Busy => "agentspace's host records are in use by another command",
        HostsError::CannotLock(_) => "agentspace's host records could not be locked",
        HostsError::Unreadable(_) => "agentspace's host records could not be read",
        HostsError::NotValid(_) => "agentspace's host records are not valid",
        HostsError::Unwritable(_) => "agentspace's host records could not be written",
        HostsError::BadProject => "the project's name is not in its normalized form",
        HostsError::CannotLockRun(_) => "the provisioning run's lock could not be checked",
        HostsError::CannotLockGuard(_) => "the guard's lock could not be checked",
    }
}

/// The `<why>` of a failure to read the project list.
fn projects_why(e: HostsError) -> &'static str {
    match e {
        HostsError::Unreadable(_) => "agentspace's project list could not be read",
        HostsError::NotValid(_) => "agentspace's project list is not valid",
        other => registry_why(other),
    }
}

/// What place answers for a supported request whose project's record is `ready` (driver.md, "On a
/// `ready` host"), before anything is issued or written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnReady {
    /// Rows 3 and 4: placed on the host at once.
    Ready,
    /// Rows 1, 2 and 6: `pending`, and a run started when one is due.
    Pending,
    /// Row 5: `unsatisfiable`, with D66's `no_capacity`.
    Refused,
}

/// The `<why>` of a lease state file that place cannot read, or that does not validate (pin P19).
const LEASES_WHY: &str = "agentspace's lease records could not be read";

/// Whether a live `ready` lease of `project` holds its host as of `now` (driver.md, "On a `ready`
/// host"; state.md, "Live leases and the lease horizon"; D56, D66): a record stored `ready`, of the
/// project, compared exactly, whose `expires_at` is after `now`. A lease that is only `pending`
/// holds no host's size, and an expired, `released` or `unsatisfiable` one holds nothing.
pub(crate) fn holds_host(leases: &[Lease], project: &str, now: i64) -> bool {
    leases
        .iter()
        .any(|l| l.state == "ready" && l.project == project && is_live(l, now))
}

/// Whether the map holds `instance_type`: a type outside it meets no size (D67; pin P14).
fn is_mapped(instance_type: &str) -> bool {
    SIZES
        .iter()
        .any(|s| instance_type_for(s) == Some(instance_type))
}

/// D66's table, as driver.md's "On a `ready` host" gives it: the first row that applies, and
/// nothing else. `size` is the request's normalized size, one of `SIZES`; `instance_type` is the
/// record's; `leases` are the records of the lease state file, read at `now`.
fn on_ready(
    size: &str,
    instance_type: Option<&str>,
    leases: &[Lease],
    project: &str,
    now: i64,
) -> OnReady {
    // Row 1: a `pending` lease a run would place is `resize_target`'s selection (pins P2, P18 and
    // P20), so the project's every new lease is `pending` while one exists.
    if resize_target(leases, project).is_some() {
        return OnReady::Pending;
    }
    // Row 2: no type, or one the map does not hold, whatever other leases hold the host.
    let Some(have) = instance_type.filter(|t| is_mapped(t)) else {
        return OnReady::Pending;
    };
    // Row 3: exactly, whatever other leases hold the host.
    if instance_type_for(size) == Some(have) {
        return OnReady::Ready;
    }
    match (meets(have, size), holds_host(leases, project, now)) {
        // Row 4: larger, and another live `ready` lease holds the host.
        (true, true) => OnReady::Ready,
        // Row 5: smaller, and another live `ready` lease holds the host.
        (false, true) => OnReady::Refused,
        // Row 6: larger or smaller, and nothing holds the host.
        (_, false) => OnReady::Pending,
    }
}

/// Decides a supported request on a `ready` record: reads the lease state file of the context's
/// state folder, read-only and taking no lock, since the CLI, place's caller, holds the lease
/// state lock (driver.md, "On a `ready` host", the last bullet; Lock order). A missing file is no
/// leases; one that cannot be read or does not validate fails the place with `internal`, before
/// anything is written or started.
fn decide_on_ready(ctx: &Context, req: &PlaceRequest, host: &Host) -> Result<OnReady, Failure> {
    let file = agentspace_state::read(&ctx.state_dir).map_err(|_| lookup(LEASES_WHY))?;
    Ok(on_ready(
        req.size,
        host.instance_type.as_deref(),
        &file.leases,
        req.project,
        now(),
    ))
}

/// Row 5's answer: `unsatisfiable`, with one reason, `no_capacity`, and D66's message. It changes
/// nothing.
fn refused(req: &PlaceRequest) -> Placed {
    Placed::Unsatisfiable {
        reasons: vec![Reason {
            code: "no_capacity",
            message: no_capacity_at_size(req.project, req.size),
        }],
    }
}

/// Which host key lines a lease on `host` carries in its alias (D47).
fn host_key<'a>(host: &Host, project: &'a str) -> HostKey<'a> {
    if hosts::is_box(host) {
        HostKey::Box
    } else {
        HostKey::Other(project)
    }
}

/// Place, by the project's host (driver.md, Where it places a lease).
pub fn place(ctx: &Context, req: &PlaceRequest, detach: &SharedDetach) -> Result<Placed, Failure> {
    let unmet = reasons(req.platform, req.profile, req.size);
    if !unmet.is_empty() {
        return Ok(Placed::Unsatisfiable { reasons: unmet });
    }
    if !hosts::is_normalized_project(req.project) {
        return Err(lookup(registry_why(HostsError::BadProject)));
    }

    // Architect's to confirm: with no `hosts.json`, swimcoach's lease goes to the box as in v0.06,
    // without taking `hosts.lock`, so a box lease makes no file of the registry's. The missing file
    // reads as the seed, whose box record is `ready`, and a writer that makes the file meanwhile
    // keeps that record first and `ready` (state.md, The seed; nothing moves a record out of
    // `ready`), so the answer is the one the lock would give. From v0.09 the record is decided by
    // D66's table, as `read_for_placing` gives it, and only an answer that writes nothing is given
    // here: a `pending` one goes on to the locked path, which decides again and records the run.
    if req.project == hosts::BOX_PROJECT && registry_is_missing(&ctx.home) {
        let placing = hosts::read_for_placing(&ctx.home).map_err(|e| lookup(registry_why(e)))?;
        if let Some(host) =
            hosts::host_for(&placing, req.project).filter(|h| h.state == HostState::Ready)
        {
            match decide_on_ready(ctx, req, host)? {
                OnReady::Ready => return ops::issue_ready(ctx, req, host_key(host, req.project)),
                OnReady::Refused => return Ok(refused(req)),
                OnReady::Pending => {}
            }
        }
    }

    // Step 1: the reads that decide, under `hosts.lock`, waited for up to 30 seconds. The decision
    // reads the registry as a placer does, a missing file implying the box's `m9g.xlarge` (pin
    // P2c); that value MUST NOT be the start of a write (pin P31).
    let held = hosts::lock(&ctx.home).map_err(|e| lookup(registry_why(e)))?;
    let placing = hosts::read_for_placing(&ctx.home).map_err(|e| lookup(registry_why(e)))?;
    let record = match hosts::host_for(&placing, req.project) {
        Some(host) if host.state == HostState::Ready => match decide_on_ready(ctx, req, host)? {
            OnReady::Ready => {
                // A `ready` answer writes nothing to the registry.
                let key = host_key(host, req.project);
                drop(held);
                return ops::issue_ready(ctx, req, key);
            }
            OnReady::Refused => return Ok(refused(req)),
            OnReady::Pending => Record::Existing,
        },
        Some(_) => Record::Existing,
        None => {
            let list = hosts::read_projects(&ctx.home).map_err(|e| lookup(projects_why(e)))?;
            if !hosts::is_listed(&list, req.project) {
                return Ok(Placed::Unsatisfiable {
                    reasons: vec![Reason {
                        code: "no_capacity",
                        message: no_capacity(req.project),
                    }],
                });
            }
            Record::New
        }
    };

    // A `pending` answer checks the CA first, and reads only (pin P14.1).
    check_ca(&ctx.home).map_err(|f| failure(ca_failure_message(f).to_string(), Vec::new()))?;

    // Steps 2 to 4, then step 5: the lock is released before any run is started. The write starts
    // from a fresh plain read under the same hold, so a missing file is written as the seed
    // document, whose box record has no `instance_type` (pins P31 and P38).
    let mut registry = hosts::read(&ctx.home).map_err(|e| lookup(registry_why(e)))?;
    let run = record_run(ctx, req.project, &mut registry, record, &held)?;
    drop(held);
    if let Some(run) = run {
        start_run(ctx, req.project, run, detach, PLACE_START_BOUND);
    }
    Ok(Placed::Pending)
}

/// Whether nothing is at `hosts.json`, with every folder above it a folder or missing: the case in
/// which the registry reads as the seed. Anything else, a folder at its path included, is left to
/// the locked read, which reports it.
fn registry_is_missing(home: &Path) -> bool {
    let agentspace = hosts::agentspace_dir(home);
    let folder_ok = agentspace
        .ancestors()
        .find_map(|a| fs::metadata(a).ok())
        .is_some_and(|m| m.is_dir());
    folder_ok
        && matches!(
            fs::symlink_metadata(hosts::hosts_path(home)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound
        )
}

/// `check_pending` (driver.md, check_pending; Starting a provisioning run, "From `check_pending`").
pub fn check_pending(
    ctx: &Context,
    lease: &LeaseRef,
    detach: &SharedDetach,
) -> Result<(), Failure> {
    let begun = Instant::now();
    if !hosts::is_normalized_project(lease.project) {
        return Err(lookup(registry_why(HostsError::BadProject)));
    }
    // One try, never a wait: a held lock starts nothing, and the next `get` checks again.
    let Some(held) = hosts::try_lock(&ctx.home).map_err(|e| lookup(registry_why(e)))? else {
        return Ok(());
    };
    let mut registry = hosts::read(&ctx.home).map_err(|e| lookup(registry_why(e)))?;
    // A record in any state is treated by steps 2 to 5; a `ready` one keeps its `state` (pin P3).
    let record = if hosts::host_for(&registry, lease.project).is_some() {
        Record::Existing
    } else {
        let list = hosts::read_projects(&ctx.home).map_err(|e| lookup(projects_why(e)))?;
        if !hosts::is_listed(&list, lease.project) {
            // A registry changed by hand since the `create`: nothing at all, and `get` never makes
            // a lease `unsatisfiable`.
            return Ok(());
        }
        Record::New
    };
    let run = record_run(ctx, lease.project, &mut registry, record, &held)?;
    drop(held);
    if let Some(run) = run {
        start_run(
            ctx,
            lease.project,
            run,
            detach,
            CHECK_BOUND.saturating_sub(begun.elapsed()),
        );
    }
    Ok(())
}

/// Steps 2 to 4 of "When the driver starts a run": decides whether a run is due, by reads alone,
/// finds the program only when one is, and writes the registry once, and only what applies.
/// Returns the run to start once `hosts.lock` is released.
fn record_run(
    ctx: &Context,
    project: &str,
    registry: &mut Hosts,
    record: Record,
    held: &HostsLock,
) -> Result<Option<Run>, Failure> {
    let now = now();
    let last = match record {
        Record::New => None,
        Record::Existing => {
            hosts::host_for(registry, project).and_then(|h| h.last_attempt_at.clone())
        }
    };
    let due = run_is_due(&ctx.home, project, last.as_deref(), now);
    // Step 3: a program that cannot be found fails before anything is written.
    let run = if due {
        Some(operator_run(ctx, project)?)
    } else {
        None
    };

    // Step 4.
    let stamp = format_time(now);
    let changed = match record {
        Record::New => {
            registry.hosts.push(Host {
                project: project.to_string(),
                stack: hosts::stack_name(project),
                state: HostState::Creating,
                instance_id: None,
                instance_type: None,
                last_attempt_at: due.then(|| stamp.clone()),
            });
            true
        }
        Record::Existing if due => {
            if let Some(host) = registry.hosts.iter_mut().find(|h| h.project == project) {
                host.last_attempt_at = Some(stamp);
                // A `ready` record never has its `state` changed by the driver (D48).
                if host.state != HostState::Ready {
                    host.state = HostState::Creating;
                }
            }
            true
        }
        Record::Existing => false,
    };
    if changed {
        hosts::write(&ctx.home, registry, held).map_err(|e| lookup(registry_why(e)))?;
    }
    Ok(run)
}

/// Step 2: a run is due when the record's `last_attempt_at` is absent or at least 5 minutes
/// before now, and no run holds the host's `provision.lock`. The lock is tried once and let go at
/// once; a missing folder or lock file is a lock nobody holds, and the try makes neither.
fn run_is_due(home: &Path, project: &str, last: Option<&str>, now: i64) -> bool {
    let old_enough = last
        .and_then(parse_time)
        .is_none_or(|at| now.saturating_sub(at) >= RUN_EVERY_SECONDS);
    if !old_enough {
        return false;
    }
    // QUESTION: driver.md does not say what place does when the run lock cannot be opened for a
    // reason other than its being held (state.md: "not a free lock ... starts or runs nothing").
    // Interim: no run is due, so nothing is started and the lease is `pending`.
    matches!(hosts::run_is_running(home, project), Ok(false))
}

/// Whether `program` is agentspace's own operator beside the CLI, as the CLI resolves it with
/// `AGENTSPACE_PROVISION_CMD` unset or empty.
fn is_default_operator(ctx: &Context, program: &Path) -> bool {
    let named = program
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(OPERATOR_FILE));
    let folder = |p: &Path| p.parent().map(|f| f.to_string_lossy().to_lowercase());
    named && folder(program).is_some() && folder(program) == folder(&ctx.own_exe)
}

/// A `.cmd` or `.bat` program, its extension compared without regard to case.
fn needs_interpreter(program: &Path) -> bool {
    program.extension().is_some_and(|e| {
        let e = e.to_string_lossy();
        e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat")
    })
}

/// Step 3 and the operator program (driver.md, The operator program): the run's command line, or
/// the failure of a program that cannot be found.
fn operator_run(ctx: &Context, project: &str) -> Result<Run, Failure> {
    let missing = |message: &str| failure(message.to_string(), Vec::new());
    // Architect's to confirm: a context with no operator program (the CLI could not make the path
    // full) is read as agentspace's own operator not being installed beside the CLI.
    let Some(program) = ctx.operator_program.as_deref() else {
        return Err(missing(OPERATOR_NOT_BESIDE));
    };
    if !program.is_file() {
        // Architect's to confirm: the context does not say whether the path came from
        // AGENTSPACE_PROVISION_CMD, so the default's place beside the CLI chooses the message.
        return Err(missing(if is_default_operator(ctx, program) {
            OPERATOR_NOT_BESIDE
        } else {
            OPERATOR_NAMED_MISSING
        }));
    }
    let tail = ["provision", "-Host", project].map(str::to_string);
    if !needs_interpreter(program) {
        return Ok(Run {
            program: program.to_path_buf(),
            args: tail.to_vec(),
        });
    }
    let root = ctx
        .system_root
        .as_deref()
        .map(|r| r.trim_end_matches(['\\', '/']))
        .filter(|r| !r.is_empty());
    let Some(root) = root else {
        return Err(missing(OPERATOR_NO_INTERPRETER));
    };
    // The program is an argument of cmd.exe, which a command line carries only as text.
    let Some(text) = program.to_str() else {
        return Err(missing(OPERATOR_NAMED_MISSING));
    };
    let mut args = vec!["/d".to_string(), "/c".to_string(), text.to_string()];
    args.extend(tail);
    Ok(Run {
        program: PathBuf::from(format!("{root}\\System32\\cmd.exe")),
        args,
    })
}

fn full(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The run's four variables (driver.md, The detached start; state.md, The provisioning run's
/// start). An empty value makes the variable absent from the run's environment, so
/// `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR` reach the run if and only if the CLI had them
/// set and not empty, which the context's `state_dir_env` and `ssh_dir_env` say (the T03 pin as
/// amended, ce0967f), each made full.
fn run_environment(ctx: &Context, home: &Path) -> Vec<(&'static str, OsString)> {
    let folder = |value: &Option<PathBuf>| {
        value
            .as_deref()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| full(p).into_os_string())
            .unwrap_or_default()
    };
    let proxy = ctx
        .proxy_override
        .as_deref()
        .filter(|c| !c.is_empty())
        .map(OsString::from)
        .unwrap_or_default();
    vec![
        ("USERPROFILE", home.as_os_str().to_os_string()),
        ("AGENTSPACE_STATE_DIR", folder(&ctx.state_dir_env)),
        ("AGENTSPACE_SSH_DIR", folder(&ctx.ssh_dir_env)),
        // A command line, passed exactly as the CLI had it, never made full.
        ("AGENTSPACE_PROXY_COMMAND", proxy),
    ]
}

/// Step 5: makes the host's folder when it is missing, then starts the run detached, in the home
/// folder, and waits for the start, never for the run, at most `bound`. A start that fails, or
/// does not return within `bound`, changes nothing more and fails nothing.
///
/// The start MUST always be begun once step 4 has written `last_attempt_at`, or the record would
/// say a run was started when none was, and none would start for 5 minutes (driver.md, a start
/// counts as made because it was begun). So a `bound` already used up waits a floor of one second
/// rather than skipping the start; check_pending's whole call still ends within its 20 seconds.
fn start_run(ctx: &Context, project: &str, run: Run, detach: &SharedDetach, bound: Duration) {
    let bound = bound.max(Duration::from_secs(1));
    let home = full(&ctx.home);
    // A failure to make the folder is a start that fails.
    if fs::create_dir_all(hosts::host_dir(&home, project)).is_err() {
        return;
    }
    let env = run_environment(ctx, &home);
    let detach = Arc::clone(detach);
    let (answer, receive) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("agentspace-driver-start".to_string())
        .spawn(move || {
            let args: Vec<&str> = run.args.iter().map(String::as_str).collect();
            let env: Vec<(&str, &OsStr)> = env.iter().map(|(n, v)| (*n, v.as_os_str())).collect();
            // Nobody reads the answer of a start that outlived its bound.
            let _ = answer.send(detach.start(&run.program, &args, &home, &env));
        });
    if spawned.is_ok() {
        // The lease is `pending` whatever the start answers.
        let _ = receive.recv_timeout(bound);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// One start a fake detach was asked for. The environment is kept by name, and a test
    /// compares values it set itself.
    #[derive(Clone)]
    struct Started {
        program: PathBuf,
        args: Vec<String>,
        cwd: PathBuf,
        env: Vec<(String, OsString)>,
    }

    #[derive(Default)]
    struct Fake {
        starts: Mutex<Vec<Started>>,
        refuse: bool,
        hang: Option<Duration>,
    }

    impl Detach for Fake {
        fn start(
            &self,
            program: &Path,
            args: &[&str],
            cwd: &Path,
            env: &[(&str, &OsStr)],
        ) -> Result<(), String> {
            if let Some(d) = self.hang {
                std::thread::sleep(d);
            }
            self.starts.lock().unwrap().push(Started {
                program: program.to_path_buf(),
                args: args.iter().map(|a| a.to_string()).collect(),
                cwd: cwd.to_path_buf(),
                env: env
                    .iter()
                    .map(|(n, v)| (n.to_string(), v.to_os_string()))
                    .collect(),
            });
            if self.refuse {
                Err("Refused.".to_string())
            } else {
                Ok(())
            }
        }
    }

    struct Fx {
        _tmp: tempfile::TempDir,
        ctx: Context,
        fake: Arc<Fake>,
        detach: SharedDetach,
    }

    fn fx_with(fake: Fake) -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let home = root.join("home");
        let bin = root.join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin).unwrap();
        let operator = bin.join(OPERATOR_FILE);
        fs::write(&operator, b"").unwrap();
        let ctx = Context {
            home: home.clone(),
            ssh_dir: home.join(".ssh"),
            proxy_override: None,
            system_root: Some("C:\\Windows".to_string()),
            own_exe: bin.join("agentspace.exe"),
            state_dir: home.join(".agentspace"),
            operator_program: Some(operator),
            state_dir_env: None,
            ssh_dir_env: None,
        };
        let fake = Arc::new(fake);
        let detach: SharedDetach = fake.clone();
        Fx {
            _tmp: tmp,
            ctx,
            fake,
            detach,
        }
    }

    fn fx() -> Fx {
        fx_with(Fake::default())
    }

    impl Fx {
        fn starts(&self) -> Vec<Started> {
            self.fake.starts.lock().unwrap().clone()
        }
        fn write(&self, name: &str, text: &str) {
            let dir = self.ctx.home.join(".agentspace");
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(name), text).unwrap();
        }
        fn list(&self, project: &str) {
            self.write(
                "projects.json",
                &format!(
                    r#"{{"version":0,"projects":[{{"name":"{project}","approved":"2026-11-20"}}]}}"#
                ),
            );
        }
        fn registry(&self) -> Option<Vec<u8>> {
            fs::read(self.ctx.home.join(".agentspace").join("hosts.json")).ok()
        }
        fn host(&self, project: &str) -> Option<Host> {
            hosts::host_for(&hosts::read(&self.ctx.home).unwrap(), project).cloned()
        }
        fn seed_with(&self, record: &str) {
            self.write(
                "hosts.json",
                &format!(
                    r#"{{"version":0,"hosts":[{{"project":"swimcoach","stack":"agentbox","state":"ready"}},{record}]}}"#
                ),
            );
        }
        fn check(&self, project: &str) -> Result<(), Failure> {
            let lease = LeaseRef {
                id: "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90",
                project,
            };
            check_pending(&self.ctx, &lease, &self.detach)
        }
        fn place(&self, project: &str) -> Result<Placed, Failure> {
            place(&self.ctx, &request(project), &self.detach)
        }
    }

    fn request(project: &str) -> PlaceRequest<'_> {
        PlaceRequest {
            id: "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90",
            project,
            platform: "linux/arm64",
            profile: "agent-linux",
            size: "m",
            ttl_seconds: 3600,
        }
    }

    fn ago(seconds: i64) -> String {
        format_time(now() - seconds)
    }

    #[test]
    fn an_unsupported_request_answers_before_any_file_is_read_or_made() {
        let f = fx();
        f.write("hosts.json", "not json");
        let mut r = request("other");
        r.platform = "windows/amd64";
        r.size = "xxl";
        let Placed::Unsatisfiable { reasons } = place(&f.ctx, &r, &f.detach).unwrap() else {
            panic!("Not unsatisfiable");
        };
        let codes: Vec<&str> = reasons.iter().map(|r| r.code).collect();
        assert_eq!(codes, ["unsupported_platform", "unsupported_size"]);
        assert!(!f.ctx.home.join(".agentspace").join("hosts.lock").exists());
        assert!(f.starts().is_empty());
    }

    #[test]
    fn a_project_with_no_host_and_not_listed_is_no_capacity_and_writes_nothing() {
        let f = fx();
        let Placed::Unsatisfiable { reasons } = f.place("other").unwrap() else {
            panic!("Not unsatisfiable");
        };
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].code, "no_capacity");
        assert_eq!(
            reasons[0].message,
            "No capacity can be found for project 'other': it has no host and is not on agentspace's project list."
        );
        assert_eq!(f.registry(), None);
        assert!(f.starts().is_empty());
        assert!(!f.ctx.home.join(".agentspace").join("leases").exists());
    }

    #[test]
    fn a_registry_or_list_that_is_not_valid_fails_the_place_and_writes_nothing() {
        let f = fx();
        f.write("hosts.json", "{}");
        let e = f.place("other").unwrap_err();
        assert_eq!(
            e.message,
            "The lease's host could not be looked up: agentspace's host records are not valid. Nothing was recorded."
        );
        let f = fx();
        f.write("projects.json", "{}");
        let e = f.place("other").unwrap_err();
        assert_eq!(
            e.message,
            "The lease's host could not be looked up: agentspace's project list is not valid. Nothing was recorded."
        );
        assert_eq!(f.registry(), None);
        // A project with a record never reads the list: swimcoach goes on to issue, which fails
        // here only for want of a CA.
        let e = f.place("swimcoach").unwrap_err();
        assert!(e.message.starts_with("agentspace's certificate authority"));
    }

    #[test]
    fn a_box_lease_with_no_registry_makes_no_registry_file() {
        let f = fx();
        let e = f.place("swimcoach").unwrap_err();
        assert!(e.message.starts_with("agentspace's certificate authority"));
        assert!(!f.ctx.home.join(".agentspace").exists());
        assert!(f.starts().is_empty());
    }

    #[test]
    fn a_pending_answer_checks_the_ca_first_and_writes_and_starts_nothing_without_one() {
        let f = fx();
        f.list("other");
        let e = f.place("other").unwrap_err();
        assert_eq!(
            e.message,
            "agentspace's certificate authority has not been set up, so no certificate can be issued. Nothing was changed; the operator sets it up."
        );
        assert_eq!(f.registry(), None);
        assert!(f.starts().is_empty());
    }

    #[test]
    fn check_pending_for_a_listed_project_adds_the_record_and_starts_one_run() {
        let f = fx();
        f.list("other");
        f.check("other").unwrap();
        let host = f.host("other").unwrap();
        assert_eq!(host.state, HostState::Creating);
        assert_eq!(host.stack, "agentspace-host-other");
        let at = parse_time(host.last_attempt_at.as_deref().unwrap()).unwrap();
        assert!((now() - at).abs() <= 5);
        // The box's record stays first.
        let all = hosts::read(&f.ctx.home).unwrap();
        assert_eq!(all.hosts[0].stack, "agentbox");
        let starts = f.starts();
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0].program, f.ctx.operator_program.clone().unwrap());
        assert_eq!(starts[0].args, ["provision", "-Host", "other"]);
        assert_eq!(starts[0].cwd, f.ctx.home);
        assert!(
            f.ctx
                .home
                .join(".agentspace")
                .join("hosts")
                .join("other")
                .is_dir()
        );
        assert!(
            !f.ctx
                .home
                .join(".agentspace")
                .join("hosts")
                .join("other")
                .join("provision.lock")
                .exists()
        );
        // Within 5 minutes, nothing more.
        let before = f.registry();
        f.check("other").unwrap();
        assert_eq!(f.starts().len(), 1);
        assert_eq!(f.registry(), before);
        // Nothing of the lease state file.
        assert!(!f.ctx.state_dir.join("leases.json").exists());
        assert!(!f.ctx.state_dir.join("leases.lock").exists());
    }

    #[test]
    fn the_run_gets_the_four_variables_and_only_those_the_cli_set() {
        let f = fx();
        f.list("other");
        f.check("other").unwrap();
        let env = &f.starts()[0].env;
        let get = |name: &str| env.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
        assert_eq!(
            get("USERPROFILE"),
            Some(f.ctx.home.clone().into_os_string())
        );
        assert_eq!(get("AGENTSPACE_STATE_DIR"), Some(OsString::new()));
        assert_eq!(get("AGENTSPACE_SSH_DIR"), Some(OsString::new()));
        assert_eq!(get("AGENTSPACE_PROXY_COMMAND"), Some(OsString::new()));
        assert_eq!(env.len(), 4);

        let mut f = fx();
        let elsewhere = f.ctx.home.parent().unwrap().join("state");
        let ssh = f.ctx.home.parent().unwrap().join("ssh");
        f.ctx.state_dir = elsewhere.clone();
        f.ctx.ssh_dir = ssh.clone();
        f.ctx.state_dir_env = Some(elsewhere.clone());
        f.ctx.ssh_dir_env = Some(ssh.clone());
        f.ctx.proxy_override = Some("relative\\proxy.cmd %h".to_string());
        f.list("other");
        f.check("other").unwrap();
        let env = &f.starts()[0].env;
        let get = |name: &str| env.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
        assert_eq!(
            get("AGENTSPACE_STATE_DIR"),
            Some(elsewhere.into_os_string())
        );
        assert_eq!(get("AGENTSPACE_SSH_DIR"), Some(ssh.into_os_string()));
        assert_eq!(
            get("AGENTSPACE_PROXY_COMMAND"),
            Some(OsString::from("relative\\proxy.cmd %h"))
        );
    }

    #[test]
    fn a_ready_host_keeps_ready_and_only_its_last_attempt_changes() {
        let f = fx();
        f.seed_with(
            r#"{"project":"other","stack":"agentspace-host-other","state":"ready","instance_id":"i-1234567890abcdef0"}"#,
        );
        f.check("other").unwrap();
        let host = f.host("other").unwrap();
        assert_eq!(host.state, HostState::Ready);
        assert_eq!(host.instance_id.as_deref(), Some("i-1234567890abcdef0"));
        assert!(host.last_attempt_at.is_some());
        assert_eq!(f.starts().len(), 1);
    }

    #[test]
    fn a_failed_record_five_minutes_old_runs_again_and_a_newer_one_does_not() {
        let f = fx();
        f.seed_with(&format!(
            r#"{{"project":"other","stack":"agentspace-host-other","state":"failed","last_attempt_at":"{}"}}"#,
            ago(299)
        ));
        let before = f.registry();
        f.check("other").unwrap();
        assert!(f.starts().is_empty());
        assert_eq!(f.registry(), before);

        let f = fx();
        f.seed_with(&format!(
            r#"{{"project":"other","stack":"agentspace-host-other","state":"failed","last_attempt_at":"{}"}}"#,
            ago(301)
        ));
        f.check("other").unwrap();
        assert_eq!(f.starts().len(), 1);
        let host = f.host("other").unwrap();
        assert_eq!(host.state, HostState::Creating);
    }

    #[test]
    fn a_held_run_lock_starts_nothing_and_writes_nothing() {
        let f = fx();
        f.seed_with(r#"{"project":"other","stack":"agentspace-host-other","state":"creating"}"#);
        let held = hosts::try_run_lock(&f.ctx.home, "other").unwrap().unwrap();
        let before = f.registry();
        f.check("other").unwrap();
        assert!(f.starts().is_empty());
        assert_eq!(f.registry(), before);
        drop(held);
        f.check("other").unwrap();
        assert_eq!(f.starts().len(), 1);
    }

    #[test]
    fn a_held_registry_lock_makes_check_pending_return_at_once() {
        let f = fx();
        f.list("other");
        let held = hosts::lock(&f.ctx.home).unwrap();
        let begun = Instant::now();
        f.check("other").unwrap();
        assert!(begun.elapsed() < Duration::from_secs(2));
        assert!(f.starts().is_empty());
        drop(held);
        assert_eq!(f.registry(), None);
    }

    #[test]
    fn check_pending_for_a_project_not_listed_and_without_a_record_does_nothing() {
        let f = fx();
        f.check("other").unwrap();
        assert_eq!(f.registry(), None);
        assert!(f.starts().is_empty());
        let f = fx();
        f.write("hosts.json", "{}");
        assert!(f.check("other").is_err());
        assert!(f.starts().is_empty());
    }

    #[test]
    fn a_program_that_cannot_be_found_fails_and_writes_nothing() {
        let mut f = fx();
        f.list("other");
        f.ctx.operator_program = Some(f.ctx.own_exe.with_file_name(OPERATOR_FILE));
        fs::remove_file(f.ctx.operator_program.as_ref().unwrap()).unwrap();
        assert_eq!(f.check("other").unwrap_err().message, OPERATOR_NOT_BESIDE);
        f.ctx.operator_program = Some(f.ctx.home.join("stand-in.exe"));
        assert_eq!(
            f.check("other").unwrap_err().message,
            OPERATOR_NAMED_MISSING
        );
        f.ctx.operator_program = None;
        assert_eq!(f.check("other").unwrap_err().message, OPERATOR_NOT_BESIDE);
        let cmd = f.ctx.home.join("stand-in.CMD");
        fs::write(&cmd, b"").unwrap();
        f.ctx.operator_program = Some(cmd.clone());
        f.ctx.system_root = Some(String::new());
        assert_eq!(
            f.check("other").unwrap_err().message,
            OPERATOR_NO_INTERPRETER
        );
        assert_eq!(f.registry(), None);
        assert!(f.starts().is_empty());

        f.ctx.system_root = Some("C:\\Windows\\".to_string());
        f.check("other").unwrap();
        let start = &f.starts()[0];
        assert_eq!(
            start.program,
            PathBuf::from("C:\\Windows\\System32\\cmd.exe")
        );
        assert_eq!(
            start.args,
            [
                "/d",
                "/c",
                cmd.to_str().unwrap(),
                "provision",
                "-Host",
                "other"
            ]
        );
    }

    #[test]
    fn a_run_that_is_not_due_needs_no_program() {
        let mut f = fx();
        f.seed_with(&format!(
            r#"{{"project":"other","stack":"agentspace-host-other","state":"creating","last_attempt_at":"{}"}}"#,
            ago(10)
        ));
        f.ctx.operator_program = None;
        f.check("other").unwrap();
        assert!(f.starts().is_empty());
    }

    #[test]
    fn a_start_that_fails_or_hangs_changes_nothing_more_and_is_bounded() {
        let f = fx_with(Fake {
            refuse: true,
            ..Fake::default()
        });
        f.list("other");
        f.check("other").unwrap();
        assert_eq!(f.host("other").unwrap().state, HostState::Creating);
        assert_eq!(f.starts().len(), 1);

        let f = fx_with(Fake {
            hang: Some(Duration::from_secs(25)),
            ..Fake::default()
        });
        f.list("other");
        let begun = Instant::now();
        f.check("other").unwrap();
        assert!(begun.elapsed() < Duration::from_secs(20));
        assert!(f.host("other").unwrap().last_attempt_at.is_some());
    }

    #[test]
    fn a_run_lock_that_cannot_be_tried_starts_nothing() {
        // A file where the hosts folder belongs: the run lock is not a free lock, so no run is
        // due, and the record is added without `last_attempt_at`.
        let f = fx();
        f.list("other");
        f.write("hosts", "a file");
        f.check("other").unwrap();
        assert!(f.starts().is_empty());
        let host = f.host("other").unwrap();
        assert_eq!(host.state, HostState::Creating);
        assert_eq!(host.last_attempt_at, None);
    }

    #[test]
    fn a_folder_that_cannot_be_made_is_a_start_that_fails() {
        let f = fx();
        f.list("other");
        let home = f.ctx.home.clone();
        let detach = f.detach.clone();
        let run = operator_run(&f.ctx, "other").unwrap();
        // The host's folder's place holds a file: the start makes nothing and starts nothing.
        fs::create_dir_all(hosts::hosts_dir(&home)).unwrap();
        fs::write(hosts::host_dir(&home, "other"), b"x").unwrap();
        start_run(&f.ctx, "other", run, &detach, Duration::from_secs(5));
        assert!(f.starts().is_empty());
    }

    /// A bound already used up still begins the start, since step 4 has written
    /// `last_attempt_at` and a record MUST NOT say a run was started when none was.
    #[test]
    fn a_bound_already_used_up_still_begins_the_start() {
        let f = fx();
        f.list("other");
        let detach = f.detach.clone();
        let run = operator_run(&f.ctx, "other").unwrap();
        start_run(&f.ctx, "other", run, &detach, Duration::ZERO);
        assert_eq!(f.starts().len(), 1);
    }

    #[test]
    fn a_project_that_is_not_normalized_makes_no_path() {
        let f = fx();
        assert!(f.check("..").is_err());
        assert!(f.place("..").is_err());
        assert!(f.starts().is_empty());
    }

    // ---- v0.09: D66's table (driver.md, "On a `ready` host") ----------------------------------

    const NOW: i64 = 1_800_000_000;

    fn lease(state: &str, project: &str, driver: Option<&str>, size: &str) -> Lease {
        Lease {
            id: "5f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90".to_string(),
            project: project.to_string(),
            state: state.to_string(),
            platform: "linux/arm64".to_string(),
            profile: "agent-linux".to_string(),
            size: size.to_string(),
            ttl_seconds: 3600,
            created_at: format_time(NOW - 600),
            expires_at: (state == "ready").then(|| format_time(NOW + 600)),
            released_at: None,
            ssh: (state == "ready").then(|| agentspace_state::Ssh {
                host_alias: format!("agentspace-{project}-5f2a9c1e"),
                user: "ubuntu".to_string(),
            }),
            reasons: None,
            driver: driver.map(str::to_string),
        }
    }

    /// A live `ready` lease of `project`, which holds its host.
    fn holding(project: &str) -> Lease {
        lease("ready", project, Some("ec2"), "m")
    }

    /// A `pending` lease of `project` that a run would place.
    fn waiting(project: &str, size: &str) -> Lease {
        lease("pending", project, Some("ec2"), size)
    }

    fn decide(size: &str, instance_type: Option<&str>, leases: &[Lease]) -> OnReady {
        on_ready(size, instance_type, leases, "p", NOW)
    }

    #[test]
    fn row_1_a_pending_lease_a_run_would_place_holds_every_new_lease_pending() {
        // Exactly, larger and smaller all answer `pending` while it exists.
        for (size, have) in [
            ("m", "m9g.xlarge"),
            ("s", "m9g.xlarge"),
            ("xl", "m9g.xlarge"),
        ] {
            let leases = [holding("p"), waiting("p", "l")];
            assert_eq!(
                decide(size, Some(have), &leases),
                OnReady::Pending,
                "{size}"
            );
        }
        // Only a run's lease counts: another driver's, none, a size outside SIZES, another
        // project's and a `ready` one do not hold the place `pending`.
        let not_a_runs = [
            lease("pending", "p", Some("x"), "l"),
            lease("pending", "p", None, "l"),
            lease("pending", "p", Some("ec2"), "xxl"),
            lease("pending", "q", Some("ec2"), "l"),
        ];
        assert_eq!(decide("m", Some("m9g.xlarge"), &not_a_runs), OnReady::Ready);
    }

    #[test]
    fn row_2_no_type_or_one_outside_the_map_is_pending_never_no_capacity() {
        let leases = [holding("p")];
        assert_eq!(decide("xl", None, &leases), OnReady::Pending);
        assert_eq!(decide("m", None, &[]), OnReady::Pending);
        assert_eq!(decide("xl", Some("m9g.8xlarge"), &leases), OnReady::Pending);
        assert_eq!(decide("s", Some("M9G.large"), &leases), OnReady::Pending);
    }

    #[test]
    fn row_3_exactly_is_ready_whatever_holds_the_host() {
        for size in SIZES {
            let have = instance_type_for(size);
            assert_eq!(decide(size, have, &[]), OnReady::Ready, "{size}");
            assert_eq!(
                decide(size, have, &[holding("p")]),
                OnReady::Ready,
                "{size}"
            );
        }
    }

    #[test]
    fn rows_4_and_5_a_live_ready_lease_places_on_a_larger_host_and_refuses_a_smaller_one() {
        let leases = [holding("p")];
        assert_eq!(decide("m", Some("m9g.2xlarge"), &leases), OnReady::Ready);
        assert_eq!(decide("s", Some("m9g.4xlarge"), &leases), OnReady::Ready);
        assert_eq!(decide("l", Some("m9g.xlarge"), &leases), OnReady::Refused);
        assert_eq!(decide("xl", Some("m9g.large"), &leases), OnReady::Refused);
    }

    #[test]
    fn row_6_with_nothing_holding_the_host_a_resize_either_way_is_pending() {
        assert_eq!(decide("m", Some("m9g.4xlarge"), &[]), OnReady::Pending);
        assert_eq!(decide("xl", Some("m9g.xlarge"), &[]), OnReady::Pending);
        // None of these holds the host: expired, released, unsatisfiable, another project's
        // `ready` lease, and a `pending` lease no run would place.
        let mut expired = holding("p");
        expired.expires_at = Some(format_time(NOW));
        let mut released = lease("released", "p", Some("ec2"), "m");
        released.released_at = Some(format_time(NOW - 60));
        let nothing_holds = [
            expired,
            released,
            lease("unsatisfiable", "p", Some("ec2"), "m"),
            holding("q"),
            lease("pending", "p", Some("ec2"), "xxl"),
        ];
        assert_eq!(
            decide("xl", Some("m9g.xlarge"), &nothing_holds),
            OnReady::Pending
        );
        assert_eq!(
            decide("s", Some("m9g.xlarge"), &nothing_holds),
            OnReady::Pending
        );
        // A `ready` lease one second before it expires still holds the host.
        let mut last_second = holding("p");
        last_second.expires_at = Some(format_time(NOW + 1));
        assert_eq!(
            decide("xl", Some("m9g.xlarge"), &[last_second]),
            OnReady::Refused
        );
    }

    /// Makes the CA in the fixture's throwaway home with the operator's own `ca`, run in process,
    /// as the integration tests do: the context names the throwaway home and a made-up user and
    /// computer, and nothing of the real profile.
    fn make_ca(home: &Path) {
        struct Quiet;
        impl agentspace_operator_api::Say for Quiet {
            fn mode(&self) -> agentspace_operator_api::Mode {
                agentspace_operator_api::Mode::Person
            }
            fn progress(&self, _: &str, _: Option<agentspace_operator_api::CallerLine>) {}
            fn warning(&self, _: &str) {}
            fn detail(&self, _: &str) {}
        }
        let ctx = agentspace_operator_api::Context {
            home: home.to_path_buf(),
            local_app_data: None,
            system_root: PathBuf::from(
                std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()),
            ),
            nested: false,
        };
        let env = agentspace_ca::CaEnv {
            user_name: Some("someone".to_string()),
            computer_name: Some("EXAMPLEPC".to_string()),
            ..agentspace_ca::CaEnv::default()
        };
        assert!(
            agentspace_ca::ca(&ctx, &env, &Quiet).is_ok(),
            "The throwaway CA could not be made"
        );
    }

    fn fx_ca() -> Fx {
        let f = fx();
        make_ca(&f.ctx.home);
        f
    }

    impl Fx {
        fn leases(&self, records: &[Lease]) {
            let file = agentspace_state::StateFile {
                leases: records.to_vec(),
            };
            let bytes = agentspace_state::serialize(&file).unwrap();
            fs::create_dir_all(&self.ctx.state_dir).unwrap();
            fs::write(self.ctx.state_dir.join("leases.json"), bytes).unwrap();
        }
        fn place_size(&self, project: &str, size: &str) -> Result<Placed, Failure> {
            let mut r = request(project);
            r.size = size;
            place(&self.ctx, &r, &self.detach)
        }
    }

    /// A live lease, by the real clock, for a fixture's lease state file.
    fn live(mut l: Lease) -> Lease {
        l.created_at = ago(600);
        if l.state == "ready" {
            l.expires_at = Some(format_time(now() + 3600));
        }
        l
    }

    const OTHER_READY_XLARGE: &str = r#"{"project":"other","stack":"agentspace-host-other","state":"ready","instance_id":"i-1234567890abcdef0","instance_type":"m9g.xlarge"}"#;

    #[test]
    fn row_5_through_place_answers_d66s_message_and_writes_and_starts_nothing() {
        let f = fx();
        f.seed_with(OTHER_READY_XLARGE);
        f.leases(&[live(holding("other"))]);
        let before = f.registry();
        let Placed::Unsatisfiable { reasons } = f.place_size("other", "l").unwrap() else {
            panic!("Not unsatisfiable");
        };
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].code, "no_capacity");
        assert_eq!(
            reasons[0].message,
            "No capacity can be found for project 'other' at size 'l': its host is serving other leases at a smaller size. Release them, or ask for a size they fit."
        );
        assert_eq!(f.registry(), before);
        assert!(f.starts().is_empty());
        assert!(!f.ctx.home.join(".agentspace").join("leases").exists());
        assert!(!hosts::host_dir(&f.ctx.home, "other").exists());
    }

    #[test]
    fn a_lease_state_file_that_cannot_be_read_fails_a_ready_hosts_place_and_writes_nothing() {
        let f = fx_ca();
        f.seed_with(OTHER_READY_XLARGE);
        f.write("leases.json", "{");
        let before = f.registry();
        let e = f.place_size("other", "m").unwrap_err();
        assert_eq!(
            e.message,
            "The lease's host could not be looked up: agentspace's lease records could not be read. Nothing was recorded."
        );
        assert_eq!(f.registry(), before);
        assert!(f.starts().is_empty());
        assert!(!f.ctx.home.join(".agentspace").join("leases").exists());
    }

    #[test]
    fn the_lease_state_file_is_read_only_for_a_ready_record() {
        // A `creating` record is placed `pending` without reading it.
        let f = fx_ca();
        f.seed_with(r#"{"project":"other","stack":"agentspace-host-other","state":"creating"}"#);
        f.write("leases.json", "{");
        assert_eq!(f.place_size("other", "m").unwrap(), Placed::Pending);
        assert_eq!(f.starts().len(), 1);
    }

    /// Ruling on #0203 (pins P31 and P38): the decision reads `read_for_placing`, and the write
    /// starts from a plain `read`, so the box's seed record goes to the disk with no type.
    #[test]
    fn a_create_of_a_listed_project_with_no_registry_writes_the_seed_box_record_with_no_type() {
        let f = fx_ca();
        f.list("other");
        assert_eq!(f.registry(), None);
        assert_eq!(f.place_size("other", "m").unwrap(), Placed::Pending);
        let bytes = String::from_utf8(f.registry().unwrap()).unwrap();
        assert!(!bytes.contains("instance_type"), "{bytes}");
        let all = hosts::read(&f.ctx.home).unwrap();
        assert_eq!(all.hosts[0].stack, "agentbox");
        assert_eq!(all.hosts[0].instance_type, None);
        assert_eq!(all.hosts[1].project, "other");
        assert_eq!(all.hosts[1].state, HostState::Creating);
        assert_eq!(f.starts().len(), 1);
    }

    #[test]
    fn row_1_through_place_is_pending_starts_a_due_run_and_keeps_ready_and_the_type() {
        let f = fx_ca();
        f.seed_with(OTHER_READY_XLARGE);
        f.leases(&[live(waiting("other", "l"))]);
        assert_eq!(f.place_size("other", "m").unwrap(), Placed::Pending);
        let host = f.host("other").unwrap();
        assert_eq!(host.state, HostState::Ready);
        assert_eq!(host.instance_type.as_deref(), Some("m9g.xlarge"));
        assert_eq!(host.instance_id.as_deref(), Some("i-1234567890abcdef0"));
        assert!(host.last_attempt_at.is_some());
        let starts = f.starts();
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0].args, ["provision", "-Host", "other"]);
        // Within 5 minutes, no second run and no write.
        let before = f.registry();
        assert_eq!(f.place_size("other", "m").unwrap(), Placed::Pending);
        assert_eq!(f.starts().len(), 1);
        assert_eq!(f.registry(), before);
    }

    #[test]
    fn row_2_through_place_is_pending_even_while_a_lease_holds_the_host() {
        let f = fx_ca();
        f.seed_with(r#"{"project":"other","stack":"agentspace-host-other","state":"ready"}"#);
        f.leases(&[live(holding("other"))]);
        assert_eq!(f.place_size("other", "xl").unwrap(), Placed::Pending);
        assert_eq!(f.starts().len(), 1);
        let host = f.host("other").unwrap();
        assert_eq!(host.state, HostState::Ready);
        assert_eq!(host.instance_type, None);
    }

    #[test]
    fn row_6_through_place_starts_a_resizing_run_only_when_one_is_due() {
        let f = fx_ca();
        f.seed_with(r#"{"project":"other","stack":"agentspace-host-other","state":"ready","instance_type":"m9g.4xlarge"}"#);
        assert_eq!(f.place_size("other", "m").unwrap(), Placed::Pending);
        assert_eq!(f.starts().len(), 1);
        assert_eq!(
            f.host("other").unwrap().instance_type.as_deref(),
            Some("m9g.4xlarge")
        );

        let f = fx_ca();
        f.seed_with(&format!(
            r#"{{"project":"other","stack":"agentspace-host-other","state":"ready","instance_type":"m9g.4xlarge","last_attempt_at":"{}"}}"#,
            ago(10)
        ));
        let before = f.registry();
        assert_eq!(f.place_size("other", "m").unwrap(), Placed::Pending);
        assert!(f.starts().is_empty());
        assert_eq!(f.registry(), before);
    }

    #[test]
    fn a_box_lease_with_no_registry_is_refused_by_the_implied_type_and_makes_no_registry_file() {
        let f = fx();
        f.leases(&[live(holding("swimcoach"))]);
        let Placed::Unsatisfiable { reasons } = f.place_size("swimcoach", "xl").unwrap() else {
            panic!("Not unsatisfiable");
        };
        assert_eq!(reasons[0].message, no_capacity_at_size("swimcoach", "xl"));
        assert_eq!(f.registry(), None);
        assert!(!hosts::hosts_lock_path(&f.ctx.home).exists());
        assert!(f.starts().is_empty());
    }

    #[test]
    fn a_box_lease_with_no_registry_held_pending_writes_the_seed_with_no_type() {
        let f = fx_ca();
        f.leases(&[live(waiting("swimcoach", "l"))]);
        assert_eq!(f.place_size("swimcoach", "m").unwrap(), Placed::Pending);
        let bytes = String::from_utf8(f.registry().unwrap()).unwrap();
        assert!(!bytes.contains("instance_type"), "{bytes}");
        let host = f.host("swimcoach").unwrap();
        assert_eq!(host.state, HostState::Ready);
        assert!(host.last_attempt_at.is_some());
        assert_eq!(f.starts().len(), 1);
        assert_eq!(f.starts()[0].args, ["provision", "-Host", "swimcoach"]);
    }
}
