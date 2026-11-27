//! The real walk over the laptop's own process table, with stub processes (docs/driver.md,
//! "Ending a lease's connections": "Which processes are the alias's clients", "An ssh client's
//! destination", "Each with its tree", "The count").
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! **Process safety, and the rules every test here keeps:**
//! - **No test calls `end_alias_clients`.** The walk is always `end_alias_clients_under`, over a
//!   throwaway folder under the temp folder that holds the test's copies of the stub, with an alias
//!   built from a UUIDv4 drawn at random for that test (`agentspace-t02test-` and its first 8 hex
//!   digits), and a second random alias for the processes that must not match. No process of
//!   Terry's can have either alias, and none is in the folder.
//! - **Every process a test starts is ended by a drop guard** that holds its handle, whatever the
//!   assertions do, and each test checks at its end that each is gone. A test never ends, opens
//!   or prints a process it did not start: a descendant is opened by the process ID its parent
//!   stub wrote, at once, and held.
//! - **No test prints a command line.** A message names a case by its index and its own inputs.

#![cfg(windows)]

use agentspace_connections::line::release_text;
use agentspace_connections::{EndError, end_alias_clients_under};
use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const STUB: &str = env!("CARGO_BIN_EXE_agentspace-connections-stub");

const ENV_CHILD: &str = "AGENTSPACE_CONN_STUB_CHILD";
const ENV_DEPTH: &str = "AGENTSPACE_CONN_STUB_DEPTH";
const ENV_PIDS: &str = "AGENTSPACE_CONN_STUB_PIDS";
const ENV_CHILD_EXE: &str = "AGENTSPACE_CONN_STUB_CHILD_EXE";
const ENV_CHILD_ARGS: &str = "AGENTSPACE_CONN_STUB_CHILD_ARGS";
const ENV_CHAIN: &str = "AGENTSPACE_CONN_STUB_CHAIN";
const ENV_LABEL: &str = "AGENTSPACE_CONN_STUB_LABEL";
const ENV_MARKS: &str = "AGENTSPACE_CONN_STUB_MARKS";
const ENV_EXIT: &str = "AGENTSPACE_CONN_STUB_EXIT";
const UNIT: char = '\x1f';
const RECORD: char = '\x1e';

/// How long a test waits for a stub's descendants to appear.
const APPEAR: Duration = Duration::from_secs(30);
/// How long a test lets a process take to be gone, once the call has returned.
const GRACE: Duration = Duration::from_secs(2);

/// The few Windows calls the tests need, to hold a stub's descendants by handle.
#[allow(unsafe_code)]
mod win {
    use std::ffi::c_void;

    pub type Handle = *mut c_void;

    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_TIMEOUT: u32 = 258;

    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn TerminateProcess(handle: Handle, code: u32) -> i32;
        fn WaitForSingleObject(handle: Handle, millis: u32) -> u32;
        fn GetExitCodeProcess(handle: Handle, code: *mut u32) -> i32;
    }

    /// A process this test started, held by a handle that keeps its ID from being reused.
    pub struct Held {
        pub pid: u32,
        handle: Handle,
    }

    impl Held {
        /// Opens the process of this ID, which a stub of the test has just written.
        pub fn open(pid: u32) -> Option<Held> {
            // SAFETY: OpenProcess takes plain values and returns a handle or null.
            let handle = unsafe {
                OpenProcess(
                    PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                    0,
                    pid,
                )
            };
            if handle.is_null() {
                None
            } else {
                Some(Held { pid, handle })
            }
        }

        /// Whether the process is still running.
        pub fn alive(&self) -> bool {
            // SAFETY: The handle is open and owned by `self`.
            unsafe { WaitForSingleObject(self.handle, 0) == WAIT_TIMEOUT }
        }

        /// The process's exit code, or None while it runs or when it cannot be read.
        pub fn exit_code(&self) -> Option<u32> {
            if self.alive() {
                return None;
            }
            let mut code = 0u32;
            // SAFETY: The handle is open, and `code` is a valid place for the result.
            let ok = unsafe { GetExitCodeProcess(self.handle, &mut code) };
            if ok == 0 { None } else { Some(code) }
        }

        /// Ends the process, which this test started.
        pub fn terminate(&self) {
            // SAFETY: The handle is open and owned by `self`.
            unsafe {
                TerminateProcess(self.handle, 1);
            }
        }
    }

    impl Drop for Held {
        fn drop(&mut self) {
            // SAFETY: The handle is open, owned by `self`, and closed once.
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

use win::Held;

/// 16 random bytes: the process's random hash keys, mixed with the clock, the process ID and a
/// counter, so that two tests never draw alike.
fn random_bytes() -> [u8; 16] {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut out = [0u8; 16];
    for chunk in out.chunks_mut(8) {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
        hasher.write_u128(nanos);
        hasher.write_u32(std::process::id());
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    out
}

/// A UUIDv4 drawn at random, in its text form.
fn random_uuid_v4() -> String {
    let mut b = random_bytes();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        hex[0..4].concat(),
        hex[4..6].concat(),
        hex[6..8].concat(),
        hex[8..10].concat(),
        hex[10..16].concat()
    )
}

/// An alias built from a lease ID drawn at random.
fn random_alias() -> String {
    format!("agentspace-t02test-{}", &random_uuid_v4()[..8])
}

/// One stub the test started, and the descendants its chain made.
struct Started {
    label: String,
    root: Child,
    desc: Vec<Held>,
    pidfile: PathBuf,
    /// The folder where the stub's labelled processes write their own process IDs.
    marks: PathBuf,
}

/// A test's throwaway folder, its two aliases and every process it started.
struct Lab {
    dir: PathBuf,
    alias: String,
    other: String,
    started: Vec<Started>,
}

/// A stub and its descendants, as a case names them: the image name, the arguments, how many
/// levels of descendants, and the first level's program and arguments when they are not the
/// stub's own.
struct Spec<'a> {
    image: &'a str,
    args: &'a [&'a str],
    depth: u32,
    child: Option<(&'a str, &'a [&'a str])>,
    /// The programs started one below the other after the plain levels, each below the one before.
    chain: Vec<Node<'a>>,
    /// Variables the stub's whole tree inherits, besides the stub's own.
    env: Vec<(String, String)>,
}

/// One program of a chain: its label, whether it exits as soon as it has started what it starts,
/// how many plain levels of descendants it starts below itself, its image and its arguments.
struct Node<'a> {
    label: &'a str,
    exits: bool,
    depth: u32,
    image: &'a str,
    args: &'a [&'a str],
}

