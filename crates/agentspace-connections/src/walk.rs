//! The walk of `end_alias_clients`, over the `ProcessTable` seam (docs/driver.md, Ending a lease's
//! connections).
//!
//! RFC 2119 keywords; the capitals are load-bearing.
//!
//! It is portable and holds no `unsafe`, so its rules are tested over a stand-in table and clock on
//! every platform; `windows` gives it Windows.

use crate::{EndError, Identity, Result, argv};
use std::time::{Duration, Instant};

/// One deadline for every wait of a call, counted from the call's start (driver.md, Each with its
/// tree).
pub(crate) const DEADLINE: Duration = Duration::from_secs(10);

/// The rounds of ends, waits and walks a call makes at most (driver.md, Each with its tree).
pub(crate) const ROUNDS: u32 = 3;

/// The exit code a call gives each process it terminates (driver.md, Each with its tree).
pub(crate) const EXIT_CODE: u32 = 1;

/// How long a round waits, at most, for a process whose termination was refused, so that one
/// refused because it is exiting is seen to have exited (driver.md, The count). It is bounded by
/// the deadline as every wait is.
const REFUSED_WAIT: Duration = Duration::from_secs(1);

/// The Windows error `OpenProcess` gives for a process ID that no process has.
pub(crate) const ERROR_INVALID_PARAMETER: u32 = 87;

/// How far up the ancestor walk goes at most, so that a cycle in a stale list cannot hold it.
const MAX_ANCESTORS: usize = 4096;

/// The rights a handle is opened with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Access {
    /// To read the process's limited information, its token's user among it, and to wait on it.
    Query,
    /// A query's rights and the right to terminate it.
    Terminate,
}

/// One row of the process list: a process and the parent process ID the list gives for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entry {
    pub pid: u32,
    pub parent: u32,
}

/// What the walk asks of the operating system. `Err` is a Windows error number.
pub(crate) trait ProcessTable {
    /// An open process handle. Dropping it closes it.
    type Handle;
    /// The processes running now, each with the parent process ID the list gives for it.
    fn list(&self) -> std::result::Result<Vec<Entry>, u32>;
    /// The process ID of the process the walk runs in.
    fn own_pid(&self) -> u32;
    /// Opens the process whose ID is `pid` exactly. The handle MUST NOT be inheritable. A number
    /// that no process has is `ERROR_INVALID_PARAMETER`.
    fn open(&self, pid: u32, access: Access) -> std::result::Result<Self::Handle, u32>;
    /// Whether the process has exited, without waiting.
    fn exited(&self, h: &Self::Handle) -> std::result::Result<bool, u32>;
    /// Waits up to `timeout` for the process to exit, and says whether it has.
    fn wait_exit(&self, h: &Self::Handle, timeout: Duration) -> std::result::Result<bool, u32>;
    /// Its creation time, as FILETIME ticks in UTC.
    fn created(&self, h: &Self::Handle) -> std::result::Result<u64, u32>;
    /// Its parent's process ID, read on its own handle.
    fn parent_pid(&self, h: &Self::Handle) -> std::result::Result<u32, u32>;
    /// Whether its token's user is the walking process's own; `None` when it cannot be read.
    fn is_own_user(&self, h: &Self::Handle) -> Option<bool>;
    /// The full path of the executable it runs; `None` when it cannot be read.
    fn image_path(&self, h: &Self::Handle) -> Option<String>;
    /// Its command line, read from the process; `None` when it cannot be read.
    fn command_line(&self, h: &Self::Handle) -> Option<String>;
    /// Terminates the process with `exit_code`.
    fn terminate(&self, h: &Self::Handle, exit_code: u32) -> std::result::Result<(), u32>;
}

/// The time since some fixed start, for the call's deadline.
pub(crate) trait Clock {
    fn now(&self) -> Duration;
}

/// The monotonic clock.
pub(crate) struct RealClock(Instant);

