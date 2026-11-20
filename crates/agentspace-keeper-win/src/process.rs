//! The logic of a query and a stop, over the `Os` seam (keeper.md, Trusting the record; Stopping a
//! holder).
//!
//! It is portable, so its rules are tested with a fake on every platform; `sys` gives it Windows.

#![cfg_attr(not(windows), allow(dead_code))]

use crate::time::filetime_text;
use agentspace_keeper_api::{HolderRecord, Matcher, ProcessInfo, Query, Stopped};
use std::path::Path;
use std::time::Duration;

/// The Windows error `OpenProcess` gives for a PID that no process has.
pub(crate) const ERROR_INVALID_PARAMETER: u32 = 87;

/// One row of the process list: a process and the parent PID Windows lists for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entry {
    pub pid: u32,
    pub parent: u32,
}

/// What the logic asks of the operating system. `Err` is a Windows error number.
pub(crate) trait Os {
    /// An open process handle. Dropping it closes it.
    type Handle;
    /// Opens a process with only the rights to read its limited information and to wait on it,
    /// and with the right to terminate it too when `terminate` is set (keeper.md, Trusting the
    /// record, item 2). The handle MUST NOT be inheritable. The process opened MUST be the one
    /// whose PID is `pid` exactly; a number that is no process's PID is `ERROR_INVALID_PARAMETER`,
    /// even where the system would open another process for it.
    fn open(&self, pid: u32, terminate: bool) -> Result<Self::Handle, u32>;
    /// Whether the process has exited, without waiting.
    fn exited(&self, h: &Self::Handle) -> Result<bool, u32>;
    /// Waits up to `timeout` for the process to exit; whether it has.
    fn wait_exit(&self, h: &Self::Handle, timeout: Duration) -> Result<bool, u32>;
    /// Its creation time, as `FILETIME` ticks in UTC.
    fn created(&self, h: &Self::Handle) -> Result<u64, u32>;
    /// Its command line, read back, or `None` when it cannot be read.
    fn command_line(&self, h: &Self::Handle) -> Option<String>;
    /// Its parent's PID, read on its own handle.
    fn parent_pid(&self, h: &Self::Handle) -> Result<u32, u32>;
    /// The processes running now, each with the parent PID Windows lists for it.
    fn processes(&self) -> Result<Vec<Entry>, u32>;
    /// Terminates the process.
    fn terminate(&self, h: &Self::Handle) -> Result<(), u32>;
    /// The PID of the process this runs in, which a stop never terminates.
    fn own_pid(&self) -> u32;
}

/// How long a stop waits, and how many rounds it walks (keeper.md, Stopping a holder).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    pub per_process: Duration,
    pub rounds: u32,
}

impl Timing {
    pub(crate) const KEEPER: Timing = Timing {
        per_process: crate::STOP_WAIT_PER_PROCESS,
        rounds: crate::STOP_ROUNDS,
    };
}

/// The pinned `Processes::query` (keeper.md, Trusting the record, items 2 and 3): the process is
/// opened without the right to terminate it, and a process that has exited, or that no PID names,
/// is `Gone`.
pub(crate) fn query<O: Os>(os: &O, pid: u32) -> Query {
    let h = match os.open(pid, false) {
        Ok(h) => h,
        Err(ERROR_INVALID_PARAMETER) => return Query::Gone,
        Err(e) => return Query::CannotOpen(e),
    };
    match os.exited(&h) {
        Ok(true) => return Query::Gone,
        Ok(false) => {}
        // It was opened but could not be read, which a trust check reads as item 2 failing.
        // A process that opens but whose exit state or creation time cannot be read is
        // `CannotOpen` with that Windows error, so it is not trusted (keeper.md, Stopping a
        // holder; architect, 2026-11-19, for E0.06-T02-S03).
        Err(e) => return Query::CannotOpen(e),
    }
    match os.created(&h) {
        Ok(ticks) => Query::Running(ProcessInfo {
            process_started_at: filetime_text(ticks),
            command_line: os.command_line(&h),
        }),
        Err(e) => Query::CannotOpen(e),
    }
}