fn spec<'a>(image: &'a str, args: &'a [&'a str]) -> Spec<'a> {
    Spec {
        image,
        args,
        depth: 1,
        child: None,
        chain: Vec::new(),
        env: Vec::new(),
    }
}

impl Lab {
    fn new() -> Lab {
        let alias = random_alias();
        let mut other = random_alias();
        while other == alias {
            other = random_alias();
        }
        let dir = std::env::temp_dir().join(format!("agentspace-t02test-{}", random_uuid_v4()));
        fs::create_dir_all(&dir).expect("The throwaway folder");
        // The full path as Windows names the processes' images: long names, no `\\?\` prefix.
        let canonical = fs::canonicalize(&dir).expect("The throwaway folder's path");
        let text = canonical.to_string_lossy().to_string();
        let dir = PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text));
        Lab {
            dir,
            alias,
            other,
            started: Vec::new(),
        }
    }

    /// A second throwaway folder beside this one, with the same two aliases. Its name is this
    /// one's with `x` after it, so it starts with this one's name as text and is still another
    /// folder.
    fn beside(&self) -> Lab {
        let dir = PathBuf::from(format!("{}x", self.dir.to_string_lossy()));
        fs::create_dir_all(&dir).expect("The second throwaway folder");
        Lab {
            dir,
            alias: self.alias.clone(),
            other: self.other.clone(),
            started: Vec::new(),
        }
    }

    /// Fills `<a>` (the alias), `<A>` (it in capitals), `<b>` (the other alias) and `<t>` (the
    /// alias less its last character).
    fn fill(&self, text: &str) -> String {
        text.replace("<a>", &self.alias)
            .replace("<A>", &self.alias.to_ascii_uppercase())
            .replace("<b>", &self.other)
            .replace("<t>", &self.alias[..self.alias.len() - 1])
    }

    /// The copy of the stub under this image name, made the first time it is asked for.
    fn copy_of(&self, image: &str) -> PathBuf {
        let path = self.dir.join(image);
        if !path.exists() {
            fs::copy(STUB, &path).expect("The stub's copy");
        }
        path
    }

    /// Starts the stub and waits until every descendant of its chain exists and is held.
    fn start(&mut self, spec: Spec<'_>) -> usize {
        let exe = self.copy_of(spec.image);
        let index = self.started.len();
        let pidfile = self.dir.join(format!("pids-{index}.txt"));
        let marks = self.dir.join(format!("marks-{index}"));
        fs::create_dir_all(&marks).expect("The marks folder");
        let args: Vec<String> = spec.args.iter().map(|a| self.fill(a)).collect();
        let mut command = Command::new(&exe);
        command
            .args(&args)
            .env_remove(ENV_CHILD)
            .env(ENV_DEPTH, spec.depth.to_string())
            .env(ENV_PIDS, &pidfile)
            .env(ENV_MARKS, &marks)
            .env_remove(ENV_LABEL)
            .env_remove(ENV_EXIT)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some((image, child_args)) = spec.child {
            let child_args: Vec<String> = child_args.iter().map(|a| self.fill(a)).collect();
            command
                .env(ENV_CHILD_EXE, self.copy_of(image))
                .env(ENV_CHILD_ARGS, child_args.join("\u{1f}"));
        } else {
            command.env_remove(ENV_CHILD_EXE).env_remove(ENV_CHILD_ARGS);
        }
        let mut expected = spec.depth as usize;
        if spec.chain.is_empty() {
            command.env_remove(ENV_CHAIN);
        } else {
            let records: Vec<String> = spec
                .chain
                .iter()
                .map(|node| {
                    let mut fields = vec![
                        node.label.to_string(),
                        if node.exits { "1" } else { "0" }.to_string(),
                        node.depth.to_string(),
                        self.copy_of(node.image).to_string_lossy().into_owned(),
                    ];
                    fields.extend(node.args.iter().map(|a| self.fill(a)));
                    fields.join(&UNIT.to_string())
                })
                .collect();
            command.env(ENV_CHAIN, records.join(&RECORD.to_string()));
            expected += spec
                .chain
                .iter()
                .map(|n| 1 + n.depth as usize)
                .sum::<usize>();
        }
        for (name, value) in &spec.env {
            command.env(name, value);
        }
        let root = command.spawn().expect("The stub starts");
        self.started.push(Started {
            label: format!("#{index} {} with {} arguments", spec.image, args.len()),
            root,
            desc: Vec::new(),
            pidfile,
            marks,
        });
        self.hold_descendants(index, expected);
        index
    }

    fn hold_descendants(&mut self, index: usize, expected: usize) {
        let begun = Instant::now();
        loop {
            let s = &mut self.started[index];
            if let Ok(text) = fs::read_to_string(&s.pidfile) {
                for line in text.split_terminator('\n') {
                    let Ok(pid) = line.trim().parse::<u32>() else {
                        continue;
                    };
                    if s.desc.iter().any(|h| h.pid == pid) {
                        continue;
                    }
                    let held = Held::open(pid).expect("A descendant of the stub is opened");
                    s.desc.push(held);
                }
            }
            if s.desc.len() >= expected {
                return;
            }
            if let Ok(Some(status)) = s.root.try_wait() {
                panic!("{} exited early with {:?}", s.label, status.code());
            }
            if begun.elapsed() > APPEAR {
                panic!("The descendants of {} did not appear", s.label);
            }
            sleep(Duration::from_millis(50));
        }
    }

    fn spawn(&mut self, image: &str, args: &[&str]) -> usize {
        self.start(spec(image, args))
    }

    /// The real walk over this lab's folder, for this lab's alias, and how long it took.
    fn run(&self) -> Result<usize, EndError> {
        self.run_for(&self.alias)
    }

    fn run_for(&self, alias: &str) -> Result<usize, EndError> {
        let begun = Instant::now();
        let result = end_alias_clients_under(alias, &self.dir);
        assert!(
            begun.elapsed() < Duration::from_secs(10),
            "The call did not return inside its 10 seconds"
        );
        result
    }

    fn root_alive(&mut self, index: usize) -> bool {
        matches!(self.started[index].root.try_wait(), Ok(None))
    }

    /// Whether the stub and all its descendants are gone, within the grace.
    fn tree_gone(&mut self, index: usize) -> bool {
        let begun = Instant::now();
        loop {
            let gone =
                !self.root_alive(index) && self.started[index].desc.iter().all(|h| !h.alive());
            if gone {
                return true;
            }
            if begun.elapsed() > GRACE {
                return false;
            }
            sleep(Duration::from_millis(25));
        }
    }

    /// Whether every descendant of the stub is gone, within the grace.
    fn desc_gone(&self, index: usize) -> bool {
        let begun = Instant::now();
        while self.started[index].desc.iter().any(Held::alive) {
            if begun.elapsed() > GRACE {
                return false;
            }
            sleep(Duration::from_millis(25));
        }
        true
    }

    /// The place in `desc` of the process of this label, which the stub wrote its own process ID
    /// for. Waits for the mark, which a process writes as it starts.
    fn mark(&self, index: usize, label: &str) -> usize {
        let path = self.started[index].marks.join(format!("{label}.pid"));
        let begun = Instant::now();
        loop {
            if let Ok(text) = fs::read_to_string(&path)
                && let Ok(pid) = text.trim().parse::<u32>()
                && let Some(at) = self.started[index].desc.iter().position(|h| h.pid == pid)
            {
                return at;
            }
            assert!(
                begun.elapsed() < APPEAR,
                "The mark {label} of {} did not appear",
                self.started[index].label
            );
            sleep(Duration::from_millis(25));
        }
    }

    /// Waits until the descendant at `at` has exited by itself.
    fn wait_exited(&self, index: usize, at: usize) {
        let begun = Instant::now();
        while self.started[index].desc[at].alive() {
            assert!(
                begun.elapsed() < APPEAR,
                "A descendant of {} did not exit",
                self.started[index].label
            );
            sleep(Duration::from_millis(25));
        }
    }

    /// Each descendant at these places is gone within the grace, and was given exit code 1.
    fn assert_held_ended(&self, index: usize, places: &[usize], what: &str) {
        let begun = Instant::now();
        for &at in places {
            let held = &self.started[index].desc[at];
            while held.alive() && begun.elapsed() <= GRACE {
                sleep(Duration::from_millis(25));
            }
            assert!(!held.alive(), "{what} is still running");
            assert_eq!(held.exit_code(), Some(1), "The exit code of {what}");
        }
    }

    /// Each descendant at these places still runs.
    fn assert_held_alive(&self, index: usize, places: &[usize], what: &str) {
        for &at in places {
            assert!(self.started[index].desc[at].alive(), "{what} was ended");
        }
    }

    fn assert_ended(&mut self, indexes: &[usize]) {
        for &i in indexes {
            assert!(
                self.tree_gone(i),
                "{} or a descendant is still running",
                self.started[i].label
            );
        }
    }

    /// The stub and every descendant run: the walk touched nothing of them.
    fn assert_alive(&mut self, indexes: &[usize]) {
        for &i in indexes {
            assert!(self.root_alive(i), "{} was ended", self.started[i].label);
            let label = self.started[i].label.clone();
            for held in &self.started[i].desc {
                assert!(held.alive(), "A descendant of {label} was ended");
            }
        }
    }

    /// A process that is ended is given exit code 1 (keeper.md, Stopping a holder).
    fn assert_exit_code_one(&mut self, indexes: &[usize]) {
        for &i in indexes {
            let status = self.started[i].root.try_wait().expect("The stub's status");
            assert_eq!(
                status.and_then(|s| s.code()),
                Some(1),
                "The exit code of {}",
                self.started[i].label
            );
            for held in &self.started[i].desc {
                assert_eq!(
                    held.exit_code(),
                    Some(1),
                    "The exit code of a descendant of {}",
                    self.started[i].label
                );
            }
        }
    }

    /// Ends every process the test started, by the handles it holds. Safe to call twice.
    fn end_all(&mut self) {
        for s in &mut self.started {
            for held in &s.desc {
                held.terminate();
            }
            let _ = s.root.kill();
            let _ = s.root.wait();
        }
    }

    /// The end of every test: ends what is left, and checks that each process is gone.
    fn finish(mut self) {
        self.end_all();
        let begun = Instant::now();
        for s in &self.started {
            while s.desc.iter().any(Held::alive) && begun.elapsed() < Duration::from_secs(10) {
                sleep(Duration::from_millis(25));
            }
            assert!(
                s.desc.iter().all(|h| !h.alive()),
                "A descendant of {} is still running",
                s.label
            );
        }
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        self.end_all();
        // The copies of the stub may be released a moment after their processes end.
        for _ in 0..20 {
            if fs::remove_dir_all(&self.dir).is_ok() {
                return;
            }
            sleep(Duration::from_millis(100));
        }
    }
}

