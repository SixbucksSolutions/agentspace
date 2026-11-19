//! One pass (docs/keeper.md, A pass; Before the start; Log lines).

use crate::Deps;
use crate::args::check_ssh_exe;
use crate::log::{Logger, pass_line, rotate_ssh_log};
use agentspace_keeper_api::{HolderRecord, Query, StartRequest, Started, Stopped, Taken};
use agentspace_state::{Record, StoreError};
use std::io::Write;
use std::path::Path;

/// What one pass is run with.
#[derive(Debug, Clone, Copy)]
pub struct PassContext<'a> {
    /// The state folder, a full path, which exists.
    pub state_dir: &'a Path,
    /// The keeper's `%USERPROFILE%`, a full path: the holder's working folder.
    pub profile: &'a Path,
    /// `-SshExe`, or its default.
    pub ssh_exe: &'a Path,
    /// The keeper's own program.
    pub own_exe: &'a Path,
    /// `-Now`, as a Unix time in whole seconds; `None` takes the current UTC second.
    pub now: Option<i64>,
    /// The holder a keeper tracks (keeper.md, Watching): its PID, creation time, lease ID and
    /// alias, as the keeper holds them. `None` in a one-pass run, which tracks nothing.
    pub tracked: Option<&'a HolderRecord>,
}

/// What a pass did, by its action line, for the exit code and for a keeper's tracking
/// (keeper.md, Watching, "What the pass did").
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
    /// `idle`.
    Idle,
    /// `skipped`: another pass held `board-holder.lock`.
    Skipped,
    /// `failed`, from a start, the checks before it included; the holder's PID when it started
    /// but could not be recorded.
    StartFailed(Option<u32>),
    /// `failed` before the pass decided anything: the lease state could not be read, or another
    /// error of the pass.
    Failed,
}

/// The outcome of a pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassReport {
    /// What the pass did.
    pub action: Action,
    /// The one-pass run's exit code (keeper.md, Exit codes): 0 when the pass ran or was skipped,
    /// 1 when the lease state could not be read, the pass met an error it does not handle, or its
    /// own line could not be logged.
    pub exit_code: i32,
}

/// The decision of step 7, with the line that says it.
struct Outcome {
    action: Action,
    exit_code: i32,
    line: String,
}

/// The pass time, fixed when the pass begins: as a Unix time, and as every line writes it.
struct PassTime {
    secs: i64,
    text: String,
}

/// Runs one pass (keeper.md, The order of a pass), logging its lines to `board-holder.log` in
/// the state folder and to `stdout`.
pub fn run_pass(ctx: &PassContext, deps: &Deps, stdout: &mut dyn Write) -> PassReport {
    // The pass time is fixed when the pass begins, before the lock is taken.
    let secs = ctx
        .now
        .unwrap_or_else(|| deps.clock.utc_now_ms().div_euclid(1000));
    let time = PassTime {
        secs,
        text: agentspace_state::format_time(secs),
    };
    let mut log = Logger::new(ctx.state_dir, deps.clock, stdout);

    // Step 1. A pass that cannot take the lock logs `skipped` and ends.
    let guard = match deps.locks.pass(ctx.state_dir) {
        Taken::Held(guard) => Some(guard),
        // The pin gives `Busy` for the keeper lock only; from the pass lock it is another taker
        // holding it, and is read as `Skipped` (architect, 2026-11-19, for E0.06-T01-S06).
        Taken::Skipped | Taken::Busy(_) => None,
        Taken::Failed(e) => {
            let outcome = pass_error(&time.text, &e);
            return finish(outcome, &time.text, &mut log);
        }
    };
    let outcome = match guard {
        None => Outcome {
            action: Action::Skipped,
            exit_code: 0,
            line: pass_line(&time.text, "skipped", "", "", "", Some("locked")),
        },
        Some(_) => locked_pass(ctx, deps, &time, &mut log),
    };
    let report = finish(outcome, &time.text, &mut log);
    // Step 8: the lock is released once the pass's line is logged.
    drop(guard);
    report
}

/// Logs the pass's own line. A pass whose own line is dropped ends as a pass that met an error,
/// and tries once to log that in its place; what the pass did stands.
fn finish(outcome: Outcome, time: &str, log: &mut Logger) -> PassReport {
    match log.log(&outcome.line) {
        Ok(()) => PassReport {
            action: outcome.action,
            exit_code: outcome.exit_code,
        },
        Err(e) => {
            let reason =
                format!("The pass stopped on an error: board-holder.log could not be written: {e}");
            let _ = log.log(&pass_line(time, "failed", "", "", "", Some(&reason)));
            PassReport {
                action: outcome.action,
                exit_code: 1,
            }
        }
    }
}

/// A `failed` line for an error the pass does not handle, with exit 1.
fn pass_error(time: &str, error: &str) -> Outcome {
    Outcome {
        action: Action::Failed,
        exit_code: 1,
        line: pass_line(
            time,
            "failed",
            "",
            "",
            "",
            Some(&format!("The pass stopped on an error: {error}")),
        ),
    }
}