/// The new look of keeper.md's stop table: the trust check's items 2 to 4, on a fresh handle.
fn new_look_finds<O: Os>(
    os: &O,
    holder: &HolderRecord,
    ssh_exe: &Path,
    matcher: &dyn Matcher,
) -> bool {
    match query(os, holder.pid) {
        Query::Running(info) => {
            info.process_started_at == holder.process_started_at
                && info
                    .command_line
                    .as_deref()
                    .is_some_and(|cl| matcher.is_holder(cl, ssh_exe, &holder.alias))
        }
        Query::Gone | Query::CannotOpen(_) => false,
    }
}

/// One process of the holder's tree, held open until the stop ends.
struct Node<H> {
    pid: u32,
    created: u64,
    depth: usize,
    handle: H,
    /// Terminated without an error.
    terminated: bool,
    /// The error of its last terminate, while it has not been terminated.
    terminate_error: Option<u32>,
}

/// The pinned `Processes::stop` (keeper.md, Stopping a holder).
///
/// The holder is opened by PID with the right to terminate it, and its creation time and command
/// line are checked again on that handle, by the same rule as the trust check. Its tree is found by
/// parent PID, each descendant opened and checked on its own handle. Every handle is held until
/// this returns, so no PID in the tree is reused while it is being stopped. Each round terminates
/// the tree leaves first and the holder last, then waits for each process to exit.
pub(crate) fn stop<O: Os>(
    os: &O,
    holder: &HolderRecord,
    ssh_exe: &Path,
    matcher: &dyn Matcher,
    timing: Timing,
) -> Stopped {
    let h = match os.open(holder.pid, true) {
        Ok(h) => h,
        Err(e) => {
            return if new_look_finds(os, holder, ssh_exe, matcher) {
                Stopped::CannotOpenToStop(e)
            } else {
                Stopped::Gone
            };
        }
    };
    // A holder whose exit state or creation time cannot be read on the handle opened to stop it
    // is "The stop met an error" (`Error`), and the record is kept (keeper.md, Stopping a holder;
    // architect, 2026-11-19).
    match os.exited(&h) {
        Ok(true) => return Stopped::Gone,
        Ok(false) => {}
        Err(e) => {
            return Stopped::Error(format!(
                "Windows error {e} while checking whether it was running."
            ));
        }
    }
    let created = match os.created(&h) {
        Ok(t) => t,
        Err(e) => {
            return Stopped::Error(format!(
                "Windows error {e} while reading its creation time."
            ));
        }
    };
    let same = filetime_text(created) == holder.process_started_at
        && os
            .command_line(&h)
            .is_some_and(|cl| matcher.is_holder(&cl, ssh_exe, &holder.alias));
    // The table's "for another reason" row: a holder that fails the check on the handle opened to
    // stop it, while a new look on a fresh handle still trusts it, is `NotStopped` with these
    // words (keeper.md, Stopping a holder; architect, 2026-11-19).
    if !same {
        return if new_look_finds(os, holder, ssh_exe, matcher) {
            Stopped::NotStopped(
                "it did not match the record when checked again on the handle opened to stop it"
                    .to_string(),
            )
        } else {
            Stopped::Gone
        };
    }
    // A stop MUST NOT terminate the keeper's own process, as the holder or in its tree (keeper.md,
    // Stopping a holder; architect, 2026-11-19), which the trust check already makes unreachable.
    if holder.pid == os.own_pid() {
        return Stopped::NotStopped("it is the keeper's own process".to_string());
    }

    let mut tree = vec![Node {
        pid: holder.pid,
        created,
        depth: 0,
        handle: h,
        terminated: false,
        terminate_error: None,
    }];
    for round in 0..timing.rounds {
        let added = match walk(os, &mut tree) {
            Ok(n) => n,
            // A process list that cannot be read is `Error` (keeper.md, Stopping a holder;
            // architect, 2026-11-19). Nothing is terminated when the tree cannot be found: the
            // holder's own children would be orphaned, and the record is kept for the next pass.
            Err(e) if round == 0 => {
                return Stopped::Error(format!(
                    "Windows error {e} while listing processes to find its tree."
                ));
            }
            Err(_) => break,
        };
        let retry = tree
            .iter()
            .any(|n| n.terminate_error.is_some() && !has_exited(os, &n.handle));
        if round > 0 && added == 0 && !retry {
            break;
        }
        let order = leaves_first(&tree);
        for &i in &order {
            let node = &mut tree[i];
            if node.terminated || has_exited(os, &node.handle) {
                continue;
            }
            match os.terminate(&node.handle) {
                Ok(()) => {
                    node.terminated = true;
                    node.terminate_error = None;
                }
                Err(e) => node.terminate_error = Some(e),
            }
        }
        for &i in &order {
            // A wait that fails is read as a process still running, and the outcome says so.
            let _ = os.wait_exit(&tree[i].handle, timing.per_process);
        }
    }

    if !has_exited(os, &tree[0].handle) {
        return match tree[0].terminate_error {
            Some(e) => Stopped::Error(format!("Windows error {e} while terminating it.")),
            None => Stopped::StillRunning,
        };
    }
    let mut left: Vec<u32> = tree[1..]
        .iter()
        .filter(|n| !has_exited(os, &n.handle))
        .map(|n| n.pid)
        .collect();
    left.sort_unstable();
    if left.is_empty() {
        Stopped::All
    } else {
        Stopped::TreeLeft(left)
    }
    // Every handle in `tree` is closed here, when the stop ends.
}