type Row<'a> = (&'a str, &'a [&'a str]);

/// The common shape of a case: every `yes` row is a client and every `no` row is not. The walk
/// returns the count of the `yes` rows, they and their descendants are gone with exit code 1, and
/// every `no` row runs with its descendants.
fn case(yes: &[Row<'_>], no: &[Row<'_>]) {
    let mut lab = Lab::new();
    let yes: Vec<usize> = yes.iter().map(|(i, a)| lab.spawn(i, a)).collect();
    let no: Vec<usize> = no.iter().map(|(i, a)| lab.spawn(i, a)).collect();
    assert_eq!(lab.run(), Ok(yes.len()));
    lab.assert_ended(&yes);
    lab.assert_exit_code_one(&yes);
    lab.assert_alive(&no);
    lab.finish();
}

// ---- The Yes rows of driver.md's examples table ------------------------------------------------

#[test]
fn ssh_clients_naming_the_alias_are_ended() {
    case(
        &[
            ("ssh.exe", &["<a>"]),
            (
                "ssh.exe",
                &[
                    "-T",
                    "-o",
                    "ProxyCommand=x y",
                    "-o",
                    "ClearAllForwardings=yes",
                    "<a>",
                ],
            ),
            ("ssh.exe", &["-N", "-o", "ExitOnForwardFailure=yes", "<a>"]),
            ("ssh.exe", &["ubuntu@<a>"]),
        ],
        &[("ssh.exe", &["<b>"])],
    );
}

