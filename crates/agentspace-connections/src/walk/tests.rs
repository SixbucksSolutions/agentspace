//! The walk's rules over a stand-in process table and clock (driver.md, Ending a lease's
//! connections, How it is checked).

use super::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

const ALIAS: &str = "agentspace-foo-1234abcd";
const SSH: &str = "C:\\Windows\\System32\\OpenSSH\\ssh.exe";
const PROXY: &str = "C:\\Tools\\agentspace\\agentspace-ssh-proxy.exe";
const OWN: u32 = 9000;
const ACCESS_DENIED: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    List,
    Open(u32, Access),
    /// A handle closed: its process, and whether it was opened with the right to terminate.
    Close(u32, bool),
    Terminate(u32, u32),
    Wait(u32),
}

#[derive(Clone)]
struct Proc {
    /// The parent ID the list gives.
    listed_parent: u32,
    /// The parent ID its own handle gives.
    parent: u32,
    created: u64,
    user: Option<bool>,
    image: Option<String>,
    cmd: Option<String>,
    alive: bool,
    /// The time it exits at by itself, or after a termination.
    exits_at: Option<Duration>,
    open_error: Option<u32>,
    terminate_open_error: Option<u32>,
    /// How many reads of its creation time fail, from the first.
    created_errors: u32,
    terminate_error: Option<u32>,
    /// It exits as it is opened with the right to terminate it.
    exits_on_terminate_open: bool,
    /// Its command line as read on a handle opened to terminate it, when that differs.
    cmd_on_terminate_handle: Option<Option<String>>,
    /// Whether a termination ends it.
    dies: bool,
    /// A process it starts as it is terminated, before it dies.
    spawns: Option<(u32, Box<Proc>)>,
}

fn proc(parent: u32, created: u64) -> Proc {
    Proc {
        listed_parent: parent,
        parent,
        created,
        user: Some(true),
        image: Some("C:\\Tools\\other.exe".to_string()),
        cmd: Some("other.exe".to_string()),
        alive: true,
        exits_at: None,
        open_error: None,
        terminate_open_error: None,
        created_errors: 0,
        terminate_error: None,
        exits_on_terminate_open: false,
        cmd_on_terminate_handle: None,
        dies: true,
        spawns: None,
    }
}

fn ssh(parent: u32, created: u64, args: &str) -> Proc {
    Proc {
        image: Some(SSH.to_string()),
        cmd: Some(format!("\"{SSH}\" {args}")),
        ..proc(parent, created)
    }
}

fn client(parent: u32, created: u64) -> Proc {
    ssh(parent, created, ALIAS)
}

fn proxy(parent: u32, created: u64, args: &str) -> Proc {
    Proc {
        image: Some(PROXY.to_string()),
        cmd: Some(format!("{PROXY} {args}")),
        ..proc(parent, created)
    }
}

struct State {
    procs: BTreeMap<u32, Proc>,
    now: Duration,
    events: Vec<Event>,
    /// The error each call of `list` gives, in turn; `None` reads the list.
    list_errors: Vec<Option<u32>>,
    lists: usize,
    /// The handles open now.
    live_handles: i64,
}

impl State {
    fn alive(&self, pid: u32) -> bool {
        self.procs
            .get(&pid)
            .is_some_and(|p| p.alive && p.exits_at.is_none_or(|t| self.now < t))
    }
}

#[derive(Clone)]
struct Fake(Rc<RefCell<State>>);

/// A handle on one process, by its ID and creation time; dropping it records its close.
struct FakeHandle {
    pid: u32,
    created: u64,
    terminate: bool,
    state: Rc<RefCell<State>>,
}

impl Drop for FakeHandle {
    fn drop(&mut self) {
        let mut s = self.state.borrow_mut();
        s.events.push(Event::Close(self.pid, self.terminate));
        s.live_handles -= 1;
    }
}

