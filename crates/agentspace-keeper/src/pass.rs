//! One pass, with a turn per host (docs/keeper.md, A pass; The hosts of a pass; Before the start;
//! Log lines).

use crate::Deps;
use crate::args::check_ssh_exe;
use crate::hosts::{
    HostKey, box_project, host_of, list_state_folder, read_registry, record_projects,
    ssh_log_projects,
};
use crate::log::{Logger, pass_line, pass_line_host, rotate_ssh_log};
use agentspace_keeper_api::{HolderRecord, Query, StartRequest, Started, Stopped, Taken};
use agentspace_state::{Record, StoreError};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;

/// What one pass is run with.
#[derive(Debug, Clone, Copy)]
pub struct PassContext<'a> {
    /// The state folder, a full path, which exists.
    pub state_dir: &'a Path,
    /// The keeper's `%USERPROFILE%`, a full path: the holder's working folder, and the folder
    /// under which the host registry is read (keeper.md, Hosts).
    pub profile: &'a Path,
    /// `-SshExe`, or its default.
    pub ssh_exe: &'a Path,
    /// The keeper's own program.
    pub own_exe: &'a Path,
    /// `-Now`, as a Unix time in whole seconds; `None` takes the current UTC second.
    pub now: Option<i64>,
    /// The holder a keeper tracks on the box (keeper.md, Watching): its PID, creation time, lease
    /// ID and alias, as the keeper holds them. `None` in a one-pass run, which tracks nothing. A
    /// keeper's own loop gives the pass every host's tracked holder through the crate's own
    /// entry, and this field then goes unused.
    pub tracked: Option<&'a HolderRecord>,
}

/// What a pass, or one host's turn, did, by its action line, for the exit code and for a keeper's
/// tracking (keeper.md, Watching, "What each host's turn did").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// `started`: the holder, recorded.
    Started(HolderRecord),
    /// `kept`: the record's holder.
    Kept(HolderRecord),
    /// `stopped`: the record's holder is no longer found.
    Stopped(HolderRecord),
    /// `failed`, from a stop: the record's holder, which a stop was tried on.
    StopFailed(HolderRecord),
    /// `lost`: the tracked holder.
    Lost(HolderRecord),
    /// `idle`; in a keeper, also a host in its hold-off whose turn would have started a holder,
    /// which logs no line.
    Idle,
    /// `skipped`: another pass held `board-holder.lock`.
    Skipped,
    /// `failed`, from a start, the checks before it included; the holder's PID when it started
    /// but could not be recorded.
    StartFailed(Option<u32>),
    /// `failed` before the pass, or a host's turn, decided anything: the lease state or the host
    /// registry could not be read, or another error of the pass or the turn.
    Failed,
}

/// The outcome of a pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassReport {
    /// What the pass did: the box's turn, or, when the pass took no turn, the pass's own line
    /// (`skipped`, or `failed`).
    pub action: Action,
    /// The one-pass run's exit code (keeper.md, Exit codes): 0 when the pass ran or was skipped,
    /// 1 when the lease state or the host registry could not be read, the pass or any host's turn
    /// met an error it does not handle, or a line of the pass or of any turn could not be logged.
    pub exit_code: i32,
}

/// One host's turn, as a keeper tracks by it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Turn {
    pub host: HostKey,
    pub action: Action,
}

/// A pass, whole: what a keeper's loop reads from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Passed {
    /// The pass's own action when it took no turn, else the box's turn's.
    pub action: Action,
    /// Every host's turn, in order, the box's first. Empty when the pass took none.
    pub turns: Vec<Turn>,
    /// The hosts of the pass (keeper.md, The hosts of a pass), or `None` when it took no turn.
    pub hosts: Option<Vec<HostKey>>,
    pub exit_code: i32,
}

/// What a keeper gives its pass: the holder it tracks on each host, and whether a host's
/// hold-off is running (keeper.md, Watching). A one-pass run gives nothing.
pub(crate) struct Tracking<'a> {
    pub tracked: &'a BTreeMap<HostKey, HolderRecord>,
    pub held_off: &'a dyn Fn(&HostKey) -> bool,
}

/// A decision, with the line that says it; a host in its hold-off logs none.
struct Outcome {
    action: Action,
    exit_code: i32,
    line: Option<String>,
}