#[test]
fn ssh_clients_by_url_and_by_options_with_values_are_ended() {
    case(
        &[
            ("ssh.exe", &["ssh://ubuntu@<a>:22"]),
            (
                "ssh.exe",
                &["-l", "ubuntu", "-p", "22", "<a>", "uname", "-a"],
            ),
            ("ssh.exe", &["-s", "<a>", "sftp"]),
        ],
        &[("ssh.exe", &["ssh://ubuntu@<b>:22"])],
    );
}

#[test]
fn ssh_clients_in_capitals_after_double_dash_and_before_an_option_are_ended() {
    case(
        &[
            ("ssh.exe", &["-vvv", "-oPort=22", "<A>"]),
            ("ssh.exe", &["--", "<a>"]),
            ("ssh.exe", &["<a>", "-v"]),
        ],
        &[("ssh.exe", &["--", "<b>"])],
    );
}

#[test]
fn a_proxy_with_exactly_the_alias_is_ended() {
    case(
        &[("agentspace-ssh-proxy.exe", &["<a>"])],
        &[("agentspace-ssh-proxy.exe", &["<b>"])],
    );
}

#[test]
fn the_image_name_is_compared_without_ascii_case() {
    case(
        &[
            ("SSH.EXE", &["<a>"]),
            ("Agentspace-SSH-Proxy.EXE", &["<a>"]),
        ],
        &[("SSH.EXE", &["<b>"])],
    );
}