impl Fake {
    fn new(procs: Vec<(u32, Proc)>) -> Fake {
        let mut map: BTreeMap<u32, Proc> = procs.into_iter().collect();
        // The walking process itself, under a parent that is not a client.
        map.entry(OWN).or_insert_with(|| proc(8000, 500));
        map.entry(8000).or_insert_with(|| proc(4, 100));
        Fake(Rc::new(RefCell::new(State {
            procs: map,
            now: Duration::ZERO,
            events: Vec::new(),
            list_errors: Vec::new(),
            lists: 0,
            live_handles: 0,
        })))
    }

    fn events(&self) -> Vec<Event> {
        self.0.borrow().events.clone()
    }

    fn terminated(&self) -> Vec<u32> {
        self.events()
            .iter()
            .filter_map(|e| match e {
                Event::Terminate(pid, _) => Some(*pid),
                _ => None,
            })
            .collect()
    }

    fn now(&self) -> Duration {
        self.0.borrow().now
    }

    fn run(&self) -> Result<usize> {
        end_clients(self, self, ALIAS, &|_| true)
    }

    /// The process with this handle's ID, when it is still the one the handle was opened on.
    fn with<R>(&self, h: &FakeHandle, f: impl FnOnce(&State, &Proc) -> R) -> Option<R> {
        let s = self.0.borrow();
        let p = s.procs.get(&h.pid)?;
        (p.created == h.created).then(|| f(&s, p))
    }
}

impl Clock for Fake {
    fn now(&self) -> Duration {
        self.0.borrow().now
    }
}

impl ProcessTable for Fake {
    type Handle = FakeHandle;

    fn list(&self) -> std::result::Result<Vec<Entry>, u32> {
        let mut s = self.0.borrow_mut();
        s.events.push(Event::List);
        let i = s.lists;
        s.lists += 1;
        if let Some(Some(e)) = s.list_errors.get(i) {
            return Err(*e);
        }
        let pids: Vec<u32> = s.procs.keys().copied().collect();
        Ok(pids
            .into_iter()
            .filter(|pid| s.alive(*pid))
            .map(|pid| Entry {
                pid,
                parent: s.procs[&pid].listed_parent,
            })
            .collect())
    }

    fn own_pid(&self) -> u32 {
        OWN
    }

    fn open(&self, pid: u32, access: Access) -> std::result::Result<FakeHandle, u32> {
        let mut s = self.0.borrow_mut();
        s.events.push(Event::Open(pid, access));
        if !s.alive(pid) {
            return Err(ERROR_INVALID_PARAMETER);
        }
        let p = s
            .procs
            .get_mut(&pid)
            .expect("a process that is alive is in the table");
        if let Some(e) = p.open_error {
            return Err(e);
        }
        if access == Access::Terminate {
            if p.exits_on_terminate_open {
                p.alive = false;
            }
            if let Some(e) = p.terminate_open_error {
                return Err(e);
            }
        }
        let created = p.created;
        s.live_handles += 1;
        Ok(FakeHandle {
            pid,
            created,
            terminate: access == Access::Terminate,
            state: Rc::clone(&self.0),
        })
    }

    fn exited(&self, h: &FakeHandle) -> std::result::Result<bool, u32> {
        Ok(!self.with(h, |s, _| s.alive(h.pid)).unwrap_or(false))
    }

    fn wait_exit(&self, h: &FakeHandle, timeout: Duration) -> std::result::Result<bool, u32> {
        self.0.borrow_mut().events.push(Event::Wait(h.pid));
        if self.exited(h)? {
            return Ok(true);
        }
        let at = self.with(h, |_, p| p.exits_at).flatten();
        let mut s = self.0.borrow_mut();
        match at {
            Some(t) if t <= s.now + timeout => {
                s.now = s.now.max(t);
                Ok(true)
            }
            _ => {
                s.now += timeout;
                Ok(false)
            }
        }
    }

    fn created(&self, h: &FakeHandle) -> std::result::Result<u64, u32> {
        let mut s = self.0.borrow_mut();
        match s.procs.get_mut(&h.pid) {
            Some(p) if p.created == h.created && p.created_errors > 0 => {
                p.created_errors -= 1;
                Err(ACCESS_DENIED)
            }
            _ => Ok(h.created),
        }
    }

