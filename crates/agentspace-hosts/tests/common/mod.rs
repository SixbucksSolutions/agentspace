//! Shared fixtures for the black-box tests of agentspace-hosts (docs/state.md, The host registry,
//! The project list, A host's folder). Fixtures only ever name made-up values.
//!
//! Every name of the crate's API that the card leaves open (the paths' functions, the error's
//! variants) is called from this file alone, so that one edit follows a name that differs.
#![allow(dead_code)]

use agentspace_hosts::{
    BOX_PROJECT, BOX_STACK, Failure, Host, HostState, Hosts, HostsError, stack_name,
};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------------------------
// A throwaway home, under the temp folder and removed when dropped.
// ---------------------------------------------------------------------------------------------

pub struct TempHome {
    root: PathBuf,
}

impl TempHome {
    pub fn new() -> TempHome {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root = std::env::temp_dir().join(format!(
            "agentspace-hosts-test-{}-{}-{nanos}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("The throwaway home must be made");
        TempHome { root }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// `%USERPROFILE%\.agentspace`, spelled out here rather than taken from the crate.
    pub fn state(&self) -> PathBuf {
        self.root.join(".agentspace")
    }

    pub fn hosts_json(&self) -> PathBuf {
        self.state().join("hosts.json")
    }

    pub fn projects_json(&self) -> PathBuf {
        self.state().join("projects.json")
    }

    /// Writes `bytes` as hosts.json, making `.agentspace` first.
    pub fn put_hosts(&self, bytes: impl AsRef<[u8]>) {
        std::fs::create_dir_all(self.state()).unwrap();
        std::fs::write(self.hosts_json(), bytes).unwrap();
    }

    /// Writes `bytes` as projects.json, making `.agentspace` first.
    pub fn put_projects(&self, bytes: impl AsRef<[u8]>) {
        std::fs::create_dir_all(self.state()).unwrap();
        std::fs::write(self.projects_json(), bytes).unwrap();
    }

    pub fn hosts_bytes(&self) -> Vec<u8> {
        std::fs::read(self.hosts_json()).unwrap()
    }

    /// The names of the entries directly in `dir`, sorted.
    pub fn names_in(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = match std::fs::read_dir(dir) {
            Ok(rd) => rd
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        };
        v.sort();
        v
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// ---------------------------------------------------------------------------------------------
// Records.
// ---------------------------------------------------------------------------------------------

pub fn box_host() -> Host {
    Host {
        project: BOX_PROJECT.to_string(),
        stack: BOX_STACK.to_string(),
        state: HostState::Ready,
        instance_id: None,
        instance_type: None,
        last_attempt_at: None,
    }
}

pub fn host(project: &str, state: HostState) -> Host {
    Host {
        project: project.to_string(),
        stack: stack_name(project),
        state,
        instance_id: None,
        instance_type: None,
        last_attempt_at: None,
    }
}

pub fn full_host(project: &str, state: HostState, id: &str, at: &str) -> Host {
    Host {
        project: project.to_string(),
        stack: stack_name(project),
        state,
        instance_id: Some(id.to_string()),
        instance_type: None,
        last_attempt_at: Some(at.to_string()),
    }
}

/// The type a missing registry implies for the box, as it is read and never as it is written
/// (docs/state.md, The host registry, "The seed"; pins P2c and P31).
pub const IMPLIED_BOX_TYPE: &str = "m9g.xlarge";

pub fn hosts(records: Vec<Host>) -> Hosts {
    Hosts { hosts: records }
}

/// The seed document's records, as it is WRITTEN: the box's record with no `instance_type`.
pub fn seed() -> Hosts {
    hosts(vec![box_host()])
}

/// What a reader that PLACES a lease reads of a MISSING file (`read_for_placing`): the seed's
/// record with `instance_type` `m9g.xlarge` (from v0.09; docs/state.md, "The seed", pins P2c and
/// P31). A file that exists never reads like this, and a writer never starts from it.
pub fn seed_as_read_for_placing() -> Hosts {
    let mut h = seed();
    h.hosts[0].instance_type = Some(IMPLIED_BOX_TYPE.to_string());
    h
}

/// The box and then the given records.
pub fn with_box(mut rest: Vec<Host>) -> Hosts {
    let mut all = vec![box_host()];
    all.append(&mut rest);
    hosts(all)
}

/// A JSON document for the registry holding the box and one more record, `record` being the
/// text of that record.
pub fn doc_with(record: &str) -> String {
    format!(
        "{{\"version\":0,\"hosts\":[{{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}},{record}]}}"
    )
}

pub const BOX_RECORD: &str =
    "{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}";

// ---------------------------------------------------------------------------------------------
// The errors.
// ---------------------------------------------------------------------------------------------

/// A file at hosts.json or projects.json whose content is not valid.
pub fn is_not_valid(e: &HostsError) -> bool {
    matches!(e, HostsError::NotValid(_))
}

/// A change to the registry that would not be valid, refused before anything is written.
pub fn is_change_not_valid(e: &HostsError) -> bool {
    matches!(e, HostsError::Unwritable(Failure::ChangeNotValid(_)))
}

/// The lock could not be opened for a reason other than its being held.
pub fn is_cannot_lock(e: &HostsError) -> bool {
    matches!(e, HostsError::CannotLock(_))
}

/// A file where agentspace's folder, a host's folder or the home folder belongs.
pub fn is_folder_file(e: &HostsError) -> bool {
    matches!(
        e,
        HostsError::CannotLock(Failure::FolderIsAFile)
            | HostsError::Unreadable(Failure::FolderIsAFile)
            | HostsError::Unwritable(Failure::FolderIsAFile)
            | HostsError::CannotLockRun(Failure::FolderIsAFile)
    )
}

/// A file or folder at hosts.json (or projects.json) that cannot be read as a valid file: state.md
/// treats it as not valid, and the crate MAY tell the two apart.
pub fn is_file_problem(e: &HostsError) -> bool {
    is_not_valid(e) || matches!(e, HostsError::Unreadable(_))
}

/// A refusal of a change that is not valid.
pub fn is_write_refusal(e: &HostsError) -> bool {
    is_change_not_valid(e)
}

/// Any failure to write where nothing is wrong with the change itself.
pub fn is_write_failure(e: &HostsError) -> bool {
    matches!(e, HostsError::Unwritable(_) | HostsError::NotValid(_))
}

/// The message of an error is agentspace's own words: not empty, starts with a capital, and names
/// no path (the home, the temp folder, or any drive path).
pub fn assert_message(err: &HostsError, home: &Path) {
    let msg = err.to_string();
    assert!(!msg.is_empty(), "A message must not be empty");
    let first = msg.chars().next().unwrap();
    assert!(
        first.is_uppercase(),
        "A message starts with a capital, and this one does not: {msg}"
    );
    let home_text = home.to_string_lossy();
    assert!(
        !msg.contains(&*home_text),
        "A message must not name the home folder: {msg}"
    );
    let temp = std::env::temp_dir().to_string_lossy().into_owned();
    assert!(
        !msg.contains(&temp),
        "A message must not name a path in the temp folder: {msg}"
    );
    assert!(
        !msg.contains(":\\") && !msg.contains(":/"),
        "A message must not name a drive path: {msg}"
    );
}

// ---------------------------------------------------------------------------------------------
// The run lock's helper program.
// ---------------------------------------------------------------------------------------------

/// The helper binary: next to the test binary's own folder first, so a test binary built for
/// another target and run elsewhere still finds it, and the path cargo gave at build time next.
pub fn helper_path() -> PathBuf {
    let beside = std::env::current_exe().ok().and_then(|exe| {
        let dir = exe.parent()?.parent()?.to_path_buf();
        let p = dir.join(format!(
            "hosts-run-lock-helper{}",
            std::env::consts::EXE_SUFFIX
        ));
        p.is_file().then_some(p)
    });
    if let Some(p) = beside {
        return p;
    }
    PathBuf::from(env!("CARGO_BIN_EXE_hosts-run-lock-helper"))
}

/// A running `hosts-run-lock-helper`, killed (by its own handle) when dropped if still running.
pub struct Helper {
    child: Child,
    lines: Receiver<String>,
}

impl Helper {
    /// Starts the helper for `project` in `home`, with the four home variables pointed at it.
    pub fn start(home: &Path, project: &str, hold_ms: u64) -> Helper {
        let mut child = Command::new(helper_path())
            .arg(home)
            .arg(project)
            .arg(hold_ms.to_string())
            .env("USERPROFILE", home)
            .env("HOME", home)
            .env("LOCALAPPDATA", home)
            .env("APPDATA", home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("The helper must start");
        let out = child.stdout.take().expect("The helper's stdout is piped");
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Helper { child, lines }
    }

    /// The next line the helper printed, waiting up to `timeout`.
    pub fn next_line(&self, timeout: Duration) -> String {
        match self.lines.recv_timeout(timeout) {
            Ok(l) => l.trim().to_string(),
            Err(RecvTimeoutError::Timeout) => panic!("The helper printed nothing in time"),
            Err(RecvTimeoutError::Disconnected) => panic!("The helper ended without a line"),
        }
    }

    /// Waits up to `timeout` for the helper to exit, and returns its exit code.
    pub fn wait_exit(&mut self, timeout: Duration) -> Option<i32> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait().expect("The helper can be waited on") {
                Some(status) => return status.code(),
                None => {
                    if Instant::now() >= deadline {
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        }
    }

    /// Kills the helper, which this test started, and waits for it to be gone.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            self.kill();
        }
    }
}

/// Calls `take` every 20 ms until it returns true or `deadline` has passed, and says whether it
/// did. A test MUST use it, rather than a single try, for a take that expects a lock to be free
/// again right after its holder dropped it, exited or was killed.
///
/// Why (architect, 2026-11-20, for v0.07-T02): on Linux, a fork made by a test running in
/// parallel holds a duplicate of a lock's descriptor until that child's exec closes it, so one try
/// right after a drop can find the lock held although the code (O_CLOEXEC) is right. A lock that
/// a program the holder started inherited is held for that program's whole life, so a deadline
/// shorter than that program's life still catches it. A check that a lock IS held MUST stay a
/// single immediate try, and does not use this.
pub fn free_within(deadline: Duration, mut take: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    loop {
        if take() {
            return true;
        }
        if started.elapsed() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// How long a take is retried by default: see [`free_within`].
pub const FREE_WITHIN: Duration = Duration::from_secs(2);

/// Polls `condition` until it holds or `timeout` passes, and says whether it held.
pub fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