// ---- The No rows --------------------------------------------------------------------------------

#[test]
fn the_alias_as_an_options_value_is_not_a_client() {
    case(
        &[("ssh.exe", &["<a>"])],
        &[
            ("ssh.exe", &["-J", "<a>", "other"]),
            ("ssh.exe", &["-F", "<a>", "other"]),
            ("ssh.exe", &["-i<a>", "other"]),
        ],
    );
}

#[test]
fn the_alias_in_the_remote_command_is_not_a_client() {
    case(
        &[("ssh.exe", &["<a>"])],
        &[
            ("ssh.exe", &["other", "<a>"]),
            ("ssh.exe", &["other", "--", "<a>"]),
        ],
    );
}

#[test]
fn a_longer_a_shorter_and_another_alias_are_not_clients() {
    case(
        &[("ssh.exe", &["<a>"])],
        &[
            ("ssh.exe", &["<a>0"]),
            ("ssh.exe", &["<t>"]),
            ("ssh.exe", &["<b>"]),
        ],
    );
}

#[test]
fn an_ssh_url_with_a_path_and_a_line_with_no_destination_are_not_clients() {
    case(
        &[("ssh.exe", &["<a>"])],
        &[
            ("ssh.exe", &["ssh://<a>/path"]),
            ("ssh.exe", &["-p"]),
            ("ssh.exe", &[]),
        ],
    );
}

#[test]
fn a_proxy_with_extra_tokens_or_none_and_other_programs_are_not_clients() {
    case(
        &[("agentspace-ssh-proxy.exe", &["<a>"])],
        &[
            ("agentspace-ssh-proxy.exe", &["<a>", "extra"]),
            ("agentspace-ssh-proxy.exe", &[]),
            ("scp.exe", &["<a>:file", "."]),
            ("claude-ssh-proxy.exe", &["<a>"]),
            // Another image name whose only argument is the alias.
            ("other-tool.exe", &["<a>"]),
        ],
    );
}

#[test]
fn a_client_outside_the_folder_is_not_touched() {
    let mut lab = Lab::new();
    let client = lab.spawn("ssh.exe", &["<a>"]);
    let elsewhere = lab.dir.join("elsewhere");
    fs::create_dir_all(&elsewhere).expect("A folder beside the stub");
    let outside = end_alias_clients_under(&lab.alias, &elsewhere);
    assert_eq!(outside, Ok(0));
    lab.assert_alive(&[client]);
    assert_eq!(lab.run(), Ok(1));
    lab.assert_ended(&[client]);
    lab.finish();
}

#[test]
fn a_matching_client_in_a_second_folder_is_left_alone() {
    // The seam only narrows the match (the architect, 2026-11-22): clients of the same alias in a
    // second throwaway folder still run after the call that ends the first folder's.
    let mut lab = Lab::new();
    let mut beside = lab.beside();
    let inside = [
        lab.spawn("ssh.exe", &["<a>"]),
        lab.spawn("agentspace-ssh-proxy.exe", &["<a>"]),
    ];
    let outside = [
        beside.spawn("ssh.exe", &["<a>"]),
        beside.spawn("agentspace-ssh-proxy.exe", &["<a>"]),
    ];
    assert_eq!(lab.run(), Ok(2));
    lab.assert_ended(&inside);
    beside.assert_alive(&outside);
    lab.finish();
    beside.finish();
}

// ---- Trees, the count, and a second call -------------------------------------------------------

#[test]
fn a_clients_tree_is_ended_and_a_siblings_tree_is_not() {
    let mut lab = Lab::new();
    let client = lab.start(Spec {
        depth: 3,
        ..spec("ssh.exe", &["<a>"])
    });
    let other = lab.start(Spec {
        depth: 2,
        ..spec("ssh.exe", &["<b>"])
    });
    assert_eq!(lab.run(), Ok(1));
    lab.assert_ended(&[client]);
    lab.assert_exit_code_one(&[client]);
    lab.assert_alive(&[other]);
    lab.finish();
}

#[test]
fn the_count_is_the_clients_never_their_descendants() {
    let mut lab = Lab::new();
    let clients = [
        lab.start(Spec {
            depth: 3,
            ..spec("ssh.exe", &["<a>"])
        }),
        lab.start(Spec {
            depth: 3,
            ..spec("agentspace-ssh-proxy.exe", &["<a>"])
        }),
    ];
    assert_eq!(lab.run(), Ok(2));
    lab.assert_ended(&clients);
    lab.finish();
}

#[test]
fn a_proxy_started_by_a_matching_ssh_is_ended_once_and_not_counted_on_its_own() {
    let mut lab = Lab::new();
    // The ssh client's own ProxyCommand, which is the alias's line: a client inside a client.
    let nested = lab.start(Spec {
        depth: 2,
        child: Some(("agentspace-ssh-proxy.exe", &["<a>"])),
        ..spec("ssh.exe", &["<a>"])
    });
    let single = lab.spawn("ssh.exe", &["<a>"]);
    assert_eq!(lab.run(), Ok(2));
    lab.assert_ended(&[nested, single]);
    lab.assert_exit_code_one(&[nested, single]);
    lab.finish();
}