/// The pass time, fixed when the pass begins: as a Unix time, and as every line writes it.
struct PassTime {
    secs: i64,
    text: String,
}

/// Runs one pass (keeper.md, The order of a pass), logging its lines to `board-holder.log` in
/// the state folder and to `stdout`. `ctx.tracked`, when given, is the holder tracked on the box.
pub fn run_pass(ctx: &PassContext, deps: &Deps, stdout: &mut dyn Write) -> PassReport {
    let mut tracked = BTreeMap::new();
    if let Some(t) = ctx.tracked {
        tracked.insert(None, t.clone());
    }
    let never = |_: &HostKey| false;
    let keeping = Tracking {
        tracked: &tracked,
        held_off: &never,
    };
    let passed = run_pass_hosts(ctx, &keeping, deps, stdout);
    PassReport {
        action: passed.action,
        exit_code: passed.exit_code,
    }
}

/// Runs one pass for a keeper, or a one-pass run: every host's turn, and what each did.
pub(crate) fn run_pass_hosts(
    ctx: &PassContext,
    keeping: &Tracking,
    deps: &Deps,
    stdout: &mut dyn Write,
) -> Passed {
    // The pass time is fixed when the pass begins, before the lock is taken.
    let secs = ctx
        .now
        .unwrap_or_else(|| deps.clock.utc_now_ms().div_euclid(1000));
    let time = PassTime {
        secs,
        text: agentspace_state::format_time(secs),
    };
    let mut log = Logger::new(ctx.state_dir, deps.clock, stdout);

    // Step 1. A pass that cannot take the lock logs `skipped` and ends. One lock serves every
    // host.
    let guard = match deps.locks.pass(ctx.state_dir) {
        Taken::Held(guard) => guard,
        // The pin gives `Busy` for the keeper lock only; from the pass lock it is another taker
        // holding it, and is read as `Skipped` (architect, 2026-11-19, for E0.06-T01-S06).
        Taken::Skipped | Taken::Busy(_) => {
            let line = pass_line(&time.text, "skipped", "", "", "", Some("locked"));
            return whole(Action::Skipped, 0, &line, &time.text, &mut log);
        }
        Taken::Failed(e) => {
            let line = pass_error(&time.text, &e);
            return whole(Action::Failed, 1, &line, &time.text, &mut log);
        }
    };
    let passed = locked_pass(ctx, keeping, deps, &time, &mut log);
    // Step 9: the lock is released once every line of the pass is logged.
    drop(guard);
    passed
}

/// A pass that took no turn: its one line, logged. A pass whose own line is dropped ends as a
/// pass that met an error, and tries once to log that in its place.
fn whole(action: Action, exit_code: i32, line: &str, time: &str, log: &mut Logger) -> Passed {
    let exit_code = match log.log(line) {
        Ok(()) => exit_code,
        Err(e) => {
            let reason =
                format!("The pass stopped on an error: board-holder.log could not be written: {e}");
            let _ = log.log(&pass_line(time, "failed", "", "", "", Some(&reason)));
            1
        }
    };
    Passed {
        action,
        turns: Vec::new(),
        hosts: None,
        exit_code,
    }
}

/// A `failed` line, with no host, for an error of the whole pass.
fn pass_error(time: &str, error: &str) -> String {
    pass_line(
        time,
        "failed",
        "",
        "",
        "",
        Some(&format!("The pass stopped on an error: {error}")),
    )
}