impl RealClock {
    pub(crate) fn new() -> RealClock {
        RealClock(Instant::now())
    }
}

impl Clock for RealClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

/// The two programs a client can be (driver.md, Which processes are the alias's clients, rule 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ssh,
    Proxy,
}

/// The kind of client the image's file name makes a process, if any: the last part of the full
/// path, compared without regard to ASCII case. The folder does not matter.
fn image_kind(path: &str) -> Option<Kind> {
    let name = path.rsplit(['\\', '/']).next()?;
    if name.eq_ignore_ascii_case("ssh.exe") {
        Some(Kind::Ssh)
    } else if name.eq_ignore_ascii_case("agentspace-ssh-proxy.exe") {
        Some(Kind::Proxy)
    } else {
        None
    }
}

/// Whether the command line names the alias, by rule 3 (driver.md, Which processes are the
/// alias's clients; An ssh client's destination).
fn names_alias(kind: Kind, command_line: &str, alias: &str) -> bool {
    let args = argv::split_command_line(command_line);
    let found = match kind {
        Kind::Ssh => argv::ssh_destination(&args),
        Kind::Proxy => argv::proxy_alias(&args),
    };
    found.is_some_and(|found| argv::is_alias(&found, alias))
}

/// Checks the process on `h` by rules 1 to 3, and gives its creation time when it is a running
/// client. A process that has exited, or whose creation time, image, user or command line cannot
/// be read, is not one.
fn check_client<T: ProcessTable>(
    t: &T,
    h: &T::Handle,
    alias: &str,
    in_scope: &dyn Fn(&str) -> bool,
) -> Option<u64> {
    if t.exited(h).ok()? {
        return None;
    }
    let created = t.created(h).ok()?;
    let image = t.image_path(h)?;
    let kind = image_kind(&image)?;
    if !in_scope(&image) {
        return None;
    }
    if !t.is_own_user(h)? {
        return None;
    }
    let command_line = t.command_line(h)?;
    names_alias(kind, &command_line, alias).then_some(created)
}

fn has_exited<T: ProcessTable>(t: &T, h: &T::Handle) -> bool {
    matches!(t.exited(h), Ok(true))
}

fn listed_parent(list: &[Entry], pid: u32) -> Option<u32> {
    list.iter().find(|e| e.pid == pid).map(|e| e.parent)
}

/// The walking process or one of its ancestors, which the call MUST NOT end. `created` is `None`
/// for an ancestor that could not be checked, which is spared whatever process has its ID.
struct Spared<H> {
    pid: u32,
    created: Option<u64>,
    _handle: Option<H>,
}

/// The walking process and its ancestors, found by parent process ID and each checked to have
/// been created no later than the process below it (driver.md, Which processes are the alias's
/// clients). A process on the walk that cannot be opened, or whose creation time cannot be read,
/// is treated as an ancestor, and the walk stops there. A parent ID that no process has (error
/// 87), or whose process was created after the one below it, ends the walk and is spared nowhere:
/// the parent has exited (architect, 2026-11-22, for v0.08-T02; driver.md at 3bebf43).
fn ancestors<T: ProcessTable>(t: &T, list: &[Entry]) -> Vec<Spared<T::Handle>> {
    let own = t.own_pid();
    let own_handle = t.open(own, Access::Query).ok();
    let own_created = own_handle.as_ref().and_then(|h| t.created(h).ok());
    let mut parent = own_handle
        .as_ref()
        .and_then(|h| t.parent_pid(h).ok())
        .or_else(|| listed_parent(list, own));
    // The walking process's own creation time cannot fail to read in practice; if it does, every
    // parent passes the check, which spares more rather than less.
    let mut below = own_created.unwrap_or(u64::MAX);
    let mut out = vec![Spared {
        pid: own,
        created: own_created,
        _handle: own_handle,
    }];
    while let Some(pid) = parent {
        if pid == 0 || out.len() > MAX_ANCESTORS || out.iter().any(|s| s.pid == pid) {
            break;
        }
        let h = match t.open(pid, Access::Query) {
            Ok(h) => h,
            Err(ERROR_INVALID_PARAMETER) => break,
            Err(_) => {
                out.push(Spared {
                    pid,
                    created: None,
                    _handle: None,
                });
                break;
            }
        };
        let created = match t.created(&h) {
            Ok(c) => c,
            Err(_) => {
                out.push(Spared {
                    pid,
                    created: None,
                    _handle: Some(h),
                });
                break;
            }
        };
        if created > below {
            break;
        }
        parent = t.parent_pid(&h).ok().or_else(|| listed_parent(list, pid));
        out.push(Spared {
            pid,
            created: Some(created),
            _handle: Some(h),
        });
        below = created;
    }
    out
}