    fn parent_pid(&self, h: &FakeHandle) -> std::result::Result<u32, u32> {
        self.with(h, |_, p| p.parent).ok_or(ERROR_INVALID_PARAMETER)
    }

    fn is_own_user(&self, h: &FakeHandle) -> Option<bool> {
        self.with(h, |_, p| p.user).flatten()
    }

    fn image_path(&self, h: &FakeHandle) -> Option<String> {
        self.with(h, |_, p| p.image.clone()).flatten()
    }

    fn command_line(&self, h: &FakeHandle) -> Option<String> {
        self.with(h, |s, p| {
            if !s.alive(h.pid) {
                return None;
            }
            match (&p.cmd_on_terminate_handle, h.terminate) {
                (Some(other), true) => other.clone(),
                _ => p.cmd.clone(),
            }
        })
        .flatten()
    }

    fn terminate(&self, h: &FakeHandle, exit_code: u32) -> std::result::Result<(), u32> {
        assert!(
            h.terminate,
            "terminated through a handle opened without the right"
        );
        let mut s = self.0.borrow_mut();
        s.events.push(Event::Terminate(h.pid, exit_code));
        let now = s.now;
        let p = s.procs.get_mut(&h.pid).expect("a handle's process");
        if let Some(e) = p.terminate_error {
            return Err(e);
        }
        let spawn = p.spawns.take();
        if p.dies {
            p.exits_at = Some(p.exits_at.map_or(now, |t| t.min(now)) + Duration::from_millis(50));
        }
        if let Some((pid, child)) = spawn {
            let mut child = *child;
            child.created = now.as_millis() as u64 + 10_000;
            s.procs.insert(pid, child);
        }
        Ok(())
    }
}

fn id(pid: u32, created: u64) -> Identity {
    Identity { pid, created }
}

/// Every close comes after the last termination and wait: every handle is held until the call
/// ends (driver.md, Each with its tree).
fn assert_handles_held(fake: &Fake) {
    let events = fake.events();
    let Some(last) = events
        .iter()
        .rposition(|e| matches!(e, Event::Terminate(..) | Event::Wait(_)))
    else {
        return;
    };
    let ended = fake.terminated();
    for (i, e) in events.iter().enumerate() {
        if let Event::Close(pid, true) = e
            && ended.contains(pid)
        {
            assert!(i > last, "a tree's handle was closed before the call ended");
        }
    }
}

#[test]
fn a_client_is_ended_with_its_tree_leaves_first_and_exit_code_1() {
    // ssh 100, its ProxyCommand 110 (a client itself), and what that runs, 120 and 130.
    let fake = Fake::new(vec![
        (100, client(8000, 1000)),
        (110, proxy(100, 1100, ALIAS)),
        (120, proc(110, 1200)),
        (130, proc(120, 1300)),
    ]);
    assert_eq!(fake.run(), Ok(1));
    let ends: Vec<Event> = fake
        .events()
        .into_iter()
        .filter(|e| matches!(e, Event::Terminate(..)))
        .collect();
    assert_eq!(
        ends,
        vec![
            Event::Terminate(130, 1),
            Event::Terminate(120, 1),
            Event::Terminate(110, 1),
            Event::Terminate(100, 1),
        ]
    );
    assert_handles_held(&fake);
}

#[test]
fn every_handle_is_closed_by_the_return() {
    let fake = Fake::new(vec![
        (100, client(8000, 1000)),
        (110, proc(100, 1100)),
        (
            200,
            Proc {
                terminate_error: Some(ACCESS_DENIED),
                ..client(8000, 2000)
            },
        ),
    ]);
    assert!(fake.run().is_err());
    let s = fake.0.borrow();
    assert!(s.events.iter().any(|e| matches!(e, Event::Close(..))));
    assert_eq!(s.live_handles, 0);
}

#[test]
fn two_clients_are_counted_and_descendants_never() {
    let fake = Fake::new(vec![
        (100, client(8000, 1000)),
        (104, proc(100, 1100)),
        (200, proxy(8000, 2000, ALIAS)),
        (204, proc(200, 2100)),
        (208, proc(204, 2200)),
    ]);
    assert_eq!(fake.run(), Ok(2));
    let mut t = fake.terminated();
    t.sort_unstable();
    assert_eq!(t, vec![100, 104, 200, 204, 208]);
}

