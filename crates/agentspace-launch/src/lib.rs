//! Starting a holder: the launch handshake, WMI and the launcher (docs/keeper.md, Starting a
//! holder; docs/state.md, A holder's launch files; D42).
//!
//! RFC 2119 keywords in this crate's comments, and the capitals are load-bearing.
//!
//! # The public API
//!
//! | Item | What it is |
//! |---|---|
//! | `WmiStarter` | The pinned `agentspace_keeper_api::Starter`: the pass's half of the handshake. It writes the request, has the launcher created through WMI's `Win32_Process.Create`, waits for the claim and the result within every bound of The handshake, withdraws an unanswered request with `abandoned`, checks the holder once, and removes the three files in the spec's order, keeping the claim of a request it cannot remove. |
//! | `run_launcher` | The launcher's half: it claims the request, starts the holder with `CreateProcessW`, and writes the result. It returns the launcher's exit code. |
//! | `LAUNCHER_ARGUMENT`, `launcher_request` | The private argument that makes the keeper's own program the launcher (D42), and its reader. |
//!
//! # The launcher's command line
//!
//! **The launcher is the keeper's own program (`StartRequest::own_exe`), run as
//! `"<own_exe>" -Launch "<request>"`**, where `<request>` is the full path of
//! `board-holder-launch.<hex>.json`. Neither path holds a quote: the pass refuses a start whose
//! state folder or program path does (keeper.md, Before the start).
//! - **The keeper's `main` MUST route it before its usage checks**: when `launcher_request` gives
//!   a path for the arguments after the program name, `main` MUST exit with
//!   `run_launcher(&path)`, and do nothing else. It is exactly two arguments, `-Launch` (compared
//!   exactly, case included) and the request's path; anything else is the keeper's own command
//!   line.
//! - **`-Launch` MUST NOT appear in a usage line or a message** (D42; keeper.md, Command line: it
//!   is not part of the interface, and a check never passes it).
//!
//! # The launcher's exit codes
//!
//! `run_launcher` returns 0 when it answered, whatever its answer said, and when it rightly started
//! nothing: it lost the claim, or found its request gone or unreadable after it claimed it. It
//! returns 1 when it could not write its result, so its pass sees no answer, and 2 when its
//! argument names no `board-holder-launch.<hex>.json`, in which case it wrote nothing. It writes
//! nothing to stdout or stderr, and logs nothing (keeper.md, The handshake, step 4).
//!
//! # Building on another system
//!
//! **Every call into Windows is in `src/win/`, behind `cfg(windows)`, and only those modules use
//! `unsafe`.** On any other system the crate builds and its portable parts (the handshake's steps
//! and bounds, the launcher's claim and answer, the time forms) run and are tested the same; there
//! the create call throws `Win32_Process.Create is only on Windows.`, so a start fails after its
//! three tries, and a launcher answers `Holders are started only on Windows.` (keeper.md, Where it
//! runs: the keeper is Windows-only in v0).
//!
//! # Tests
//!
//! The test-only binary `agentspace-launch-testhost` (src/bin/testhost.rs) is never installed. It
//! is the keeper's stand-in for the tests: with `-Launch <request>` it runs `run_launcher`, and
//! with `--sleep <secs>` it is a stub holder that sleeps.

#![deny(unsafe_code)]

mod files;
mod handshake;
mod launcher;
mod time;

#[cfg(windows)]
#[allow(unsafe_code)]
mod win;

#[cfg(test)]
mod testing;

use agentspace_keeper_api::{Clock, Matcher, Query, StartRequest, Started, Starter};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The private argument that runs the keeper's own program as the launcher (D42). It MUST NOT
/// appear in a usage line.
pub const LAUNCHER_ARGUMENT: &str = "-Launch";

/// The request's path when `args` (the arguments after the program's name) are exactly
/// `-Launch <request>`, and `None` for any other command line. The keeper's `main` routes a
/// `Some` to `run_launcher` before anything else.
pub fn launcher_request(args: &[OsString]) -> Option<PathBuf> {
    match args {
        [flag, request] if flag == LAUNCHER_ARGUMENT && !request.is_empty() => {
            Some(PathBuf::from(request))
        }
        _ => None,
    }
}