/// Steps 2 to 8.
fn locked_pass(
    ctx: &PassContext,
    keeping: &Tracking,
    deps: &Deps,
    when: &PassTime,
    log: &mut Logger,
) -> Passed {
    let time = when.text.as_str();
    let fail = |line: String, log: &mut Logger| whole(Action::Failed, 1, &line, time, log);

    // Step 2: a matcher that fails its own cases trusts nothing, on any host.
    if let Err(cases) = deps.matcher.self_check() {
        // `self_check`'s error is the failing cases alone (`<case>; <case>`), and the pass adds
        // the words before them (architect, 2026-11-19, for E0.06-T01-S06).
        return fail(
            pass_error(
                time,
                &format!("The holder command matcher failed its own checks: {cases}"),
            ),
            log,
        );
    }

    // Step 3: the holders' logs, the box's first, then each other host's found by listing the
    // state folder. A failure to rotate, to list, or to log the rotation does not stop the pass.
    if rotate_ssh_log(ctx.state_dir, None) {
        let _ = log.log(&pass_line(time, "rotated", "", "", "", None));
    }
    if let Ok(names) = list_state_folder(ctx.state_dir) {
        for project in ssh_log_projects(&names) {
            if rotate_ssh_log(ctx.state_dir, Some(&project)) {
                let _ = log.log(&pass_line_host(
                    time,
                    "rotated",
                    "",
                    "",
                    "",
                    Some(&project),
                    None,
                ));
            }
        }
    }

    // Step 4: the lease state, read without a lock. A missing file is no leases.
    let leases = match agentspace_state::read(ctx.state_dir) {
        Ok(file) => file.leases,
        Err(e) => {
            let why = match e {
                StoreError::Unreadable(w)
                | StoreError::NotValid(w)
                | StoreError::CannotLock(w)
                | StoreError::Unwritable(w) => w,
                StoreError::Busy => "the state file is busy".to_string(),
            };
            return fail(
                pass_line(
                    time,
                    "failed",
                    "",
                    "",
                    "",
                    Some(&format!("Lease state could not be read: {why}")),
                ),
                log,
            );
        }
    };

    // Step 5: the host registry, read without its lock, under the keeper's own USERPROFILE. A
    // missing registry is the box alone.
    let registry = match read_registry(ctx.profile) {
        Ok(hosts) => hosts,
        Err(why) => {
            return fail(
                pass_line(
                    time,
                    "failed",
                    "",
                    "",
                    "",
                    Some(&format!("Host registry could not be read: {why}")),
                ),
                log,
            );
        }
    };
    let box_name = box_project(&registry);

    // Step 6: the ready leases, each with its host.
    let ready: Vec<(HostKey, Ready)> = leases
        .iter()
        .filter_map(|r| Ready::from(r, when.secs).map(|l| (host_of(&registry, &r.project), l)))
        .collect();

    // Step 7: the hosts of the pass. The box always; each host a ready lease is on; each other
    // host whose record is in the state folder; and each host a keeper tracks a holder on.
    let names = match list_state_folder(ctx.state_dir) {
        Ok(names) => names,
        Err(e) => return fail(pass_error(time, &e), log),
    };
    let mut hosts: BTreeSet<HostKey> = BTreeSet::new();
    hosts.insert(None);
    hosts.extend(ready.iter().map(|(h, _)| h.clone()));
    hosts.extend(record_projects(&names, &box_name).into_iter().map(Some));
    hosts.extend(keeping.tracked.keys().cloned());

    // Step 8: each host's turn, one at a time, the box's first. A turn that fails, or meets an
    // error, ends only that turn.
    let mut turns = Vec::new();
    let mut exit_code = 0;
    for host in &hosts {
        let on_host: Vec<&Ready> = ready
            .iter()
            .filter(|(h, _)| h == host)
            .map(|(_, l)| l)
            .collect();
        let who = Who {
            files: host.as_deref(),
            name: host.as_deref().unwrap_or(&box_name),
        };
        let tracked = keeping.tracked.get(host);
        let outcome = turn(
            ctx,
            deps,
            time,
            &who,
            &on_host,
            tracked,
            (keeping.held_off)(host),
        );
        if outcome.exit_code != 0 {
            exit_code = 1;
        }
        if let Some(line) = &outcome.line
            && let Err(e) = log.log(line)
        {
            // A turn whose line was dropped ends as a turn that met an error: it tries once to
            // log that, with the host's `host=` field for a host that is not the box, and the
            // pass goes on (keeper.md, Log lines). What the turn did stands.
            let reason =
                format!("The pass stopped on an error: board-holder.log could not be written: {e}");
            let _ = log.log(&who.line(time, "failed", "", "", "", false, Some(&reason)));
            exit_code = 1;
        }
        turns.push(Turn {
            host: host.clone(),
            action: outcome.action,
        });
    }

    let action = turns.first().map_or(Action::Failed, |t| t.action.clone());
    Passed {
        action,
        turns,
        hosts: Some(hosts.into_iter().collect()),
        exit_code,
    }
}