#[test]
fn a_matching_proxy_beside_a_non_matching_ssh_parent_is_counted() {
    let mut lab = Lab::new();
    // The proxy's parent is not a client, so the proxy is one on its own, and its parent is not
    // ended.
    let parent = lab.start(Spec {
        depth: 1,
        child: Some(("agentspace-ssh-proxy.exe", &["<a>"])),
        ..spec("ssh.exe", &["<b>"])
    });
    assert_eq!(lab.run(), Ok(1));
    assert!(
        lab.desc_gone(parent),
        "The matching descendant was not ended"
    );
    assert!(lab.root_alive(parent), "The non-matching parent was ended");
    lab.finish();
}

#[test]
fn a_second_call_right_after_returns_zero() {
    let mut lab = Lab::new();
    let yes = [
        lab.spawn("ssh.exe", &["<a>"]),
        lab.spawn("agentspace-ssh-proxy.exe", &["<a>"]),
    ];
    let no = lab.spawn("ssh.exe", &["<b>"]);
    assert_eq!(lab.run(), Ok(2));
    assert_eq!(lab.run(), Ok(0));
    lab.assert_ended(&yes);
    lab.assert_alive(&[no]);
    lab.finish();
}

#[test]
fn the_alias_given_to_the_call_is_compared_without_ascii_case() {
    let mut lab = Lab::new();
    let yes = [
        lab.spawn("ssh.exe", &["<a>"]),
        lab.spawn("ssh.exe", &["<A>"]),
        lab.spawn("agentspace-ssh-proxy.exe", &["<a>"]),
    ];
    let no = lab.spawn("ssh.exe", &["<b>"]);
    let upper = lab.alias.to_ascii_uppercase();
    assert_eq!(lab.run_for(&upper), Ok(3));
    lab.assert_ended(&yes);
    lab.assert_alive(&[no]);
    lab.finish();
}

#[test]
fn no_client_gives_zero() {
    let lab = Lab::new();
    assert_eq!(lab.run(), Ok(0));
    lab.finish();

    let mut lab = Lab::new();
    let no = [
        lab.spawn("ssh.exe", &["<b>"]),
        lab.spawn("agentspace-ssh-proxy.exe", &["<b>"]),
    ];
    assert_eq!(lab.run(), Ok(0));
    lab.assert_alive(&no);
    lab.finish();
}

// ---- One walk ends a chain of exited programs (driver.md, "Each with its tree"; D99; #0228) -----

/// A chain of clients and exited programs, as driver.md's "Each with its tree" describes it: a top
/// client, and below it, one below the other, an exited program and a client, as many times as the
/// case asks. Each exited program is still held by the process above it, and has started the
/// process below it before it exited, so that process has a parent that has gone while the process
/// above still holds it.
struct Chain {
    /// The started stub: the top client.
    top: usize,
    /// The places in the top's descendants of the clients below it, from the highest down.
    clients: Vec<usize>,
    /// The places of the exited programs, from the highest down.
    exited: Vec<usize>,
    /// The places of every other descendant: the plain levels below the lower clients.
    below: Vec<usize>,
}

/// Starts a chain. The top client's command line is `ssh.exe aliases[0]`, and each later entry is
/// the argument of one more `ssh.exe`, below an exited program. `below[k]` is how many plain levels
/// of descendants the client at `aliases[k + 1]` starts below itself. Waits until every program
/// of the chain runs or has exited, as it should.
fn start_chain(lab: &mut Lab, aliases: &[&str], below: &[u32]) -> Chain {
    let lower = aliases.len() - 1;
    assert_eq!(
        below.len(),
        lower,
        "One entry of `below` for each lower client"
    );
    let x_labels: Vec<String> = (1..=lower).map(|k| format!("x{k}")).collect();
    let c_labels: Vec<String> = (1..=lower).map(|k| format!("c{k}")).collect();
    let c_args: Vec<[&str; 1]> = aliases[1..].iter().map(|a| [*a]).collect();
    let mut nodes = Vec::new();
    for k in 0..lower {
        nodes.push(Node {
            label: &x_labels[k],
            exits: true,
            depth: 0,
            image: "claude-ssh-proxy.exe",
            args: &[],
        });
        nodes.push(Node {
            label: &c_labels[k],
            exits: false,
            depth: below[k],
            image: "ssh.exe",
            args: &c_args[k],
        });
    }
    let top = lab.start(Spec {
        depth: 0,
        chain: nodes,
        ..spec("ssh.exe", &[aliases[0]])
    });
    let clients: Vec<usize> = c_labels.iter().map(|l| lab.mark(top, l)).collect();
    let exited: Vec<usize> = x_labels.iter().map(|l| lab.mark(top, l)).collect();
    for &at in &exited {
        lab.wait_exited(top, at);
    }
    let total = lab.started[top].desc.len();
    let below: Vec<usize> = (0..total)
        .filter(|at| !clients.contains(at) && !exited.contains(at))
        .collect();
    assert!(lab.root_alive(top), "The top client runs before the walk");
    lab.assert_held_alive(top, &clients, "A lower client before the walk");
    lab.assert_held_alive(
        top,
        &below,
        "A process below a lower client before the walk",
    );
    Chain {
        top,
        clients,
        exited,
        below,
    }
}

