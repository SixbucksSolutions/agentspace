//! Stand-in clients for the release tests (docs/driver.md, "Ending a lease's connections").
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! **Process safety, and the rules every test that uses this keeps:**
//! - **A stand-in is a renamed copy of `tests/support/stub.rs`'s program**, in a folder of its own
//!   under the temp folder. A test MUST NOT copy, open or start the installed programs, nor any
//!   `ssh.exe` of Windows'.
//! - **Every alias a test uses is built from a UUIDv4 drawn at random for that test**, so that no
//!   process of Terry's can match it, and no other test's stand-in can.
//! - **Every process a test starts is ended by the `Lab`'s drop guard**, whatever the assertions
//!   do. A test never ends, opens or prints a process it did not start: a grandchild is opened by
//!   the process ID its parent stand-in wrote, at once, and held by the handle.
//! - **No test prints a command line or an environment value.**
#![cfg(windows)]
#![allow(dead_code, unsafe_code)]

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const STUB: &str = env!("CARGO_BIN_EXE_agentspace-driver-ec2-stub");

/// How long a test waits for something a stand-in or a child process has to do.
pub const WAIT: Duration = Duration::from_secs(30);

/// The few Windows calls the tests need, to hold a stand-in's children by handle.
mod win {
    use std::ffi::c_void;

    type Handle = *mut c_void;

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

        pub fn alive(&self) -> bool {
            // SAFETY: The handle is open and owned by `self`.
            unsafe { WaitForSingleObject(self.handle, 0) == WAIT_TIMEOUT }
        }