/// The host a turn is about: the files it names (`None` for the box), and its project, which its
/// lines name.
struct Who<'a> {
    files: Option<&'a str>,
    name: &'a str,
}

impl Who<'_> {
    /// A line of this host's turn (keeper.md, A pass's line): `host=<project>` on every line of a
    /// host that is not the box, and on the box's lines that name a holder only.
    #[allow(clippy::too_many_arguments)]
    fn line(
        &self,
        time: &str,
        action: &str,
        lease: &str,
        alias: &str,
        pid: &str,
        names_holder: bool,
        reason: Option<&str>,
    ) -> String {
        let host = (self.files.is_some() || names_holder).then_some(self.name);
        pass_line_host(time, action, lease, alias, pid, host, reason)
    }

    fn holder_line(
        &self,
        time: &str,
        action: &str,
        r: &HolderRecord,
        reason: Option<&str>,
    ) -> String {
        self.line(
            time,
            action,
            &r.lease_id,
            &r.alias,
            &r.pid.to_string(),
            true,
            reason,
        )
    }
}

/// One host's turn (keeper.md, The order of a pass, step 8): v0.06's pass for that host alone,
/// on its record and its leases.
fn turn(
    ctx: &PassContext,
    deps: &Deps,
    time: &str,
    who: &Who,
    ready: &[&Ready],
    tracked: Option<&HolderRecord>,
    held_off: bool,
) -> Outcome {
    // Step 8.1: a record that names no trusted holder is removed, and its process left alone.
    let trusted = match deps.records.read_host(ctx.state_dir, who.files) {
        None => None,
        Some(record) => {
            if is_trusted(&record, ctx.ssh_exe, deps) {
                Some(record)
            } else {
                if let Err(e) = deps.records.remove_host(ctx.state_dir, who.files) {
                    // An untrusted record that cannot be removed is an error the pass does not
                    // handle: it ends this host's turn with this line, and the run exits 1
                    // (keeper.md, Exit codes; The order of a pass).
                    return Outcome {
                        action: Action::Failed,
                        exit_code: 1,
                        line: Some(who.line(
                            time,
                            "failed",
                            "",
                            "",
                            "",
                            false,
                            Some(&format!(
                                "The pass stopped on an error: The record could not be removed: {e}"
                            )),
                        )),
                    };
                }
                None
            }
        }
    };

    // Step 8.2: the first row that applies.
    if let Some(record) = trusted {
        // The record's lease_id is compared with a lease's ID without regard to ASCII case
        // (state.md, The holder record; architect, 2026-11-19); the alias is compared exactly.
        // Only the host's own ready leases count: a lease ready on another host stops it.
        let keep = ready
            .iter()
            .any(|l| l.id.eq_ignore_ascii_case(&record.lease_id) && l.alias == record.alias);
        if keep {
            return Outcome {
                line: Some(who.holder_line(time, "kept", &record, None)),
                action: Action::Kept(record),
                exit_code: 0,
            };
        }
        return stop(ctx, deps, time, who, record);
    }
    if let Some(t) = tracked {
        return Outcome {
            line: Some(who.holder_line(time, "lost", t, None)),
            action: Action::Lost(t.clone()),
            exit_code: 0,
        };
    }
    match choose(ready) {
        None => Outcome {
            action: Action::Idle,
            exit_code: 0,
            line: Some(who.line(time, "idle", "", "", "", false, None)),
        },
        // A host in a keeper's hold-off starts nothing, and its turn logs no line.
        Some(_) if held_off => Outcome {
            action: Action::Idle,
            exit_code: 0,
            line: None,
        },
        Some(lease) => start(ctx, deps, time, who, lease),
    }
}

/// A ready lease: stored `ready`, and `expires_at` strictly after the pass time.
struct Ready<'a> {
    id: &'a str,
    alias: &'a str,
    expires_at: i64,
}

