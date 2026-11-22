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

use agentspace_connections::{EndError, end_alias_clients_under};
use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::PathBuf;
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
}

fn spec<'a>(image: &'a str, args: &'a [&'a str]) -> Spec<'a> {
    Spec {
        image,
        args,
        depth: 1,
        child: None,
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
        let args: Vec<String> = spec.args.iter().map(|a| self.fill(a)).collect();
        let mut command = Command::new(&exe);
        command
            .args(&args)
            .env_remove(ENV_CHILD)
            .env(ENV_DEPTH, spec.depth.to_string())
            .env(ENV_PIDS, &pidfile)
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
        let root = command.spawn().expect("The stub starts");
        self.started.push(Started {
            label: format!("#{index} {} with {} arguments", spec.image, args.len()),
            root,
            desc: Vec::new(),
            pidfile,
        });
        self.hold_descendants(index, spec.depth as usize);
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