/// Every client of the chain, with what runs below it, is gone and was given exit code 1. The
/// exited programs ended by themselves, with no code to check.
fn assert_chain_ended(lab: &mut Lab, chain: &Chain) {
    for (k, &at) in chain.clients.iter().enumerate() {
        lab.assert_held_ended(
            chain.top,
            &[at],
            &format!("The client {} below the top", k + 1),
        );
    }
    lab.assert_held_ended(chain.top, &chain.below, "A process below a lower client");
    assert_top_ended(lab, chain);
}

fn assert_top_ended(lab: &mut Lab, chain: &Chain) {
    let begun = Instant::now();
    while lab.root_alive(chain.top) && begun.elapsed() <= GRACE {
        sleep(Duration::from_millis(25));
    }
    let status = lab.started[chain.top]
        .root
        .try_wait()
        .expect("The top client's status");
    assert_eq!(
        status.and_then(|s| s.code()),
        Some(1),
        "The exit code of the top client"
    );
}

#[test]
fn the_exited_programs_of_a_chain_have_exited_and_the_clients_run_before_the_walk() {
    // The set-up the tests below rest on: with no walk, every client runs and each exited program
    // has exited, so a client below it has a parent that has gone.
    let mut lab = Lab::new();
    let chain = start_chain(&mut lab, &["<a>", "<a>", "<a>"], &[0, 0]);
    assert_eq!(chain.clients.len(), 2);
    assert_eq!(chain.exited.len(), 2);
    for &at in &chain.exited {
        assert!(!lab.started[chain.top].desc[at].alive());
    }
    lab.finish();
}

#[test]
fn one_walk_ends_every_client_of_a_chain_three_deep() {
    // "With a client, an exited program, a client, an exited program and a client, each below the
    // one before, the call builds three trees and ends all three clients."
    let mut lab = Lab::new();
    let chain = start_chain(&mut lab, &["<a>", "<a>", "<a>"], &[0, 0]);
    let _ = lab.run();
    assert_chain_ended(&mut lab, &chain);
    lab.finish();
}

#[test]
fn the_one_walk_counts_each_top_of_a_chain_three_deep_once() {
    // "A chain three deep ended in one call counts 3."
    let mut lab = Lab::new();
    let _chain = start_chain(&mut lab, &["<a>", "<a>", "<a>"], &[0, 0]);
    assert_eq!(lab.run(), Ok(3));
    lab.finish();
}

#[test]
fn the_release_line_of_a_chain_three_deep_ended_by_one_walk_reads_ended_3() {
    // D99's change for v0.12-T01: the release's line reads "Ended 3 of the lease's connections."
    // for the chain. The release's two walks are the two calls here: the first ends the chain,
    // the second ends none.
    let mut lab = Lab::new();
    let _chain = start_chain(&mut lab, &["<a>", "<a>", "<a>"], &[0, 0]);
    let first = lab.run();
    let second = lab.run();
    assert_eq!(
        release_text(&first, &second),
        "Ended 3 of the lease's connections."
    );
    lab.finish();
}

#[test]
fn a_second_walk_after_the_one_that_ended_a_chain_ends_none() {
    // "A release whose clients are all running when its first ending reads the process list ends
    // them all in that ending": nothing is left for the second.
    let mut lab = Lab::new();
    let _chain = start_chain(&mut lab, &["<a>", "<a>", "<a>"], &[0, 0]);
    let _ = lab.run();
    assert_eq!(lab.run(), Ok(0));
    lab.finish();
}

#[test]
fn one_walk_ends_every_client_of_a_chain_two_deep() {
    let mut lab = Lab::new();
    let chain = start_chain(&mut lab, &["<a>", "<a>"], &[0]);
    assert_eq!(lab.run(), Ok(2));
    assert_chain_ended(&mut lab, &chain);
    lab.finish();
}

#[test]
fn one_walk_ends_every_client_of_a_chain_five_deep() {
    // "Whatever its depth."
    let mut lab = Lab::new();
    let chain = start_chain(
        &mut lab,
        &["<a>", "<a>", "<a>", "<a>", "<a>"],
        &[0, 0, 0, 0],
    );
    assert_eq!(lab.run(), Ok(5));
    assert_chain_ended(&mut lab, &chain);
    lab.finish();
}

#[test]
fn a_lower_clients_own_tree_is_ended_with_it_and_not_counted() {
    // "Each top's tree is built by the same walk and checks": what runs below a client that is a
    // top in the same call is ended with it, and the count is the tops.
    let mut lab = Lab::new();
    let chain = start_chain(&mut lab, &["<a>", "<a>", "<a>"], &[1, 2]);
    assert_eq!(chain.below.len(), 3);
    assert_eq!(lab.run(), Ok(3));
    assert_chain_ended(&mut lab, &chain);
    lab.finish();
}