fn holder_line(time: &str, action: &str, r: &HolderRecord, reason: Option<&str>) -> String {
    pass_line(
        time,
        action,
        &r.lease_id,
        &r.alias,
        &r.pid.to_string(),
        reason,
    )
}

/// Steps 2 to 7.
fn locked_pass(ctx: &PassContext, deps: &Deps, when: &PassTime, log: &mut Logger) -> Outcome {
    let time = when.text.as_str();
    // Step 2: a matcher that fails its own cases trusts nothing.
    if let Err(cases) = deps.matcher.self_check() {
        // `self_check`'s error is the failing cases alone (`<case>; <case>`), and the pass adds
        // the words before them (architect, 2026-11-19, for E0.06-T01-S06).
        return pass_error(
            time,
            &format!("The holder command matcher failed its own checks: {cases}"),
        );
    }

    // Step 3: a failure to rotate, or to log the rotation, does not stop the pass.
    if rotate_ssh_log(ctx.state_dir) {
        let _ = log.log(&pass_line(time, "rotated", "", "", "", None));
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
            return Outcome {
                action: Action::Failed,
                exit_code: 1,
                line: pass_line(
                    time,
                    "failed",
                    "",
                    "",
                    "",
                    Some(&format!("Lease state could not be read: {why}")),
                ),
            };
        }
    };

    // Step 5.
    let ready: Vec<Ready> = leases
        .iter()
        .filter_map(|r| Ready::from(r, when.secs))
        .collect();

    // Step 6: a record that names no trusted holder is removed, and its process left alone.
    let trusted = match deps.records.read(ctx.state_dir) {
        None => None,
        Some(record) => {
            if is_trusted(&record, ctx.ssh_exe, deps) {
                Some(record)
            } else {
                if let Err(e) = deps.records.remove(ctx.state_dir) {
                    // An untrusted record that cannot be removed is an error the pass does not
                    // handle: this line and exit 1 (keeper.md, Exit codes; architect, 2026-11-19,
                    // for E0.06-T01-S06).
                    return pass_error(time, &format!("The record could not be removed: {e}"));
                }
                None
            }
        }
    };

    // Step 7: the first row that applies.
    if let Some(record) = trusted {
        // The record's lease_id is compared with a lease's ID without regard to ASCII case
        // (state.md, The holder record; architect, 2026-11-19); the alias is compared exactly.
        let keep = ready
            .iter()
            .any(|l| l.id.eq_ignore_ascii_case(&record.lease_id) && l.alias == record.alias);
        if keep {
            return Outcome {
                line: holder_line(time, "kept", &record, None),
                action: Action::Kept(record),
                exit_code: 0,
            };
        }
        return stop(ctx, deps, time, record);
    }
    if let Some(t) = ctx.tracked {
        return Outcome {
            line: holder_line(time, "lost", t, None),
            action: Action::Lost(t.clone()),
            exit_code: 0,
        };
    }
    match choose(&ready) {
        None => Outcome {
            action: Action::Idle,
            exit_code: 0,
            line: pass_line(time, "idle", "", "", "", None),
        },
        Some(lease) => start(ctx, deps, time, lease),
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
/// (keeper.md, Which lease).
fn choose<'a, 'b>(ready: &'b [Ready<'a>]) -> Option<&'b Ready<'a>> {
    ready.iter().min_by(|a, b| {
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

/// Stops a trusted holder whose lease is not ready, or whose alias changed, and starts nothing
/// (keeper.md, Stopping a holder). The record is removed only when the stop no longer finds the
/// holder, or the holder stopped and only some of its tree did not.
fn stop(ctx: &PassContext, deps: &Deps, time: &str, record: HolderRecord) -> Outcome {
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
        let _ = deps.records.remove(ctx.state_dir);
    }
    match reason {
        None => Outcome {
            line: holder_line(time, "stopped", &record, None),
            action: Action::Stopped(record),
            exit_code: 0,
        },
        Some(r) => Outcome {
            line: holder_line(time, "failed", &record, Some(&r)),
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

/// Starts a holder through the chosen lease (keeper.md, Starting a holder), records it, and only
/// then says `started`.
fn start(ctx: &PassContext, deps: &Deps, time: &str, lease: &Ready) -> Outcome {
    let failed = |pid: Option<u32>, reason: &str| Outcome {
        action: Action::StartFailed(pid),
        exit_code: 0,
        line: pass_line(
            time,
            "failed",
            lease.id,
            lease.alias,
            &pid.map(|p| p.to_string()).unwrap_or_default(),
            Some(reason),
        ),
    };
    if let Some(why) = refusal(ctx, lease.alias) {
        return failed(None, &format!("The holder could not be started: {why}"));
    }
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
    if let Err(e) = deps.records.write(ctx.state_dir, &holder) {
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
        line: pass_line(
            time,
            "started",
            lease.id,
            lease.alias,
            &holder.pid.to_string(),
            None,
        ),
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