/// Whether a process has exited; a check that fails is read as still running.
fn has_exited<O: Os>(os: &O, h: &O::Handle) -> bool {
    matches!(os.exited(h), Ok(true))
}

/// The tree's indexes, deepest first and the holder last, so every process is terminated after
/// all of its descendants.
fn leaves_first<H>(tree: &[Node<H>]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..tree.len()).collect();
    order.sort_by(|&a, &b| tree[b].depth.cmp(&tree[a].depth).then(b.cmp(&a)));
    order
}

/// Adds every descendant not yet in the tree, found by parent PID from one list of processes, and
/// says how many it added. A process is added only when it can be opened with the right to
/// terminate it, has not exited, gives its parent's PID on its own handle, and was created no
/// earlier than its parent. Anything else is left alone, and so is its subtree, which then cannot
/// be traced. Nothing is ever added by its command line.
fn walk<O: Os>(os: &O, tree: &mut Vec<Node<O::Handle>>) -> Result<usize, u32> {
    let list = os.processes()?;
    let own = os.own_pid();
    let before = tree.len();
    let mut i = 0;
    while i < tree.len() {
        let (parent, parent_created, depth) = (tree[i].pid, tree[i].created, tree[i].depth);
        for e in list.iter().filter(|e| e.parent == parent) {
            if e.pid == own || e.pid == parent || tree.iter().any(|n| n.pid == e.pid) {
                continue;
            }
            if let Some((handle, created)) = check_child(os, e.pid, parent, parent_created) {
                tree.push(Node {
                    pid: e.pid,
                    created,
                    depth: depth + 1,
                    handle,
                    terminated: false,
                    terminate_error: None,
                });
            }
        }
        i += 1;
    }
    Ok(tree.len() - before)
}