#[test]
fn no_client_is_ok_0_and_nothing_is_ended() {
    let fake = Fake::new(vec![
        (100, ssh(8000, 1000, "agentspace-foo-1234abcd0")),
        (104, ssh(8000, 1000, "-J agentspace-foo-1234abcd other")),
        (108, proxy(8000, 1000, "agentspace-foo-1234abcd extra")),
        (
            112,
            Proc {
                image: Some("C:\\Tools\\scp.exe".to_string()),
                cmd: Some(format!("scp.exe {ALIAS}:file .")),
                ..proc(8000, 1000)
            },
        ),
        (
            116,
            Proc {
                image: Some("C:\\Tools\\claude-ssh-proxy.exe".to_string()),
                cmd: Some(format!("claude-ssh-proxy.exe {ALIAS}")),
                ..proc(8000, 1000)
            },
        ),
    ]);
    assert_eq!(fake.run(), Ok(0));
    assert!(fake.terminated().is_empty());
}

#[test]
fn the_image_name_is_read_from_the_full_path_not_argv_0() {
    let fake = Fake::new(vec![
        // Runs another program while its first token says ssh.exe: not a client.
        (
            100,
            Proc {
                image: Some("C:\\Tools\\other.exe".to_string()),
                cmd: Some(format!("ssh.exe {ALIAS}")),
                ..proc(8000, 1000)
            },
        ),
        // Runs SSH.EXE from anywhere while its first token names another program: a client.
        (
            104,
            Proc {
                image: Some("D:\\Somewhere Else\\SSH.EXE".to_string()),
                cmd: Some(format!("whatever.exe -v {}", ALIAS.to_uppercase())),
                ..proc(8000, 1000)
            },
        ),
        // An image that cannot be read: not a client, not reported.
        (
            108,
            Proc {
                image: None,
                ..client(8000, 1000)
            },
        ),
    ]);
    assert_eq!(fake.run(), Ok(1));
    assert_eq!(fake.terminated(), vec![104]);
}

#[test]
fn another_users_process_is_never_a_client() {
    let fake = Fake::new(vec![
        (
            100,
            Proc {
                user: Some(false),
                ..client(8000, 1000)
            },
        ),
        (
            104,
            Proc {
                user: None,
                ..client(8000, 1000)
            },
        ),
    ]);
    assert_eq!(fake.run(), Ok(0));
    assert!(fake.terminated().is_empty());
    assert!(!fake.events().contains(&Event::Open(100, Access::Terminate)));
}

#[test]
fn a_command_line_that_cannot_be_read_is_not_a_client() {
    let fake = Fake::new(vec![(
        100,
        Proc {
            cmd: None,
            ..client(8000, 1000)
        },
    )]);
    assert_eq!(fake.run(), Ok(0));
    assert!(fake.terminated().is_empty());
}

#[test]
fn a_refused_termination_is_not_ended_and_every_other_client_is() {
    let fake = Fake::new(vec![
        (
            100,
            Proc {
                terminate_error: Some(ACCESS_DENIED),
                ..client(8000, 1000)
            },
        ),
        (200, client(8000, 2000)),
    ]);
    assert_eq!(
        fake.run(),
        Err(EndError::NotAllEnded {
            ended: 1,
            not_ended: vec![id(100, 1000)],
        })
    );
    assert!(fake.terminated().contains(&200));
    assert!(fake.now() <= DEADLINE);
}

#[test]
fn a_client_that_cannot_be_opened_to_end_it_is_not_ended() {
    let fake = Fake::new(vec![(
        100,
        Proc {
            terminate_open_error: Some(ACCESS_DENIED),
            ..client(8000, 1000)
        },
    )]);
    assert_eq!(
        fake.run(),
        Err(EndError::NotAllEnded {
            ended: 0,
            not_ended: vec![id(100, 1000)],
        })
    );
}