/// The launcher's command line: `"<own_exe>" -Launch "<request>"`.
pub(crate) fn launcher_command_line(own_exe: &Path, request: &Path) -> String {
    format!(
        "\"{}\" {LAUNCHER_ARGUMENT} \"{}\"",
        own_exe.display(),
        request.display()
    )
}

/// The launcher's half of the handshake (keeper.md, The handshake, steps 3 and 4), for the keeper
/// program run with `-Launch <request>`. It claims the request before anything else; starts the
/// holder with stdin from `NUL`, stdout and stderr one shared append handle to the request's
/// log, no other handle inherited, no window, a new process group, out of its job where Windows
/// allows, in the request's working folder; and writes the result read back from the process it
/// made. Its return value is the launcher's exit code (the crate's docs list them).
pub fn run_launcher(request: &Path) -> i32 {
    launcher::launch(request, &SystemSpawn)
}

/// The real system for the launcher.
struct SystemSpawn;

impl launcher::Spawn for SystemSpawn {
    #[cfg(windows)]
    fn spawn(
        &self,
        request: &agentspace_keeper_files::LaunchRequest,
    ) -> Result<launcher::Made, String> {
        win::spawn::spawn(request)
    }

    #[cfg(not(windows))]
    fn spawn(&self, _: &agentspace_keeper_files::LaunchRequest) -> Result<launcher::Made, String> {
        Err("Holders are started only on Windows.".to_string())
    }

    fn utc_now_ticks(&self) -> i64 {
        time::utc_now_ticks()
    }

    fn own_pid(&self) -> u32 {
        std::process::id()
    }
}

/// The pinned `Starter`: the launch handshake, with the launcher created through WMI's
/// `Win32_Process.Create` (keeper.md, Starting a holder).
#[derive(Debug, Default, Clone, Copy)]
pub struct WmiStarter {
    _private: (),
}

impl WmiStarter {
    pub fn new() -> WmiStarter {
        WmiStarter { _private: () }
    }
}

impl Starter for WmiStarter {
    /// The whole handshake: the request written, the launcher created by `Win32_Process.Create`
    /// as `"<own_exe>" -Launch "<request>"` with no window and `req.profile` as its working
    /// folder, the claim, the result and the deadline within every bound of The handshake, the
    /// holder checked once, and the three files removed: the request, the claim, the result. When
    /// the request cannot be removed its claim stays, and only the result is removed.
    /// `Started::Failed` is the whole `reason=` text, `The holder could not be started: `
    /// included.
    ///
    /// **The starter owns the request's form** (state.md, A holder's launch files): it writes
    /// every path with backslashes, turning each `/` of `req.ssh_exe` into `\`, and asks
    /// `matcher` for the holder's command line with that same path, so the request, the record
    /// and a later match agree. The checks of keeper.md, Before the start, are the pass's, and
    /// are not repeated here. The request's `application` is `req.ssh_exe`, or
    /// `<System32>\cmd.exe` for a `.cmd` or `.bat`, with `<System32>` as Windows reports it, so
    /// `matcher` MUST be the `HolderMatcher` of that same folder.
    fn start(&self, req: &StartRequest, matcher: &dyn Matcher) -> Started {
        let application = match system32() {
            Some(folder) => {
                agentspace_keeper_files::HolderMatcher::new(folder).program(req.ssh_exe)
            }
            None => req.ssh_exe.to_path_buf(),
        };
        handshake::start(&SystemPass::new(), req, matcher, &application)
    }
}

#[cfg(windows)]
fn system32() -> Option<PathBuf> {
    win::system32()
}

#[cfg(not(windows))]
fn system32() -> Option<PathBuf> {
    None
}

/// The real clocks: the UTC clock for the deadline, and a monotonic one for every wait.
struct SystemClock {
    origin: Instant,
}

impl Clock for SystemClock {
    fn utc_now_ms(&self) -> i64 {
        time::utc_now_ticks().div_euclid(10_000)
    }

    fn monotonic(&self) -> Duration {
        self.origin.elapsed()
    }

    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// The real system for the pass.
struct SystemPass {
    clock: SystemClock,
}

impl SystemPass {
    fn new() -> SystemPass {
        SystemPass {
            clock: SystemClock {
                origin: Instant::now(),
            },
        }
    }
}

impl handshake::Pass for SystemPass {
    fn clock(&self) -> &dyn Clock {
        &self.clock
    }

