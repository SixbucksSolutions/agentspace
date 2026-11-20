//! The keeper's two locks (keeper.md, Stopping a holder; state.md, The keeper's lock files).
//!
//! Each lock is a file in the state folder, held open with a share mode that refuses a second
//! taker. The open handle is the lock, never the file's existence or content, and the file is
//! never removed. The handles are opened through the standard library, whose file handles are
//! never inheritable, so no child process holds a lock.

use agentspace_keeper_api::{Locks, Taken};
use agentspace_keeper_files::{
    KEEPER_LOCK_FILE, KEEPER_PID_READ_BYTES, PASS_LOCK_FILE, parse_keeper_pid, serialize_keeper_pid,
};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// `FILE_SHARE_READ`.
const SHARE_READ: u32 = 0x1;
/// `FILE_SHARE_WRITE`.
const SHARE_WRITE: u32 = 0x2;

/// `ERROR_SHARING_VIOLATION` and `ERROR_LOCK_VIOLATION`: the file is in use.
const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;

/// The pinned `Locks` on Windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct WinLocks;

impl WinLocks {
    pub fn new() -> WinLocks {
        WinLocks
    }
}

/// A lock held: its open handle. Dropping it closes the handle and releases the lock.
#[derive(Debug)]
pub(crate) struct Held {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "The open handle is the lock: held, never read")
    )]
    pub(crate) file: File,
}

impl Locks for WinLocks {
    /// `board-holder.lock`, opened for reading and writing, created if missing, sharing nothing.
    /// While another holds it, it is tried again every 100 ms for up to 10 seconds, and `Skipped`
    /// when still held. Any other error is `Failed`. It is never `Busy`.
    // A lock does not create a missing state folder; the keeper creates it before its first pass
    // (keeper.md, Exit codes), and a lock in a missing one is `Failed` (keeper.md, Stopping a holder;
    // architect, 2026-11-19, for E0.06-T02-S03).
    fn pass(&self, state_dir: &Path) -> Taken {
        let path = state_dir.join(PASS_LOCK_FILE);
        match open_waiting(
            || open_lock(&path, 0),
            crate::PASS_LOCK_WAIT,
            crate::PASS_LOCK_RETRY,
        ) {
            Waited::Opened(file) => Taken::Held(Box::new(Held { file })),
            Waited::InUse => Taken::Skipped,
            Waited::Failed(e) => Taken::Failed(open_error(PASS_LOCK_FILE, &e)),
        }
    }

    /// `board-holder-keeper.lock`, opened for reading and writing, created if missing, sharing
    /// read only, and its content replaced with this process's PID. It is tried once and never
    /// waited for. While another holds it, it is `Busy` with the PID read from its first 32 bytes,
    /// if they give one. Any other error is `Failed`.
    fn keeper(&self, state_dir: &Path) -> Taken {
        let path = state_dir.join(KEEPER_LOCK_FILE);
        match open_lock(&path, SHARE_READ) {
            // A keeper lock whose PID cannot be written is not taken: it is `Failed`, since taking
            // it includes replacing its content (keeper.md, Stopping a holder; architect, 2026-11-19).
            Ok(mut file) => match write_pid(&mut file, std::process::id()) {
                Ok(()) => Taken::Held(Box::new(Held { file })),
                Err(e) => Taken::Failed(format!(
                    "{KEEPER_LOCK_FILE} could not be written: {}",
                    describe(&e)
                )),
            },
            Err(e) if in_use(&e) => Taken::Busy(read_pid(&path)),
            Err(e) => Taken::Failed(open_error(KEEPER_LOCK_FILE, &e)),
        }
    }
}

/// Opens a lock file for reading and writing, created if missing, with the share mode given.
fn open_lock(path: &Path, share: u32) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(share)
        .open(path)
}

/// Replaces the keeper lock's content with the PID, as decimal digits with nothing else.
fn write_pid(file: &mut File, pid: u32) -> io::Result<()> {
    file.set_len(0)?;
    file.write_all(&serialize_keeper_pid(pid))
}