#[test]
fn a_refusal_because_it_is_exiting_that_then_exits_is_neither_failure_nor_count() {
    let fake = Fake::new(vec![(
        100,
        Proc {
            terminate_error: Some(ACCESS_DENIED),
            exits_at: Some(Duration::from_millis(300)),
            ..client(8000, 1000)
        },
    )]);
    assert_eq!(fake.run(), Ok(0));
}

#[test]
fn a_client_that_exits_during_the_walk_is_neither_failure_nor_count() {
    let fake = Fake::new(vec![
        // Exits as it is opened to end it: the open succeeds on a process that has exited.
        (
            100,
            Proc {
                exits_on_terminate_open: true,
                ..client(8000, 1000)
            },
        ),
        // Exits as it is opened to end it, and the open fails.
        (
            104,
            Proc {
                exits_on_terminate_open: true,
                terminate_open_error: Some(ERROR_INVALID_PARAMETER),
                ..client(8000, 1000)
            },
        ),
        // Gone before the call opens it.
        (
            108,
            Proc {
                exits_at: Some(Duration::ZERO),
                ..client(8000, 1000)
            },
        ),
        (200, client(8000, 2000)),
    ]);
    assert_eq!(fake.run(), Ok(1));
    assert_eq!(fake.terminated(), vec![200]);
}

#[test]
fn a_client_that_changed_by_the_end_handle_is_not_ended() {
    let fake = Fake::new(vec![(
        100,
        Proc {
            cmd_on_terminate_handle: Some(Some(format!("{SSH} other-host"))),
            ..client(8000, 1000)
        },
    )]);
    assert_eq!(fake.run(), Ok(0));
    assert!(fake.terminated().is_empty());
}

#[test]
fn a_list_that_cannot_be_read_ends_nothing() {
    let fake = Fake::new(vec![(100, client(8000, 1000))]);
    fake.0.borrow_mut().list_errors = vec![Some(ACCESS_DENIED)];
    assert_eq!(
        fake.run(),
        Err(EndError::ListUnreadable {
            windows_error: ACCESS_DENIED
        })
    );
    assert_eq!(fake.events(), vec![Event::List]);
}

#[test]
fn a_list_that_cannot_be_read_in_a_later_round_ends_the_call_with_what_it_has() {
    let fake = Fake::new(vec![(
        100,
        Proc {
            spawns: Some((150, Box::new(proc(100, 0)))),
            ..client(8000, 1000)
        },
    )]);
    fake.0.borrow_mut().list_errors = vec![None, Some(ACCESS_DENIED)];
    assert_eq!(fake.run(), Ok(1));
    assert_eq!(fake.terminated(), vec![100]);
}

#[test]
fn the_own_process_as_a_client_is_spared_and_counted_once() {
    let fake = Fake::new(vec![(OWN, client(8000, 500)), (200, client(8000, 2000))]);
    assert_eq!(
        fake.run(),
        Err(EndError::NotAllEnded {
            ended: 1,
            not_ended: vec![id(OWN, 500)],
        })
    );
    assert_eq!(fake.terminated(), vec![200]);
}

#[test]
fn a_spared_ancestor_and_its_tree_are_left_whole() {
    // 7000 (a client) runs 7100 (a proxy client) and 8000, the walking process's parent; 7200 is
    // beside 8000, cannot be opened, and runs a client 7300, which the walk up from 7300 cannot
    // trace to 7000. 6000 is 7000's parent, not a client. 200 is an unrelated client.
    let fake = Fake::new(vec![
        (6000, proc(4, 10)),
        (7000, client(6000, 50)),
        (7100, proxy(7000, 60, ALIAS)),
        (8000, proc(7000, 100)),
        (
            7200,
            Proc {
                open_error: Some(ACCESS_DENIED),
                ..proc(7000, 120)
            },
        ),
        (7300, client(7200, 130)),
        (200, client(6000, 2000)),
    ]);
    assert_eq!(
        fake.run(),
        Err(EndError::NotAllEnded {
            ended: 1,
            not_ended: vec![id(7000, 50)],
        })
    );
    assert_eq!(fake.terminated(), vec![200]);
}