/// Opens a listed child and checks it on its own handle (keeper.md, Stopping a holder): its parent
/// PID is its parent's, and it was created no earlier than its parent. While the parent's handle
/// is held its PID cannot be reused, so a process that passes is the parent's child.
fn check_child<O: Os>(
    os: &O,
    pid: u32,
    parent: u32,
    parent_created: u64,
) -> Option<(O::Handle, u64)> {
    let h = os.open(pid, true).ok()?;
    if os.exited(&h).ok()? {
        return None;
    }
    if os.parent_pid(&h).ok()? != parent {
        return None;
    }
    let created = os.created(&h).ok()?;
    if created < parent_created {
        return None;
    }
    Some((h, created))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    const SSH: &str = "C:\\Tools\\ssh.exe";
    const ALIAS: &str = "agentspace-lease";
    const HOLDER_CMD: &str =
        "\"C:\\Tools\\ssh.exe\" -N -o ExitOnForwardFailure=yes agentspace-lease";
    const OWN: u32 = 9000;

    /// A matcher that says yes to exactly `HOLDER_CMD`, for `SSH` and `ALIAS`.
    struct Exact;

    impl Matcher for Exact {
        fn holder_command(&self, _: &Path, _: &str) -> String {
            HOLDER_CMD.to_string()
        }
        fn is_holder(&self, command_line: &str, ssh_exe: &Path, alias: &str) -> bool {
            command_line == HOLDER_CMD && ssh_exe == Path::new(SSH) && alias == ALIAS
        }
        fn self_check(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Event {
        Open(u32, bool),
        Close(u32),
        Terminate(u32),
        Wait(u32),
        List,
    }

    #[derive(Clone)]
    struct Proc {
        /// The parent PID the process list gives.
        listed_parent: u32,
        /// The parent PID its own handle gives.
        parent: u32,
        created: u64,
        cmd: Option<String>,
        alive: bool,
        /// The error an open gives without and with the right to terminate.
        open_error: Option<u32>,
        terminate_open_error: Option<u32>,
        /// The command line read on a handle opened to terminate, when it differs.
        cmd_on_terminate_handle: Option<Option<String>>,
        terminate_error: Option<u32>,
        /// Whether a terminate ends it.
        dies: bool,
        /// A process this one starts when it is terminated, before it dies.
        spawns: Option<(u32, Box<Proc>)>,
    }

    fn proc(parent: u32, created: u64) -> Proc {
        Proc {
            listed_parent: parent,
            parent,
            created,
            cmd: Some("child".to_string()),
            alive: true,
            open_error: None,
            terminate_open_error: None,
            cmd_on_terminate_handle: None,
            terminate_error: None,
            dies: true,
            spawns: None,
        }
    }

    fn holder_proc() -> Proc {
        Proc {
            cmd: Some(HOLDER_CMD.to_string()),
            ..proc(1, HOLDER_TICKS)
        }
    }

    /// 2026-11-19T16:33:05.123Z.
    const HOLDER_TICKS: u64 = (1_795_105_985_123 + 11_644_473_600_000) * 10_000;
    const HOLDER_AT: &str = "2026-11-19T16:33:05.123Z";

    fn record(pid: u32) -> HolderRecord {
        HolderRecord {
            pid,
            process_started_at: HOLDER_AT.to_string(),
            lease_id: "lease".to_string(),
            alias: ALIAS.to_string(),
        }
    }

    #[derive(Default)]
    struct Fake {
        procs: RefCell<BTreeMap<u32, Proc>>,
        events: Rc<RefCell<Vec<Event>>>,
        list_error: Option<u32>,
        walks: RefCell<u32>,
    }

    struct H {
        pid: u32,
        terminate: bool,
        events: Rc<RefCell<Vec<Event>>>,
    }

    impl Drop for H {
        fn drop(&mut self) {
            self.events.borrow_mut().push(Event::Close(self.pid));
        }
    }

    impl Fake {
        fn with(procs: &[(u32, Proc)]) -> Fake {
            let f = Fake::default();
            for (pid, p) in procs {
                f.procs.borrow_mut().insert(*pid, p.clone());
            }
            f
        }
        fn events(&self) -> Vec<Event> {
            self.events.borrow().clone()
        }
        fn alive(&self, pid: u32) -> bool {
            self.procs.borrow().get(&pid).is_some_and(|p| p.alive)
        }
        fn terminated(&self) -> Vec<u32> {
            self.events()
                .into_iter()
                .filter_map(|e| match e {
                    Event::Terminate(p) => Some(p),
                    _ => None,
                })
                .collect()
        }
    }

    impl Os for Fake {
        type Handle = H;
        fn open(&self, pid: u32, terminate: bool) -> Result<H, u32> {
            let procs = self.procs.borrow();
            let p = procs.get(&pid).ok_or(ERROR_INVALID_PARAMETER)?;
            if let Some(e) = p.open_error {
                return Err(e);
            }
            if terminate && let Some(e) = p.terminate_open_error {
                return Err(e);
            }
            self.events.borrow_mut().push(Event::Open(pid, terminate));
            Ok(H {
                pid,
                terminate,
                events: self.events.clone(),
            })
        }
        fn exited(&self, h: &H) -> Result<bool, u32> {
            Ok(!self.alive(h.pid))
        }
        fn wait_exit(&self, h: &H, _: Duration) -> Result<bool, u32> {
            self.events.borrow_mut().push(Event::Wait(h.pid));
            Ok(!self.alive(h.pid))
        }
        fn created(&self, h: &H) -> Result<u64, u32> {
            Ok(self.procs.borrow()[&h.pid].created)
        }
        fn command_line(&self, h: &H) -> Option<String> {
            let p = &self.procs.borrow()[&h.pid];
            match (&p.cmd_on_terminate_handle, h.terminate) {
                (Some(c), true) => c.clone(),
                _ => p.cmd.clone(),
            }
        }
        fn parent_pid(&self, h: &H) -> Result<u32, u32> {
            Ok(self.procs.borrow()[&h.pid].parent)
        }
        fn processes(&self) -> Result<Vec<Entry>, u32> {
            *self.walks.borrow_mut() += 1;
            self.events.borrow_mut().push(Event::List);
            if let Some(e) = self.list_error {
                return Err(e);
            }
            Ok(self
                .procs
                .borrow()
                .iter()
                .filter(|(_, p)| p.alive)
                .map(|(&pid, p)| Entry {
                    pid,
                    parent: p.listed_parent,
                })
                .collect())
        }
        fn terminate(&self, h: &H) -> Result<(), u32> {
            self.events.borrow_mut().push(Event::Terminate(h.pid));
            let mut procs = self.procs.borrow_mut();
            let p = procs.get_mut(&h.pid).unwrap();
            if let Some(e) = p.terminate_error {
                return Err(e);
            }
            if p.dies {
                p.alive = false;
            }
            if let Some((pid, child)) = p.spawns.take() {
                procs.insert(pid, *child);
            }
            Ok(())
        }
        fn own_pid(&self) -> u32 {
            OWN
        }
    }

    fn stop_it(f: &Fake, pid: u32) -> Stopped {
        stop(f, &record(pid), Path::new(SSH), &Exact, Timing::KEEPER)
    }

    // Query.

    #[test]
    fn a_query_opens_without_the_right_to_terminate() {
        let f = Fake::with(&[(10, holder_proc())]);
        let q = query(&f, 10);
        assert_eq!(
            q,
            Query::Running(ProcessInfo {
                process_started_at: HOLDER_AT.to_string(),
                command_line: Some(HOLDER_CMD.to_string()),
            })
        );
        assert_eq!(f.events(), vec![Event::Open(10, false), Event::Close(10)]);
    }

    #[test]
    fn a_query_of_no_process_is_gone_and_of_an_exited_one_too() {
        let f = Fake::with(&[(
            10,
            Proc {
                alive: false,
                ..holder_proc()
            },
        )]);
        assert_eq!(query(&f, 11), Query::Gone);
        assert_eq!(query(&f, 10), Query::Gone);
    }

    #[test]
    fn a_query_that_cannot_open_gives_the_windows_error() {
        let f = Fake::with(&[(
            10,
            Proc {
                open_error: Some(5),
                ..holder_proc()
            },
        )]);
        assert_eq!(query(&f, 10), Query::CannotOpen(5));
    }

    #[test]
    fn a_query_gives_no_command_line_when_it_cannot_be_read() {
        let f = Fake::with(&[(
            10,
            Proc {
                cmd: None,
                ..holder_proc()
            },
        )]);
        let Query::Running(info) = query(&f, 10) else {
            panic!("Expected a running process");
        };
        assert_eq!(info.command_line, None);
    }

    // The stop: its tree.

    /// The holder 10, its children 20 and 21, and 20's child 30.
    fn tree() -> Fake {
        Fake::with(&[
            (10, holder_proc()),
            (20, proc(10, HOLDER_TICKS + 1)),
            (21, proc(10, HOLDER_TICKS + 5)),
            (30, proc(20, HOLDER_TICKS + 2)),
            // An unrelated process.
            (40, proc(1, HOLDER_TICKS)),
        ])
    }

    #[test]
    fn the_whole_tree_is_stopped_leaves_first_and_the_holder_last() {
        let f = tree();
        assert_eq!(stop_it(&f, 10), Stopped::All);
        let order = f.terminated();
        assert_eq!(order.len(), 4);
        assert_eq!(order[0], 30, "The deepest first: {order:?}");
        assert_eq!(*order.last().unwrap(), 10, "The holder last: {order:?}");
        assert!(f.alive(40), "An unrelated process was stopped");
    }

    #[test]
    fn every_process_is_opened_with_the_right_to_terminate_and_held_to_the_end() {
        let f = tree();
        assert_eq!(stop_it(&f, 10), Stopped::All);
        let events = f.events();
        let first_close = events
            .iter()
            .position(|e| matches!(e, Event::Close(_)))
            .unwrap();
        let last_action = events
            .iter()
            .rposition(|e| matches!(e, Event::Terminate(_) | Event::Wait(_) | Event::List))
            .unwrap();
        assert!(
            first_close > last_action,
            "A handle was closed before the stop ended: {events:?}"
        );
        for pid in [10, 20, 21, 30] {
            assert!(
                events.contains(&Event::Open(pid, true)),
                "{pid} not opened to stop"
            );
            assert!(events.contains(&Event::Close(pid)), "{pid} never closed");
        }
    }

    #[test]
    fn a_child_created_before_its_parent_is_left_alone() {
        // 20 is listed under the holder but was created before it: a PID reused.
        let f = Fake::with(&[(10, holder_proc()), (20, proc(10, HOLDER_TICKS - 1))]);
        assert_eq!(stop_it(&f, 10), Stopped::All);
        assert!(f.alive(20));
        assert_eq!(f.terminated(), vec![10]);
    }

    #[test]
    fn a_child_created_at_the_same_tick_as_its_parent_is_stopped() {
        let f = Fake::with(&[(10, holder_proc()), (20, proc(10, HOLDER_TICKS))]);
        assert_eq!(stop_it(&f, 10), Stopped::All);
        assert!(!f.alive(20));
    }

    #[test]
    fn a_child_whose_own_handle_gives_another_parent_is_left_alone() {
        let f = Fake::with(&[
            (10, holder_proc()),
            (
                20,
                Proc {
                    parent: 11,
                    ..proc(10, HOLDER_TICKS + 1)
                },
            ),
        ]);
        assert_eq!(stop_it(&f, 10), Stopped::All);
        assert!(f.alive(20));
    }

    #[test]
    fn a_child_that_cannot_be_opened_is_left_alone_with_its_subtree_and_not_reported() {
        let f = Fake::with(&[
            (10, holder_proc()),
            (
                20,
                Proc {
                    terminate_open_error: Some(5),
                    ..proc(10, HOLDER_TICKS + 1)
                },
            ),
            (30, proc(20, HOLDER_TICKS + 2)),
        ]);
        assert_eq!(stop_it(&f, 10), Stopped::All);
        assert!(f.alive(20));
        assert!(f.alive(30));
    }

    #[test]
    fn a_process_with_the_holder_command_but_another_parent_is_never_touched() {
        let f = Fake::with(&[
            (10, holder_proc()),
            (50, holder_proc()), // The same command line, under PID 1.
        ]);
        assert_eq!(stop_it(&f, 10), Stopped::All);
        assert!(f.alive(50));
        assert!(!f.events().contains(&Event::Open(50, true)));
    }

    #[test]
    fn the_stop_never_terminates_its_own_process() {
        let f = Fake::with(&[(10, holder_proc()), (OWN, proc(10, HOLDER_TICKS + 1))]);
        assert_eq!(stop_it(&f, 10), Stopped::All);
        assert!(f.alive(OWN));
    }

    #[test]
    fn a_child_made_during_the_stop_is_found_by_the_next_round() {
        // Terminating 20 makes the holder start 21 before 20 dies.
        let mut c20 = proc(10, HOLDER_TICKS + 1);
        c20.spawns = Some((21, Box::new(proc(10, HOLDER_TICKS + 3))));
        let f = Fake::with(&[(10, holder_proc()), (20, c20)]);
        assert_eq!(stop_it(&f, 10), Stopped::All);
        assert!(!f.alive(21));
        assert_eq!(
            *f.walks.borrow(),
            3,
            "One walk per round, then one finding nothing new"
        );
    }

    #[test]
    fn the_rounds_are_bounded_at_five() {
        // A holder that survives, and makes a new child every time it is terminated.
        let f = Fake::with(&[(
            10,
            Proc {
                dies: false,
                ..holder_proc()
            },
        )]);
        // Each terminate of 10 adds a fresh child for the next round.
        struct Spawner<'a>(&'a Fake);
        impl Os for Spawner<'_> {
            type Handle = H;
            fn open(&self, pid: u32, t: bool) -> Result<H, u32> {
                self.0.open(pid, t)
            }
            fn exited(&self, h: &H) -> Result<bool, u32> {
                self.0.exited(h)
            }
            fn wait_exit(&self, h: &H, d: Duration) -> Result<bool, u32> {
                self.0.wait_exit(h, d)
            }
            fn created(&self, h: &H) -> Result<u64, u32> {
                self.0.created(h)
            }
            fn command_line(&self, h: &H) -> Option<String> {
                self.0.command_line(h)
            }
            fn parent_pid(&self, h: &H) -> Result<u32, u32> {
                self.0.parent_pid(h)
            }
            fn processes(&self) -> Result<Vec<Entry>, u32> {
                let n = *self.0.walks.borrow();
                self.0
                    .procs
                    .borrow_mut()
                    .insert(100 + n, proc(10, HOLDER_TICKS + 1));
                self.0.processes()
            }
            fn terminate(&self, h: &H) -> Result<(), u32> {
                self.0.terminate(h)
            }
            fn own_pid(&self) -> u32 {
                OWN
            }
        }
        let s = Spawner(&f);
        let got = stop(&s, &record(10), Path::new(SSH), &Exact, Timing::KEEPER);
        assert_eq!(got, Stopped::StillRunning);
        assert_eq!(*f.walks.borrow(), 5);
    }

    // The stop: each row of keeper.md's table.

    #[test]
    fn a_holder_gone_before_the_stop_is_gone_and_nothing_is_terminated() {
        let f = Fake::with(&[(20, proc(10, HOLDER_TICKS + 1))]);
        assert_eq!(stop_it(&f, 10), Stopped::Gone);
        assert!(f.terminated().is_empty());
    }

    #[test]
    fn a_holder_that_has_exited_but_is_still_open_is_gone() {
        let f = Fake::with(&[
            (
                10,
                Proc {
                    alive: false,
                    ..holder_proc()
                },
            ),
            (20, proc(10, HOLDER_TICKS + 1)),
        ]);
        assert_eq!(stop_it(&f, 10), Stopped::Gone);
        assert!(
            f.alive(20),
            "Nothing is stopped when the holder ended by itself"
        );
    }

    #[test]
    fn a_holder_that_cannot_be_opened_to_stop_and_still_runs_gives_the_error() {
        let f = Fake::with(&[(
            10,
            Proc {
                terminate_open_error: Some(5),
                ..holder_proc()
            },
        )]);
        assert_eq!(stop_it(&f, 10), Stopped::CannotOpenToStop(5));
        assert!(f.terminated().is_empty());
    }

    #[test]
    fn a_holder_that_cannot_be_opened_even_to_query_it_is_gone() {
        let f = Fake::with(&[(
            10,
            Proc {
                open_error: Some(5),
                terminate_open_error: Some(5),
                ..holder_proc()
            },
        )]);
        assert_eq!(stop_it(&f, 10), Stopped::Gone);
    }

    #[test]
    fn the_pid_reuse_guard_another_creation_time_is_gone_and_left_alone() {
        let f = Fake::with(&[(
            10,
            Proc {
                created: HOLDER_TICKS + 10_000,
                ..holder_proc()
            },
        )]);
        assert_eq!(stop_it(&f, 10), Stopped::Gone);
        assert!(f.alive(10));
        assert!(f.terminated().is_empty());
        assert!(!f.events().contains(&Event::List), "Its tree was walked");
    }

    #[test]
    fn another_command_line_is_gone_and_left_alone() {
        let f = Fake::with(&[(
            10,
            Proc {
                cmd: Some("\"C:\\Tools\\ssh.exe\" -N -o ExitOnForwardFailure=yes other".into()),
                ..holder_proc()
            },
        )]);
        assert_eq!(stop_it(&f, 10), Stopped::Gone);
        assert!(f.alive(10));
    }

    #[test]
    fn a_failed_check_on_the_stop_handle_with_a_trusted_new_look_is_not_stopped() {
        let f = Fake::with(&[(
            10,
            Proc {
                cmd_on_terminate_handle: Some(None),
                ..holder_proc()
            },
        )]);
        let Stopped::NotStopped(why) = stop_it(&f, 10) else {
            panic!("Expected NotStopped");
        };
        assert!(!why.is_empty());
        assert!(f.alive(10));
        assert!(f.terminated().is_empty());
    }

    #[test]
    fn a_failed_listing_is_an_error_and_nothing_is_terminated() {
        let mut f = tree();
        f.list_error = Some(8);
        let Stopped::Error(e) = stop_it(&f, 10) else {
            panic!("Expected Error");
        };
        assert!(e.contains("Windows error 8"), "{e}");
        assert!(f.terminated().is_empty());
    }

    #[test]
    fn a_terminate_that_fails_is_an_error() {
        let f = Fake::with(&[(
            10,
            Proc {
                terminate_error: Some(5),
                ..holder_proc()
            },
        )]);
        let Stopped::Error(e) = stop_it(&f, 10) else {
            panic!("Expected Error");
        };
        assert!(e.contains("Windows error 5"), "{e}");
        assert!(f.alive(10));
    }

    #[test]
    fn a_holder_still_running_after_it_was_terminated() {
        let f = Fake::with(&[(
            10,
            Proc {
                dies: false,
                ..holder_proc()
            },
        )]);
        assert_eq!(stop_it(&f, 10), Stopped::StillRunning);
    }

    #[test]
    fn a_holder_stopped_with_some_of_its_tree_left() {
        let f = Fake::with(&[
            (10, holder_proc()),
            (
                30,
                Proc {
                    dies: false,
                    ..proc(10, HOLDER_TICKS + 1)
                },
            ),
            (
                20,
                Proc {
                    dies: false,
                    ..proc(10, HOLDER_TICKS + 1)
                },
            ),
            (25, proc(10, HOLDER_TICKS + 1)),
        ]);
        assert_eq!(stop_it(&f, 10), Stopped::TreeLeft(vec![20, 30]));
    }
}