fn is_spared<H>(spared: &[Spared<H>], id: Identity) -> bool {
    spared
        .iter()
        .any(|s| s.pid == id.pid && s.created.is_none_or(|c| c == id.created))
}

/// What the walk up from a client finds above it.
struct Above {
    /// The clients among its ancestors, the nearest first.
    clients: Vec<Identity>,
    /// A process that has exited sits between it and the nearest of those clients, so the walk
    /// down from that client never reaches it (driver.md, Each with its tree; D99, #0228).
    cut: bool,
}

/// The clients among a client's ancestors, walked up by parent process ID with the creation checks
/// of the ancestor walk, and whether a program that has exited sits between it and the nearest of
/// them. A client with no client above it is a top (driver.md, Each with its tree); one with a
/// client above it and no such program between is ended in that client's tree and not counted on
/// its own. The walk stops where an ancestor cannot be opened or checked, as the ancestor walk
/// does.
fn walk_up<T: ProcessTable>(
    t: &T,
    list: &[Entry],
    h: &T::Handle,
    id: Identity,
    clients: &[Identity],
) -> Above {
    let mut above = Above {
        clients: Vec::new(),
        cut: false,
    };
    let mut parent = t.parent_pid(h).ok().or_else(|| listed_parent(list, id.pid));
    let mut below = id.created;
    let mut seen = vec![id.pid];
    while let Some(pid) = parent {
        if pid == 0 || seen.len() > MAX_ANCESTORS || seen.contains(&pid) {
            break;
        }
        let Ok(ph) = t.open(pid, Access::Query) else {
            break;
        };
        let Ok(created) = t.created(&ph) else {
            break;
        };
        if created > below {
            break;
        }
        let here = Identity { pid, created };
        if clients.contains(&here) {
            above.clients.push(here);
        } else if above.clients.is_empty() && has_exited(t, &ph) {
            above.cut = true;
        }
        seen.push(pid);
        parent = t.parent_pid(&ph).ok().or_else(|| listed_parent(list, pid));
        below = created;
    }
    above
}

/// Adds to `protected` every process listed under `root` by parent process ID, leaving out only a
/// process known to have been created before its listed parent, whose parent ID was reused. A
/// spared client's tree is left whole (driver.md, Which processes are the alias's clients), so this
/// errs towards sparing. Only a spared client's tree is spared: an ancestor that is not a client,
/// such as the desktop app or a shell, spares nothing (architect, 2026-11-22, for v0.08-T02;
/// driver.md at 3bebf43).
fn protect_tree<T: ProcessTable>(t: &T, list: &[Entry], root: Identity, protected: &mut Vec<u32>) {
    let mut stack = vec![(root.pid, Some(root.created))];
    while let Some((pid, created)) = stack.pop() {
        for e in list.iter().filter(|e| e.parent == pid && e.pid != pid) {
            if protected.contains(&e.pid) {
                continue;
            }
            let child_created = t
                .open(e.pid, Access::Query)
                .ok()
                .and_then(|h| t.created(&h).ok());
            if let (Some(parent), Some(child)) = (created, child_created)
                && child < parent
            {
                continue;
            }
            protected.push(e.pid);
            stack.push((e.pid, child_created));
        }
    }
}