#[test]
fn an_ancestor_that_cannot_be_opened_is_spared_and_so_is_a_client_above_it() {
    // 8000, the walking process's parent, cannot be opened: it is treated as an ancestor, and the
    // walk up stops there. Its own parent 7000 is a client the walk up never reaches; its tree
    // holds 8000, so it is an ancestor too, and is never ended.
    let fake = Fake::new(vec![
        (7000, client(4, 50)),
        (
            8000,
            Proc {
                open_error: Some(ACCESS_DENIED),
                ..proc(7000, 100)
            },
        ),
        (200, client(4, 2000)),
    ]);
    assert_eq!(
        fake.run(),
        Err(EndError::NotAllEnded {
            ended: 1,
            not_ended: vec![id(7000, 50)],
        })
    );
    assert_eq!(fake.terminated(), vec![200]);
}

#[test]
fn a_client_inside_the_tree_of_a_client_found_to_be_an_ancestor_is_never_ended() {
    // 7000 is a client above 8000, which cannot be opened, so the walk up stops at 8000 and 7000
    // is found to be an ancestor only by its tree. 700 is a client under 7200, which cannot be
    // opened either, so it is counted on its own; it is in 7000's tree, and MUST NOT be ended
    // whichever of the two the walk looks at first (700 is listed before 7000).
    let fake = Fake::new(vec![
        (7000, client(4, 50)),
        (
            8000,
            Proc {
                open_error: Some(ACCESS_DENIED),
                ..proc(7000, 100)
            },
        ),
        (
            7200,
            Proc {
                open_error: Some(ACCESS_DENIED),
                ..proc(7000, 120)
            },
        ),
        (700, client(7200, 130)),
        (200, client(4, 2000)),
    ]);
    assert_eq!(
        fake.run(),
        Err(EndError::NotAllEnded {
            ended: 1,
            not_ended: vec![id(7000, 50)],
        })
    );
    assert_eq!(fake.terminated(), vec![200]);
}

#[test]
fn a_client_above_an_ancestor_whose_creation_time_cannot_be_read_is_never_ended() {
    // 8000's creation time cannot be read on the walk up, which stops there; it reads later. The
    // client 7000 above it holds 8000 and the walking process in its tree, and is never ended.
    let fake = Fake::new(vec![
        (7000, client(4, 50)),
        (
            8000,
            Proc {
                created_errors: 1,
                ..proc(7000, 100)
            },
        ),
    ]);
    assert_eq!(
        fake.run(),
        Err(EndError::NotAllEnded {
            ended: 0,
            not_ended: vec![id(7000, 50)],
        })
    );
    assert!(fake.terminated().is_empty());
}

#[test]
fn an_ancestor_whose_creation_time_cannot_be_read_is_spared() {
    // 8000 is a client whose creation time cannot be read on the ancestor walk; it is spared by
    // its ID, and its tree beside the walking process is never ended.
    let fake = Fake::new(vec![(
        8000,
        Proc {
            created_errors: 1,
            ..client(4, 100)
        },
    )]);
    assert_eq!(
        fake.run(),
        Err(EndError::NotAllEnded {
            ended: 0,
            not_ended: vec![id(8000, 100)],
        })
    );
    assert!(fake.terminated().is_empty());
}

#[test]
fn a_parent_created_after_its_child_is_not_its_ancestor() {
    // The walking process's parent ID 8000 was reused by a client created after it: that client
    // is not an ancestor, and is ended.
    let fake = Fake::new(vec![(8000, client(4, 900))]);
    assert_eq!(fake.run(), Ok(1));
    assert_eq!(fake.terminated(), vec![8000]);
}

#[test]
fn a_reused_parent_id_is_not_a_descendant() {
    // 110 lists 100 as its parent but was created before it: its parent ID was reused. It is not
    // in 100's tree, and MUST NOT be ended.
    let fake = Fake::new(vec![
        (100, client(8000, 1000)),
        (110, proc(100, 900)),
        (120, proc(100, 1100)),
    ]);
    assert_eq!(fake.run(), Ok(1));
    let mut t = fake.terminated();
    t.sort_unstable();
    assert_eq!(t, vec![100, 120]);
}