/// A second keeper's reading of the PID (state.md: opened for reading, sharing read and write):
/// the first 32 bytes it can read. Anything that cannot be read gives no PID.
fn read_pid(path: &Path) -> Option<u32> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(SHARE_READ | SHARE_WRITE)
        .open(path)
        .ok()?;
    let mut bytes = Vec::with_capacity(KEEPER_PID_READ_BYTES);
    // `take` stops at 32 bytes; an error part way keeps what was read before it.
    let _ = file
        .take(KEEPER_PID_READ_BYTES as u64)
        .read_to_end(&mut bytes);
    parse_keeper_pid(&bytes)
}

/// Whether an error says the file is held by another (keeper.md: "the file being in use").
fn in_use(e: &io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
    )
}

// A lock's `<error>` is the Windows error number, as keeper.md writes the stop's, never the
// system's localized message text (keeper.md, Stopping a holder; architect, 2026-11-19).
fn describe(e: &io::Error) -> String {
    match e.raw_os_error() {
        Some(n) => format!("Windows error {n}."),
        None => format!("{e}."),
    }
}

fn open_error(file: &str, e: &io::Error) -> String {
    format!("{file} could not be opened: {}", describe(e))
}

pub(crate) enum Waited<T> {
    Opened(T),
    /// Still in use when the wait ran out.
    InUse,
    Failed(io::Error),
}

/// Tries `open` until it gives anything but "in use", every `retry`, for up to `wait` on a
/// monotonic clock. The last try is made when the wait runs out.
pub(crate) fn open_waiting<T>(
    mut open: impl FnMut() -> io::Result<T>,
    wait: Duration,
    retry: Duration,
) -> Waited<T> {
    let start = Instant::now();
    loop {
        match open() {
            Ok(t) => return Waited::Opened(t),
            Err(e) if in_use(&e) => {
                let elapsed = start.elapsed();
                if elapsed >= wait {
                    return Waited::InUse;
                }
                std::thread::sleep(retry.min(wait - elapsed));
            }
            Err(e) => return Waited::Failed(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_use_error() -> io::Error {
        io::Error::from_raw_os_error(ERROR_SHARING_VIOLATION)
    }

    #[test]
    fn the_wait_retries_while_in_use_and_takes_it_when_freed() {
        let mut tries = 0;
        let got = open_waiting(
            || {
                tries += 1;
                if tries < 4 {
                    Err(in_use_error())
                } else {
                    Ok(tries)
                }
            },
            Duration::from_secs(10),
            Duration::from_millis(1),
        );
        assert!(matches!(got, Waited::Opened(4)));
    }

    #[test]
    fn the_wait_gives_up_in_use_after_its_time() {
        let start = Instant::now();
        let got = open_waiting(
            || Err::<(), _>(in_use_error()),
            Duration::from_millis(300),
            Duration::from_millis(100),
        );
        let took = start.elapsed();
        assert!(matches!(got, Waited::InUse));
        assert!(took >= Duration::from_millis(300), "{took:?}");
        assert!(took < Duration::from_secs(2), "{took:?}");
    }

    #[test]
    fn the_wait_never_retries_another_error() {
        let mut tries = 0;
        let got = open_waiting(
            || {
                tries += 1;
                Err::<(), _>(io::Error::from_raw_os_error(5))
            },
            Duration::from_secs(10),
            Duration::from_millis(1),
        );
        assert!(matches!(got, Waited::Failed(_)));
        assert_eq!(tries, 1);
    }

    #[test]
    fn a_lock_violation_is_in_use_too() {
        assert!(in_use(&io::Error::from_raw_os_error(ERROR_LOCK_VIOLATION)));
        assert!(!in_use(&io::Error::from_raw_os_error(5)));
    }

    #[test]
    fn the_pass_wait_is_10_seconds_tried_every_100_ms() {
        assert_eq!(crate::PASS_LOCK_WAIT, Duration::from_secs(10));
        assert_eq!(crate::PASS_LOCK_RETRY, Duration::from_millis(100));
    }
}