        pub fn exit_code(&self) -> Option<u32> {
            if self.alive() {
                return None;
            }
            let mut code = 0u32;
            // SAFETY: The handle is open, and `code` is a valid place for the result.
            let ok = unsafe { GetExitCodeProcess(self.handle, &mut code) };
            if ok == 0 { None } else { Some(code) }
        }

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

pub use win::Held;

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

/// A UUIDv4 drawn at random, in its normalized (lowercase) text form.
pub fn random_uuid_v4() -> String {
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

/// What a stand-in is told besides its command line.
#[derive(Default)]
pub struct Opts {
    /// Children to start: the program and its arguments.
    pub children: Vec<(PathBuf, Vec<String>)>,
    /// A path to poll, and the file the answers go to.
    pub watch: Option<(PathBuf, PathBuf)>,
    /// Environment for the process and its children, by name and value.
    pub env: Vec<(String, String)>,
    /// A throwaway home, set as the profile of the process and its children.
    pub home: Option<PathBuf>,
}

struct Started {
    child: Child,
    held: Vec<Held>,
    pids: PathBuf,
    expected: usize,
}

/// A test's folder of stand-ins and every process it started. Dropping it ends them all and
/// removes the folder.
pub struct Lab {
    pub dir: PathBuf,
    started: Vec<Started>,
}

impl Lab {
    pub fn new() -> Lab {
        let dir = std::env::temp_dir().join(format!("asdrv-stubs-{}", random_uuid_v4()));
        fs::create_dir_all(&dir).expect("The stand-ins' folder can be made");
        Lab {
            dir,
            started: Vec::new(),
        }
    }

    /// The copy of the stand-in under this image name, made the first time it is asked for.
    pub fn copy_of(&self, image: &str) -> PathBuf {
        let path = self.dir.join(image);
        if !path.exists() {
            fs::copy(STUB, &path).expect("The stand-in's copy can be made");
        }
        path
    }

    /// Starts the stand-in under `image` with the command-line arguments `args`.
    pub fn start(&mut self, image: &str, args: &[&str]) -> usize {
        self.start_with(image, args, Opts::default())
    }

    /// As `start`, with what `opts` says. Returns once every child the stand-in starts is held.
    pub fn start_with(&mut self, image: &str, args: &[&str], opts: Opts) -> usize {
        let exe = self.copy_of(image);
        let index = self.started.len();
        let pids = self.dir.join(format!("pids-{index}.txt"));
        let mut command = Command::new(&exe);
        command
            .args(args)
            .env("ASDRV_STUB_PIDS", &pids)
            .env("ASDRV_STUB_CHILDREN", opts.children.len().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(home) = &opts.home {
            for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
                command.env(name, home);
            }
        }
        for (i, (program, child_args)) in opts.children.iter().enumerate() {
            command
                .env(format!("ASDRV_STUB_CHILD{i}_EXE"), program)
                .env(
                    format!("ASDRV_STUB_CHILD{i}_ARGS"),
                    child_args.join("\u{1f}"),
                );
        }
        if let Some((path, log)) = &opts.watch {
            command
                .env("ASDRV_STUB_WATCH", path)
                .env("ASDRV_STUB_LOG", log);
        }
        for (name, value) in &opts.env {
            command.env(name, value);
        }
        let child = command.spawn().expect("The stand-in starts");
        self.started.push(Started {
            child,
            held: Vec::new(),
            pids,
            expected: opts.children.len(),
        });
        self.hold_children(index);
        if let Some((_, log)) = &opts.watch {
            wait_for(
                || fs::read_to_string(log).is_ok_and(|t| !t.is_empty()),
                "A watch's first line",
            );
        }
        index
    }

    fn hold_children(&mut self, index: usize) {
        let begun = Instant::now();
        loop {
            let s = &mut self.started[index];
            if let Ok(text) = fs::read_to_string(&s.pids) {
                for line in text.split_terminator('\n') {
                    let Ok(pid) = line.trim().parse::<u32>() else {
                        continue;
                    };
                    if s.held.iter().any(|h| h.pid == pid) {
                        continue;
                    }
                    s.held
                        .push(Held::open(pid).expect("A child of the stand-in is opened"));
                }
            }
            if s.held.len() >= s.expected {
                return;
            }
            if let Ok(Some(status)) = s.child.try_wait() {
                panic!("A stand-in exited early with {:?}", status.code());
            }
            assert!(
                begun.elapsed() < WAIT,
                "A stand-in's children did not appear"
            );
            sleep(Duration::from_millis(50));
        }
    }

    /// Whether the stand-in this test started is still running.
    pub fn alive(&mut self, index: usize) -> bool {
        matches!(self.started[index].child.try_wait(), Ok(None))
    }

    /// The exit code of an ended stand-in.
    pub fn exit_code(&mut self, index: usize) -> Option<i32> {
        self.started[index]
            .child
            .try_wait()
            .ok()
            .flatten()
            .and_then(|s| s.code())
    }

    /// The stand-in's children, in the order it started them.
    pub fn children(&self, index: usize) -> &[Held] {
        &self.started[index].held
    }

    /// Ends a stand-in this test started, and waits for it.
    pub fn end(&mut self, index: usize) {
        let s = &mut self.started[index];
        let _ = s.child.kill();
        let _ = s.child.wait();
        for h in &s.held {
            h.terminate();
        }
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        for s in &mut self.started {
            let _ = s.child.kill();
            let _ = s.child.wait();
            for h in &s.held {
                h.terminate();
            }
        }
        // A last wait for each child, so that its image is released before the folder goes.
        let begun = Instant::now();
        while begun.elapsed() < Duration::from_secs(5)
            && self.started.iter().any(|s| s.held.iter().any(Held::alive))
        {
            sleep(Duration::from_millis(50));
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Waits until `condition` holds, for at most `WAIT`, and fails naming `what` otherwise.
pub fn wait_for(mut condition: impl FnMut() -> bool, what: &str) {
    let begun = Instant::now();
    while !condition() {
        assert!(begun.elapsed() < WAIT, "{what} did not happen in time");
        sleep(Duration::from_millis(50));
    }
}

/// The lines of a file, or none when it cannot be read.
pub fn lines_of(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Seconds since the Unix epoch of a UTC `YYYY-MM-DDTHH:MM:SSZ`, or None when the text is not in
/// that exact form.
pub fn parse_utc(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() != 20 {
        return None;
    }
    for (i, c) in b.iter().enumerate() {
        let ok = match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        };
        if !ok {
            return None;
        }
    }
    let n = |r: std::ops::Range<usize>| text[r].parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss) = (n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// The system clock in seconds since the Unix epoch.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