#[test]
fn a_descendant_that_fails_its_checks_is_left_alone() {
    let fake = Fake::new(vec![
        (100, client(8000, 1000)),
        // Listed under 100 but its own handle names another parent.
        (
            110,
            Proc {
                parent: 300,
                ..proc(100, 1100)
            },
        ),
        // Cannot be opened.
        (
            120,
            Proc {
                terminate_open_error: Some(ACCESS_DENIED),
                ..proc(100, 1200)
            },
        ),
        // Under 120, so it cannot be traced.
        (130, proc(120, 1300)),
    ]);
    assert_eq!(fake.run(), Ok(1));
    assert_eq!(fake.terminated(), vec![100]);
}

#[test]
fn a_child_made_meanwhile_is_ended_in_a_later_round_and_rounds_stop_at_3() {
    // Each of 110, 120 and 130 starts a child as it is terminated, so every round finds one more.
    let p140 = proc(130, 0);
    let p130 = Proc {
        spawns: Some((140, Box::new(p140))),
        ..proc(120, 0)
    };
    let p120 = Proc {
        spawns: Some((130, Box::new(p130))),
        ..proc(110, 0)
    };
    let p110 = Proc {
        spawns: Some((120, Box::new(p120))),
        ..proc(100, 1100)
    };
    let fake = Fake::new(vec![(100, client(8000, 1000)), (110, p110)]);
    assert_eq!(fake.run(), Ok(1));
    // Round 1: 110, then the client 100; round 2: 120; round 3: 130. 140 is never reached.
    assert_eq!(fake.terminated(), vec![110, 100, 120, 130]);
    let lists = fake
        .events()
        .iter()
        .filter(|e| matches!(e, Event::List))
        .count();
    assert_eq!(lists, 3);
}

#[test]
fn a_client_still_running_at_the_deadline_is_not_ended_and_the_call_returns_by_it() {
    let fake = Fake::new(vec![
        (
            100,
            Proc {
                dies: false,
                ..client(8000, 1000)
            },
        ),
        (
            200,
            Proc {
                dies: false,
                ..client(8000, 2000)
            },
        ),
        (300, client(8000, 3000)),
    ]);
    assert_eq!(
        fake.run(),
        Err(EndError::NotAllEnded {
            ended: 1,
            not_ended: vec![id(100, 1000), id(200, 2000)],
        })
    );
    // The spec's 10 seconds, written out, so that the constant cannot drift from it.
    assert_eq!(fake.now(), Duration::from_secs(10));
}

#[test]
fn the_deadline_counts_from_the_calls_start() {
    let fake = Fake::new(vec![(
        100,
        Proc {
            dies: false,
            ..client(8000, 1000)
        },
    )]);
    fake.0.borrow_mut().now = Duration::from_secs(100);
    assert!(fake.run().is_err());
    assert_eq!(fake.now(), Duration::from_secs(110));
}

#[test]
fn a_client_inside_another_clients_tree_is_ended_once_and_not_counted() {
    // ssh 100 runs the proxy 110, which runs a nested ssh 120 to the same alias.
    let fake = Fake::new(vec![
        (100, client(8000, 1000)),
        (110, proxy(100, 1100, ALIAS)),
        (120, client(110, 1200)),
    ]);
    assert_eq!(fake.run(), Ok(1));
    assert_eq!(fake.terminated(), vec![120, 110, 100]);
}

#[test]
fn a_client_scope_refuses_a_path_outside_it() {
    let fake = Fake::new(vec![
        (100, client(8000, 1000)),
        (
            200,
            Proc {
                image: Some("C:\\Throwaway\\ssh.exe".to_string()),
                ..client(8000, 2000)
            },
        ),
    ]);
    let r = end_clients(&fake, &fake, ALIAS, &|image| {
        image.starts_with("C:\\Throwaway\\")
    });
    assert_eq!(r, Ok(1));
    assert_eq!(fake.terminated(), vec![200]);
}