/// One process of a client's tree, held open until the call ends.
struct Node<H> {
    pid: u32,
    created: u64,
    depth: usize,
    handle: H,
    /// Terminated without an error.
    terminated: bool,
    /// Its last termination was refused.
    refused: bool,
}

/// Adds every descendant not yet in the tree, found by parent process ID from one list, and says
/// how many it added (driver.md, Each with its tree; keeper.md, Stopping a holder). A process is
/// added only when it is not protected, can be opened with the right to terminate it, has not
/// exited, gives its parent's ID on its own handle, and was created no earlier than its parent.
/// Anything else is left alone, unreported, and so is its subtree.
fn add_descendants<T: ProcessTable>(
    t: &T,
    list: &[Entry],
    tree: &mut Vec<Node<T::Handle>>,
    protected: &[u32],
) -> usize {
    let before = tree.len();
    let mut i = 0;
    while i < tree.len() {
        let (parent, parent_created, depth) = (tree[i].pid, tree[i].created, tree[i].depth);
        for e in list.iter().filter(|e| e.parent == parent) {
            if e.pid == parent || protected.contains(&e.pid) || tree.iter().any(|n| n.pid == e.pid)
            {
                continue;
            }
            let Ok(h) = t.open(e.pid, Access::Terminate) else {
                continue;
            };
            if !matches!(t.exited(&h), Ok(false)) || t.parent_pid(&h) != Ok(parent) {
                continue;
            }
            let Ok(created) = t.created(&h) else {
                continue;
            };
            if created < parent_created {
                continue;
            }
            tree.push(Node {
                pid: e.pid,
                created,
                depth: depth + 1,
                handle: h,
                terminated: false,
                refused: false,
            });
        }
        i += 1;
    }
    tree.len() - before
}

/// Whether the walking process or one of its ancestors is listed as a child of a process of the
/// tree, and was not created before it: the tree's client is then an ancestor too, above one the
/// walk up could not check, and MUST NOT be ended (driver.md, Each with its tree: "wherever they
/// sit"; architect, 2026-11-22, for v0.08-T02; driver.md at 3bebf43). It is counted once as a
/// client that could not be ended. A child whose creation time cannot be read is taken as one,
/// which errs towards sparing.
fn holds_spared<T: ProcessTable>(
    t: &T,
    list: &[Entry],
    tree: &[Node<T::Handle>],
    spared: &[Spared<T::Handle>],
) -> bool {
    tree.iter().any(|node| {
        list.iter()
            .filter(|e| e.parent == node.pid && e.pid != node.pid)
            .filter(|e| spared.iter().any(|s| s.pid == e.pid))
            .any(|e| {
                let created = t
                    .open(e.pid, Access::Query)
                    .ok()
                    .and_then(|h| t.created(&h).ok());
                created.is_none_or(|c| c >= node.created)
            })
    })
}

/// A top's tree as built, with the clients above the top when it is below a program that has
/// exited (empty for a top with no client above it).
struct Built<H> {
    id: Identity,
    above: Vec<Identity>,
    tree: Vec<Node<H>>,
}

/// The tree's indexes, deepest first and the client last, so every process is terminated after
/// all of its descendants.
fn leaves_first<H>(tree: &[Node<H>]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..tree.len()).collect();
    order.sort_by(|&a, &b| tree[b].depth.cmp(&tree[a].depth).then(b.cmp(&a)));
    order
}