    fn hex(&self) -> String {
        format!("{:032x}", random_bits())
    }

    fn means(&self) -> &str {
        "Win32_Process.Create"
    }

    fn create(&self, command_line: &str, directory: &Path) -> handshake::Create {
        let command_line = command_line.to_string();
        let directory = directory.to_string_lossy().into_owned();
        handshake::bounded(
            move || create_launcher(&command_line, &directory),
            handshake::CREATE_BOUND,
        )
    }

    fn query(&self, pid: u32) -> Query {
        query_process(pid)
    }

    fn remove(&self, path: &Path) -> bool {
        remove_launch_file(path)
    }
}

/// Removes a launch file in one attempt: `true` when it is gone afterwards, whether removed now or
/// not there, and `false` when it is still there (keeper.md, The handshake, step 8).
fn remove_launch_file(path: &Path) -> bool {
    match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

#[cfg(windows)]
fn create_launcher(command_line: &str, directory: &str) -> handshake::Create {
    win::wmi::create(command_line, directory)
}

#[cfg(not(windows))]
fn create_launcher(_: &str, _: &str) -> handshake::Create {
    handshake::Create::Threw("Win32_Process.Create is only on Windows.".to_string())
}

#[cfg(windows)]
fn query_process(pid: u32) -> Query {
    win::process::query(pid)
}

#[cfg(not(windows))]
fn query_process(_: u32) -> Query {
    Query::Gone
}

/// 128 bits for a start's `<hex>`. Its job is to not repeat, not to be secret.
fn random_bits() -> u128 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_i64(time::utc_now_ticks());
    h.write_u32(std::process::id());
    h.write_u64(COUNT.fetch_add(1, Ordering::Relaxed));
    let high = h.finish();
    h.write_u8(1);
    let low = h.finish();
    (u128::from(high) << 64) | u128::from(low)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exactly_the_private_argument_and_a_path_make_a_launcher() {
        let args = |a: &[&str]| a.iter().map(OsString::from).collect::<Vec<_>>();
        assert_eq!(
            launcher_request(&args(&["-Launch", r"C:\s\board-holder-launch.ab.json"])),
            Some(PathBuf::from(r"C:\s\board-holder-launch.ab.json"))
        );
        for other in [
            &[][..],
            &["-Launch"][..],
            &["-Launch", ""][..],
            &["-launch", "x"][..],
            &["-Launch", "x", "-Watch"][..],
            &["-Watch", "-Launch"][..],
        ] {
            assert_eq!(launcher_request(&args(other)), None, "{other:?}");
        }
    }

    #[test]
    fn the_launcher_command_line_quotes_both_paths() {
        assert_eq!(
            launcher_command_line(
                Path::new(r"C:\Program Files\agentspace\agentspace-keeper.exe"),
                Path::new(r"C:\Users\someone\.agentspace\board-holder-launch.ab.json")
            ),
            r#""C:\Program Files\agentspace\agentspace-keeper.exe" -Launch "C:\Users\someone\.agentspace\board-holder-launch.ab.json""#
        );
    }

    /// Step 8's seam: a launch file another handle holds without sharing delete is not gone, so
    /// the pass keeps its claim; one removed now, or not there, is gone.
    #[cfg(windows)]
    #[test]
    fn a_launch_file_held_without_sharing_delete_is_not_removed() {
        use std::os::windows::fs::OpenOptionsExt;
        let scratch = testing::Scratch::new("removal");
        let path = scratch.path().join("board-holder-launch.ab.json");
        std::fs::write(&path, b"{}\n").unwrap();
        let hold = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2) // FILE_SHARE_READ | FILE_SHARE_WRITE, not FILE_SHARE_DELETE.
            .open(&path)
            .unwrap();
        assert!(!remove_launch_file(&path));
        assert!(path.exists());
        drop(hold);
        assert!(remove_launch_file(&path));
        assert!(!path.exists());
        assert!(remove_launch_file(&path));
    }

    #[test]
    fn a_hex_is_32_lowercase_hex_digits_and_new_each_time() {
        use handshake::Pass;
        let pass = SystemPass::new();
        let a = pass.hex();
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
        assert_ne!(a, pass.hex());
    }
}