impl<'a> Ready<'a> {
    fn from(r: &'a Record, pass_time: i64) -> Option<Ready<'a>> {
        if r.state != "ready" {
            return None;
        }
        let expires_at = agentspace_state::parse_time(r.expires_at.as_deref()?)?;
        let ssh = r.ssh.as_ref()?;
        (expires_at > pass_time).then_some(Ready {
            id: &r.id,
            alias: &ssh.host_alias,
            expires_at,
        })
    }
}

/// The ready lease with the latest `expires_at`, ties going to the lowest ID in ordinal order
/// (keeper.md, Which lease), among one host's.
fn choose<'a, 'b>(ready: &[&'b Ready<'a>]) -> Option<&'b Ready<'a>> {
    ready.iter().copied().min_by(|a, b| {
        b.expires_at
            .cmp(&a.expires_at)
            .then_with(|| a.id.as_bytes().cmp(b.id.as_bytes()))
    })
}

/// Items 2 to 4 of Trusting the record: the process runs and can be queried, its creation time
/// equals the record's, and its command line is the holder command for this `-SshExe` and the
/// record's alias. Item 1 is the record trait's.
fn is_trusted(record: &HolderRecord, ssh_exe: &Path, deps: &Deps) -> bool {
    match deps.processes.query(record.pid) {
        Query::Running(info) => {
            info.process_started_at == record.process_started_at
                && info
                    .command_line
                    .as_deref()
                    .is_some_and(|cl| deps.matcher.is_holder(cl, ssh_exe, &record.alias))
        }
        Query::Gone | Query::CannotOpen(_) => false,
    }
}

/// Stops a trusted holder whose lease is not one of its host's ready leases, or whose alias
/// changed, and starts nothing on that host (keeper.md, Stopping a holder). The record is removed
/// only when the stop no longer finds the holder, or the holder stopped and only some of its tree
/// did not.
fn stop(ctx: &PassContext, deps: &Deps, time: &str, who: &Who, record: HolderRecord) -> Outcome {
    let found = deps.processes.stop(&record, ctx.ssh_exe, deps.matcher);
    let (remove, reason) = match found {
        Stopped::All | Stopped::Gone => (true, None),
        Stopped::CannotOpenToStop(n) => (
            false,
            Some(format!(
                "The holder could not be opened to stop it: Windows error {n}."
            )),
        ),
        Stopped::NotStopped(why) => (
            false,
            Some(format!(
                "The holder could not be stopped: {why}, but the holder is still running."
            )),
        ),
        Stopped::Error(e) => (false, Some(format!("The holder could not be stopped: {e}"))),
        Stopped::StillRunning => (
            false,
            Some("The holder is still running after it was stopped.".to_string()),
        ),
        Stopped::TreeLeft(pids) => {
            let list: Vec<String> = pids.iter().map(u32::to_string).collect();
            (
                true,
                Some(format!(
                    "The holder stopped, but these processes of its tree did not: {}.",
                    list.join(",")
                )),
            )
        }
    };
    if remove {
        // After a stop a failed removal is not reported, and the line stays as it is: the next
        // pass removes the record, which then names no running holder (keeper.md, Exit codes;
        // architect, 2026-11-19, for E0.06-T01-S06).
        let _ = deps.records.remove_host(ctx.state_dir, who.files);
    }
    match reason {
        None => Outcome {
            line: Some(who.holder_line(time, "stopped", &record, None)),
            action: Action::Stopped(record),
            exit_code: 0,
        },
        Some(r) => Outcome {
            line: Some(who.holder_line(time, "failed", &record, Some(&r))),
            action: Action::StopFailed(record),
            exit_code: 0,
        },
    }
}

/// A host alias (keeper.md, Before the start): `\A[A-Za-z0-9][A-Za-z0-9._-]{0,127}\z`,
/// case-sensitively.
fn is_host_alias(alias: &str) -> bool {
    let b = alias.as_bytes();
    !b.is_empty()
        && b.len() <= 128
        && b[0].is_ascii_alphanumeric()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

/// The checks of Before the start: the form checks of usage row 5, then the table's rows in
/// order. The first that fails is the `<why>`.
fn refusal(ctx: &PassContext, alias: &str) -> Option<String> {
    refusal_before_start(ctx.state_dir, ctx.own_exe, ctx.ssh_exe, alias)
}

/// The refusal of keeper.md's "Before the start" table for a start through `alias`, the `<why>`
/// after `The holder could not be started: `, or `None` when the start may go on. It is not
/// public API (architect, 2026-11-19, for E0.06-T01-S06): a pass is its only caller.
pub(crate) fn refusal_before_start(
    state_dir: &Path,
    own_exe: &Path,
    ssh_exe: &Path,
    alias: &str,
) -> Option<String> {
    let ssh = ssh_exe.to_string_lossy();
    if let Some(form) = check_ssh_exe(&ssh) {
        return Some(form);
    }
    if !ssh_exe.is_file() {
        return Some(format!("SshExe '{ssh}' does not exist."));
    }
    let state = state_dir.to_string_lossy();
    if state.contains('"') {
        return Some(format!("The state folder '{state}' holds a quote."));
    }
    let own = own_exe.to_string_lossy();
    if own.contains('"') {
        return Some(format!("This program's path '{own}' holds a quote."));
    }
    if !is_host_alias(alias) {
        return Some(format!("Alias '{alias}' is not a host alias."));
    }
    None
}

/// Starts a holder on the turn's host through the chosen lease (keeper.md, Starting a holder),
/// records it in that host's record, and only then says `started`.
fn start(ctx: &PassContext, deps: &Deps, time: &str, who: &Who, lease: &Ready) -> Outcome {
    let failed = |pid: Option<u32>, reason: &str| Outcome {
        action: Action::StartFailed(pid),
        exit_code: 0,
        line: Some(who.line(
            time,
            "failed",
            lease.id,
            lease.alias,
            &pid.map(|p| p.to_string()).unwrap_or_default(),
            true,
            Some(reason),
        )),
    };
    if let Some(why) = refusal(ctx, lease.alias) {
        return failed(None, &format!("The holder could not be started: {why}"));
    }
    // QUESTION: the pinned StartRequest names no holder log, and WmiStarter always writes the
    // box's `board-holder-ssh.log` into the request and its "exited at once" text, so another
    // host's start cannot yet name `board-holder-ssh.<project>.log` (keeper.md, The handshake,
    // steps 1 and 7). Interim: the request is v0.06's for every host, until the starter is given
    // the host's log by an architect's pin.
    let req = StartRequest {
        state_dir: ctx.state_dir,
        profile: ctx.profile,
        ssh_exe: ctx.ssh_exe,
        lease_id: lease.id,
        alias: lease.alias,
        own_exe: ctx.own_exe,
    };
    let holder = match deps.starter.start(&req, deps.matcher) {
        Started::Holder(h) => h,
        // `Started::Failed` is the whole `reason=` text, `The holder could not be started: `
        // included; only the refusals above add that prefix themselves (architect, 2026-11-19,
        // for E0.06-T01-S06).
        Started::Failed(reason) => return failed(None, &reason),
    };
    if let Err(e) = deps.records.write_host(ctx.state_dir, who.files, &holder) {
        // An unrecorded holder could never be stopped later, so it is stopped at once. A stop
        // that found `All`, `Gone` or `TreeLeft` counts as the holder gone; after anything else
        // it runs unrecorded (architect, 2026-11-19, for E0.06-T01-S06).
        let gone = matches!(
            deps.processes.stop(&holder, ctx.ssh_exe, deps.matcher),
            Stopped::All | Stopped::Gone | Stopped::TreeLeft(_)
        );
        let reason = if gone {
            format!(
                "The record could not be written, so the holder was stopped, or had exited: {e}"
            )
        } else {
            format!(
                "The record could not be written, and the holder could not be stopped, so it runs unrecorded: {e}"
            )
        };
        return failed(Some(holder.pid), &reason);
    }
    Outcome {
        line: Some(who.line(
            time,
            "started",
            lease.id,
            lease.alias,
            &holder.pid.to_string(),
            true,
            None,
        )),
        action: Action::Started(holder),
        exit_code: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_refusals_before_a_start_come_in_their_order() {
        let base = std::env::temp_dir().join(format!(
            "agentspace-keeper-unit-refusal-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let ssh = base.join("ssh.exe");
        std::fs::write(&ssh, b"stand-in").unwrap();
        let missing = base.join("missing.exe");
        let quoted = base.join("a\"b");
        let own = base.join("keeper.exe");
        let own_quoted = base.join("k\"eeper.exe");
        let ctx = |ssh_exe: &'static Path, state: &'static Path, own: &'static Path| PassContext {
            state_dir: state,
            profile: state,
            ssh_exe,
            own_exe: own,
            now: None,
            tracked: None,
        };
        let leak = |p: &Path| -> &'static Path { Box::leak(p.to_path_buf().into_boxed_path()) };
        let (ssh, missing, quoted, own, own_quoted, base_p) = (
            leak(&ssh),
            leak(&missing),
            leak(&quoted),
            leak(&own),
            leak(&own_quoted),
            leak(&base),
        );
        let rel = leak(Path::new("ssh.exe"));
        assert_eq!(
            refusal(&ctx(rel, quoted, own_quoted), "-bad"),
            Some("SshExe 'ssh.exe' is not a full path.".to_string())
        );
        assert_eq!(
            refusal(&ctx(missing, quoted, own_quoted), "-bad"),
            Some(format!("SshExe '{}' does not exist.", missing.display()))
        );
        assert_eq!(
            refusal(&ctx(ssh, quoted, own_quoted), "-bad"),
            Some(format!(
                "The state folder '{}' holds a quote.",
                quoted.display()
            ))
        );
        assert_eq!(
            refusal(&ctx(ssh, base_p, own_quoted), "-bad"),
            Some(format!(
                "This program's path '{}' holds a quote.",
                own_quoted.display()
            ))
        );
        assert_eq!(
            refusal(&ctx(ssh, base_p, own), "-bad"),
            Some("Alias '-bad' is not a host alias.".to_string())
        );
        assert_eq!(
            refusal(&ctx(ssh, base_p, own), "agentspace-demo-11111111"),
            None
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The alias refusal alone, with every other check passing: a state folder and a program
    /// path with no quote, and an `-SshExe` that is a file. No lease state that passes its schema
    /// reaches this refusal, so it is tested here (architect, 2026-11-19, for E0.06-T01-S06; the
    /// cases are the test author's, moved from `tests/start_refusals.rs`).
    fn alias_refusal(alias: &str) -> Option<String> {
        let tmp = tempfile::Builder::new()
            .prefix("agentspace-keeper-unit-alias-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let root = tmp
            .path()
            .canonicalize()
            .expect("The throwaway folder has a full path");
        let state = root.join("state");
        std::fs::create_dir_all(&state).unwrap();
        let ssh = root.join("ssh-stub");
        std::fs::write(&ssh, b"stub").unwrap();
        let own_exe = root.join("bin").join("agentspace-keeper");
        refusal_before_start(&state, &own_exe, &ssh, alias)
    }

    #[test]
    fn an_alias_that_matches_the_pattern_is_not_refused() {
        let longest = format!("a{}", "b".repeat(127));
        assert_eq!(longest.len(), 128);
        for alias in [
            "a",
            "A",
            "0",
            "agentspace-demo-3f2a9c1e",
            "a.b_c-d",
            "Z9",
            longest.as_str(),
        ] {
            assert_eq!(alias_refusal(alias), None, "for {alias:?}");
        }
    }

    #[test]
    fn an_alias_that_does_not_match_is_refused_with_its_own_text() {
        let too_long = format!("a{}", "b".repeat(128));
        assert_eq!(too_long.len(), 129);
        for alias in [
            "",
            "-x",
            ".x",
            "_x",
            "a b",
            "a/b",
            "a:b",
            "a\"b",
            "\u{e9}x",
            "x\u{e9}",
            "ab\n",
            "\nab",
            too_long.as_str(),
        ] {
            let got = alias_refusal(alias).unwrap_or_else(|| panic!("{alias:?} was not refused"));
            assert!(
                got.ends_with(&format!("Alias '{alias}' is not a host alias.")),
                "for {alias:?}: {got:?}"
            );
        }
    }

    #[test]
    fn a_host_alias_is_keeper_mds_pattern() {
        assert!(is_host_alias("agentspace-swimcoach-0b6f4c1e"));
        assert!(is_host_alias("A"));
        assert!(is_host_alias(&format!("a{}", "b".repeat(127))));
        assert!(!is_host_alias(&format!("a{}", "b".repeat(128))));
        assert!(!is_host_alias(""));
        assert!(!is_host_alias("-a"));
        assert!(!is_host_alias(".a"));
        assert!(!is_host_alias("a b"));
        assert!(!is_host_alias("a\n"));
        assert!(!is_host_alias("ä"));
    }
}