/// `end_alias_clients`'s logic (driver.md, Ending a lease's connections). `in_scope` is given each
/// client's image path, and a process whose path it refuses is not a client.
pub(crate) fn end_clients<T: ProcessTable, C: Clock>(
    t: &T,
    clock: &C,
    alias: &str,
    in_scope: &dyn Fn(&str) -> bool,
) -> Result<usize> {
    let deadline = clock.now() + DEADLINE;
    let list = t
        .list()
        .map_err(|windows_error| EndError::ListUnreadable { windows_error })?;

    // Every handle below is held until the call returns, so no process ID it names is reused
    // meanwhile.
    let spared = ancestors(t, &list);

    // The clients, each found by rules 1 to 3 on a handle that can only read and wait.
    let mut found: Vec<(Identity, T::Handle)> = Vec::new();
    for e in &list {
        if e.pid == 0 {
            continue;
        }
        let Ok(h) = t.open(e.pid, Access::Query) else {
            continue;
        };
        if let Some(created) = check_client(t, &h, alias, in_scope) {
            found.push((
                Identity {
                    pid: e.pid,
                    created,
                },
                h,
            ));
        }
    }
    let clients: Vec<Identity> = found.iter().map(|(id, _)| *id).collect();

    // A client that is the walking process or an ancestor is never ended, and is counted once as
    // one that could not be; its tree is left whole.
    let mut not_ended: Vec<Identity> = Vec::new();
    let mut protected: Vec<u32> = spared.iter().map(|s| s.pid).collect();
    for (id, _) in &found {
        if is_spared(&spared, *id) {
            not_ended.push(*id);
            protect_tree(t, &list, *id, &mut protected);
        }
    }

    // The clients counted on their own, the tops, none inside a spared client's tree: each with no
    // client among its ancestors, and, from v0.12, each below a program that has exited, whose
    // ancestors hold a client that the walk down from it never reaches (D99; #0228). The second
    // kind carries the clients above it, so that one whose ancestors hold a spared client is never
    // a top, whichever way it was found to be spared.
    let mut tops: Vec<(Identity, &T::Handle, Vec<Identity>)> = Vec::new();
    for (id, h) in &found {
        if is_spared(&spared, *id) || protected.contains(&id.pid) {
            continue;
        }
        let above = walk_up(t, &list, h, *id, &clients);
        if above.clients.is_empty() {
            tops.push((*id, h, Vec::new()));
        } else if above.cut
            && !above
                .clients
                .iter()
                .any(|c| is_spared(&spared, *c) || protected.contains(&c.pid))
        {
            tops.push((*id, h, above.clients));
        }
    }

    // Each client is opened by its ID with the right to terminate it, and checked again on that
    // handle, so the process checked is the process ended.
    let mut built: Vec<Built<T::Handle>> = Vec::new();
    let mut spared_trees: Vec<Vec<Node<T::Handle>>> = Vec::new();
    let mut unopened: Vec<(Identity, Vec<Identity>)> = Vec::new();
    for (id, look, above) in tops {
        let h = match t.open(id.pid, Access::Terminate) {
            Ok(h) => h,
            Err(_) => {
                // It could not be opened to end it: gone, or a client that could not be ended.
                if !has_exited(t, look) {
                    unopened.push((id, above));
                }
                continue;
            }
        };
        if check_client(t, &h, alias, in_scope) != Some(id.created) {
            // It exited, or no longer passes the rules: it is not ended, and not reported.
            continue;
        }
        let mut tree = vec![Node {
            pid: id.pid,
            created: id.created,
            depth: 0,
            handle: h,
            terminated: false,
            refused: false,
        }];
        add_descendants(t, &list, &mut tree, &protected);
        built.push(Built { id, above, tree });
    }
    // Every tree is built before any is found to hold a spared process, so that a client inside
    // such a tree is left alone whichever of the two was looked at first.
    let mut holding: Vec<Identity> = Vec::new();
    for Built { id, tree, .. } in &built {
        if holds_spared(t, &list, tree, &spared) {
            // An ancestor the walk up could not reach, above one it could not check: never
            // ended, counted once as a client that could not be, and its tree left whole.
            not_ended.push(*id);
            holding.push(*id);
            protect_tree(t, &list, *id, &mut protected);
        }
    }
    // A top below a program that has exited is one only while no tree built holds it and no
    // client above it was found to be spared: otherwise it is not ended on its own, and not
    // counted.
    let under_spared = |above: &[Identity]| {
        above
            .iter()
            .any(|c| holding.contains(c) || protected.contains(&c.pid))
    };
    for (id, above) in &unopened {
        if !under_spared(above) {
            not_ended.push(*id);
        }
    }
    let held: Vec<bool> = built
        .iter()
        .enumerate()
        .map(|(i, Built { id, above, .. })| {
            !above.is_empty()
                && (under_spared(above)
                    || built.iter().enumerate().any(|(j, Built { tree, .. })| {
                        j != i && tree.iter().any(|n| n.depth > 0 && n.pid == id.pid)
                    }))
        })
        .collect();
    let mut trees: Vec<(Identity, Vec<Node<T::Handle>>)> = Vec::new();
    for (Built { id, mut tree, .. }, held) in built.into_iter().zip(held) {
        if held {
            // Left to the tree that holds it, or to a spared client's tree, and not counted; its
            // handles are held until the call returns, as every handle is.
            not_ended.retain(|n| *n != id);
            spared_trees.push(tree);
        } else if protected.contains(&id.pid) || not_ended.contains(&id) {
            // A spared client's tree, or a client inside one, which is not counted on its own.
            spared_trees.push(tree);
        } else {
            // No process of a spared tree is ended from another tree either.
            tree.retain(|n| n.depth == 0 || !protected.contains(&n.pid));
            trees.push((id, tree));
        }
    }

    for round in 0..ROUNDS {
        if clock.now() >= deadline {
            break;
        }
        if round > 0 {
            // A list that cannot be read in a later round ends the call with what it has.
            let Ok(list) = t.list() else {
                break;
            };
            let added: usize = trees
                .iter_mut()
                .map(|(_, tree)| add_descendants(t, &list, tree, &protected))
                .sum();
            let pending = trees
                .iter()
                .flat_map(|(_, tree)| tree.iter())
                .any(|n| !n.terminated && !has_exited(t, &n.handle));
            if added == 0 && !pending {
                break;
            }
        }
        for (_, tree) in &mut trees {
            for i in leaves_first(tree) {
                let node = &mut tree[i];
                if node.terminated || has_exited(t, &node.handle) {
                    continue;
                }
                match t.terminate(&node.handle, EXIT_CODE) {
                    Ok(()) => {
                        node.terminated = true;
                        node.refused = false;
                    }
                    Err(_) => node.refused = true,
                }
            }
        }
        let nodes = || trees.iter().flat_map(|(_, tree)| tree.iter());
        for node in nodes().filter(|n| n.terminated) {
            let left = deadline.saturating_sub(clock.now());
            if left.is_zero() {
                break;
            }
            if !has_exited(t, &node.handle) {
                let _ = t.wait_exit(&node.handle, left);
            }
        }
        for node in nodes().filter(|n| !n.terminated && n.refused) {
            let left = deadline.saturating_sub(clock.now()).min(REFUSED_WAIT);
            if left.is_zero() {
                break;
            }
            if !has_exited(t, &node.handle) {
                let _ = t.wait_exit(&node.handle, left);
            }
        }
    }

    let mut ended = 0usize;
    for (id, tree) in &trees {
        let client = &tree[0];
        if has_exited(t, &client.handle) {
            if client.terminated {
                ended += 1;
            }
        } else {
            not_ended.push(*id);
        }
    }
    if not_ended.is_empty() {
        Ok(ended)
    } else {
        not_ended.sort_by_key(|id| (id.pid, id.created));
        Err(EndError::NotAllEnded { ended, not_ended })
    }
    // Every handle the call opened is closed here, when it returns.
}

#[cfg(test)]
mod tests;