#[test]
fn a_client_of_another_alias_in_a_chain_is_left_running_while_the_clients_around_it_are_ended() {
    // The top and the lowest client of the alias are ended, and the one between them, of another
    // alias, is not (driver.md, "Each with its tree": "the call MUST NOT end any other process"
    // and a found client that no built tree holds is a top, so the count is 2).
    let mut lab = Lab::new();
    let chain = start_chain(&mut lab, &["<a>", "<b>", "<a>"], &[0, 0]);
    assert_eq!(lab.run(), Ok(2));
    lab.assert_held_ended(chain.top, &[chain.clients[1]], "The lowest client");
    assert_top_ended(&mut lab, &chain);
    lab.assert_held_alive(
        chain.top,
        &[chain.clients[0]],
        "The client of another alias",
    );
    lab.finish();
}

// ---- A spared ancestor's tree, through an exited program (driver.md, "Each with its tree") ------

const REL_ALIAS: &str = "ASCONN_REL_ALIAS";
const REL_DIR: &str = "ASCONN_REL_DIR";
const REL_OUT: &str = "ASCONN_REL_OUT";
const REL_GO: &str = "ASCONN_REL_GO";
const RELEASER: &str = "releaser_is_not_a_test";

/// Not a test of its own: the release's own process for the case below. A copy of this test
/// program, started below a client, waits for the file the case writes, makes the one call for
/// the alias and the folder it is given, and writes what the call returned. It passes at once when
/// it is not given them, as in every ordinary run.
#[test]
fn releaser_is_not_a_test() {
    let var = |name: &str| std::env::var(name).ok();
    let (Some(alias), Some(dir), Some(out), Some(go)) =
        (var(REL_ALIAS), var(REL_DIR), var(REL_OUT), var(REL_GO))
    else {
        return;
    };
    let go = PathBuf::from(go);
    let begun = Instant::now();
    while !go.exists() && begun.elapsed() < APPEAR {
        sleep(Duration::from_millis(10));
    }
    let text = match end_alias_clients_under(&alias, Path::new(&dir)) {
        Ok(n) => format!("ended {n} not_ended 0"),
        Err(EndError::NotAllEnded { ended, not_ended }) => {
            format!("ended {ended} not_ended {}", not_ended.len())
        }
        Err(_) => "unreadable".to_string(),
    };
    let part = PathBuf::from(format!("{out}.part"));
    fs::write(&part, text).expect("The result is written");
    fs::rename(&part, &out).expect("The result is named");
}

#[test]
fn a_client_below_an_exited_program_that_a_spared_ancestor_holds_is_not_ended() {
    // Guard, not a change: "A client whose ancestors, through an exited program or not, hold a
    // spared ancestor is never a top, and is not ended." The top client starts the release's own
    // process, so it is a spared ancestor, and holds an exited program with a client below it.
    let mut lab = Lab::new();
    let releaser = lab.dir.join("releaser.exe");
    fs::copy(
        std::env::current_exe().expect("This test program's path"),
        &releaser,
    )
    .expect("The releaser's copy");
    let out = lab.dir.join("releaser-out.txt");
    let go = lab.dir.join("releaser-go.txt");
    let home = lab.dir.join("home");
    fs::create_dir_all(&home).expect("The throwaway home");
    let text = |p: &Path| p.to_string_lossy().into_owned();
    let mut env = vec![
        (REL_ALIAS.to_string(), lab.alias.clone()),
        (REL_DIR.to_string(), text(&lab.dir)),
        (REL_OUT.to_string(), text(&out)),
        (REL_GO.to_string(), text(&go)),
    ];
    for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
        env.push((name.to_string(), text(&home)));
    }
    let nodes = vec![
        Node {
            label: "x1",
            exits: true,
            depth: 0,
            image: "claude-ssh-proxy.exe",
            args: &[],
        },
        Node {
            label: "c2",
            exits: false,
            depth: 0,
            image: "ssh.exe",
            args: &["<a>"],
        },
    ];
    let top = lab.start(Spec {
        depth: 1,
        child: Some(("releaser.exe", &["--exact", RELEASER, "--test-threads=1"])),
        chain: nodes,
        env,
        ..spec("ssh.exe", &["<a>"])
    });
    let lower = lab.mark(top, "c2");
    let proxy = lab.mark(top, "x1");
    lab.wait_exited(top, proxy);
    lab.assert_held_alive(top, &[lower], "The lower client before the walk");

    fs::write(&go, b"").expect("The go file");
    let begun = Instant::now();
    let result = loop {
        if let Ok(text) = fs::read_to_string(&out) {
            break text;
        }
        assert!(
            begun.elapsed() < APPEAR,
            "The release's own process did not finish its call"
        );
        sleep(Duration::from_millis(25));
    };
    // The spared ancestor is counted once as a client that could not be ended, and the client
    // below the exited program is neither ended nor counted.
    assert_eq!(
        result, "ended 0 not_ended 1",
        "The call's result for a spared ancestor's tree"
    );
    assert!(lab.root_alive(top), "The spared ancestor was ended");
    lab.assert_held_alive(top, &[lower], "The client below the exited program");
    lab.finish();
}
