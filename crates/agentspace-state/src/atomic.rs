//! `write_atomically`: the keeper's and the operator's JSON files, replaced the way state.md,
//! Replacing a file, says, over the same temporary-file and replace layer as the lease state file.

use crate::sys::{self, ReplaceFailure};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a rename that finds the file held is tried again for the keeper's JSON files
/// (state.md, Replacing a file).
const REPLACE_WAIT: Duration = Duration::from_secs(5);
/// The gap between tries (state.md, The keeper's files: every 50 milliseconds).
const RETRY_EVERY: Duration = Duration::from_millis(50);

/// 128 random bits for a temporary file's name. Its job is to not repeat, not to be secret, and
/// `create_new` catches a repeat anyway.
fn random_bits() -> u128 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    h.write_u128(nanos);
    h.write_u32(std::process::id());
    h.write_u64(COUNT.fetch_add(1, Ordering::Relaxed));
    let high = h.finish();
    h.write_u8(1);
    let low = h.finish();
    (u128::from(high) << 64) | u128::from(low)
}

/// Makes `<file name>.<32 hex digits>.tmp` beside the file, a name a reader never takes for the
/// file, and returns it with the new file open.
fn create_temp(path: &Path, name: &std::ffi::OsStr) -> io::Result<(PathBuf, File)> {
    let mut last = io::Error::other("no temporary file name was free");
    for _ in 0..8 {
        let mut temp_name: OsString = name.to_os_string();
        temp_name.push(format!(".{:032x}.tmp", random_bits()));
        let temp = path.with_file_name(temp_name);
        match sys::create_temp_file(&temp) {
            Ok(f) => return Ok((temp, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => last = e,
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

/// Replaces the file at `path` with `bytes`, atomically (state.md, Replacing a file): the whole
/// new content goes into `<file name>.<32 hex digits>.tmp` in the same folder, is flushed to the
/// disk, and is moved over the file in one rename (on Windows a POSIX rename, with `MoveFileExW`
/// only where the file system refuses those semantics). A reader sees the old file or the new
/// one, never part of either.
///
/// A rename that fails because another process holds the file without sharing delete is tried
/// again every 50 milliseconds, and stops within 5 seconds, then fails with
/// `ErrorKind::TimedOut`. A replace never creates a folder (state.md, Replacing a file): a missing
/// folder is an error and leaves nothing behind, and the caller makes the folder first. The
/// temporary file is deleted if anything fails before the move, and a failed rename leaves none
/// either.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomically_with(path, bytes, REPLACE_WAIT, RETRY_EVERY, &mut sys::replace)
}

/// The move over the file: `sys::replace`, or a stand-in in a test of the retry.
type Replace<'a> = dyn FnMut(&File, &Path, &Path) -> Result<(), ReplaceFailure> + 'a;

fn write_atomically_with(
    path: &Path,
    bytes: &[u8],
    wait: Duration,
    every: Duration,
    replace: &mut Replace,
) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path names no file"))?;
    let (temp, mut handle) = create_temp(path, name)?;
    let remove_temp = || {
        let _ = fs::remove_file(&temp);
    };
    // Written and flushed to the disk first. The handle stays open for the rename, which Windows
    // does through it, and is closed before the temporary file is deleted.
    if let Err(e) = handle.write_all(bytes).and_then(|()| handle.sync_all()) {
        drop(handle);
        remove_temp();
        return Err(e);
    }
    let start = Instant::now();
    loop {
        let Err(failure) = replace(&handle, &temp, path) else {
            return Ok(());
        };
        let error = if path.is_dir() {
            // A folder at the path is not a held file, and waiting does not change it.
            Some(io::Error::other("a folder is where the file belongs"))
        } else if failure == ReplaceFailure::Other {
            Some(io::Error::other("the file could not be replaced"))
        } else if start.elapsed() >= wait {
            Some(io::Error::new(
                io::ErrorKind::TimedOut,
                "the file stayed in use",
            ))
        } else {
            None
        };
        if let Some(e) = error {
            drop(handle);
            remove_temp();
            return Err(e);
        }
        std::thread::sleep(every.min(wait.saturating_sub(start.elapsed())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentspace-atomic-{tag}-{:x}", random_bits()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn a_new_file_is_written_with_exactly_the_bytes_and_no_temporary_file_stays() {
        let dir = folder("new");
        let file = dir.join("board-holder.json");
        write_atomically(&file, b"{\"a\":1}\n").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"{\"a\":1}\n");
        assert_eq!(leftovers(&dir), ["board-holder.json"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_existing_file_is_replaced_whole() {
        let dir = folder("replace");
        let file = dir.join("x.json");
        fs::write(&file, b"a much longer old content\n").unwrap();
        write_atomically(&file, b"{}\n").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"{}\n");
        write_atomically(&file, b"").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"");
        assert_eq!(leftovers(&dir), ["x.json"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_is_an_error_and_is_not_made() {
        let dir = folder("mkdir");
        let missing = dir.join("a");
        assert!(write_atomically(&missing.join("b").join("x.json"), b"1\n").is_err());
        assert!(!missing.exists());
        assert!(leftovers(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_at_the_path_fails_at_once_and_leaves_no_temporary_file() {
        let dir = folder("isdir");
        let file = dir.join("x.json");
        fs::create_dir(&file).unwrap();
        let start = Instant::now();
        assert!(write_atomically(&file, b"1\n").is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(leftovers(&dir), ["x.json"]);
        assert!(file.is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_where_the_folder_belongs_fails() {
        let dir = folder("underfile");
        let blocker = dir.join("blocker");
        fs::write(&blocker, b"x").unwrap();
        assert!(write_atomically(&blocker.join("x.json"), b"1\n").is_err());
        assert_eq!(leftovers(&dir), ["blocker"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_path_naming_no_file_fails() {
        assert!(write_atomically(Path::new("/"), b"1\n").is_err());
    }

    #[test]
    fn the_contract_retries_every_50_milliseconds_for_5_seconds() {
        // state.md, Replacing a file and The keeper's files.
        assert_eq!(REPLACE_WAIT, Duration::from_secs(5));
        assert_eq!(RETRY_EVERY, Duration::from_millis(50));
    }

    #[test]
    fn the_temporary_file_is_beside_the_file_named_for_it_and_whole_before_the_move() {
        let dir = folder("beside");
        let file = dir.join("board-holder.json");
        let mut seen: Option<(PathBuf, Vec<u8>)> = None;
        let mut replace = |h: &File, from: &Path, to: &Path| {
            seen = Some((from.to_path_buf(), fs::read(from).unwrap_or_default()));
            sys::replace(h, from, to)
        };
        write_atomically_with(
            &file,
            b"{\"pid\":1}\n",
            REPLACE_WAIT,
            RETRY_EVERY,
            &mut replace,
        )
        .unwrap();
        let (temp, content) = seen.unwrap();
        assert_eq!(temp.parent(), Some(dir.as_path()));
        let name = temp.file_name().unwrap().to_string_lossy().into_owned();
        let hex = name
            .strip_prefix("board-holder.json.")
            .and_then(|n| n.strip_suffix(".tmp"))
            .unwrap();
        assert_eq!(hex.len(), 32);
        assert!(
            hex.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert_eq!(content, b"{\"pid\":1}\n");
        assert_eq!(leftovers(&dir), ["board-holder.json"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_held_file_is_tried_again_until_the_wait_ends_then_times_out_leaving_no_temporary_file() {
        let dir = folder("held");
        let file = dir.join("x.json");
        fs::write(&file, b"old").unwrap();
        let mut tries = 0u32;
        let mut held = |_: &File, _: &Path, _: &Path| {
            tries += 1;
            Err(ReplaceFailure::Held)
        };
        let wait = Duration::from_millis(300);
        let every = Duration::from_millis(50);
        let began = Instant::now();
        let e = write_atomically_with(&file, b"new", wait, every, &mut held).unwrap_err();
        let took = began.elapsed();
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
        assert!(took >= wait, "{took:?}");
        assert!(took < wait + Duration::from_millis(500), "{took:?}");
        // About one try every 50 milliseconds: never more often, and not far fewer.
        assert!((4..=8).contains(&tries), "{tries} tries");
        assert_eq!(fs::read(&file).unwrap(), b"old");
        assert_eq!(leftovers(&dir), ["x.json"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hold_that_ends_during_the_wait_lets_the_write_through() {
        let dir = folder("released");
        let file = dir.join("x.json");
        fs::write(&file, b"old").unwrap();
        let mut tries = 0u32;
        let mut replace = |h: &File, from: &Path, to: &Path| {
            tries += 1;
            if tries < 3 {
                Err(ReplaceFailure::Held)
            } else {
                sys::replace(h, from, to)
            }
        };
        write_atomically_with(
            &file,
            b"new",
            Duration::from_secs(5),
            Duration::from_millis(10),
            &mut replace,
        )
        .unwrap();
        assert_eq!(tries, 3);
        assert_eq!(fs::read(&file).unwrap(), b"new");
        assert_eq!(leftovers(&dir), ["x.json"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn any_other_failure_fails_at_once_leaving_no_temporary_file() {
        let dir = folder("other");
        let file = dir.join("x.json");
        fs::write(&file, b"old").unwrap();
        let mut tries = 0u32;
        let mut other = |_: &File, _: &Path, _: &Path| {
            tries += 1;
            Err(ReplaceFailure::Other)
        };
        let began = Instant::now();
        let e = write_atomically_with(&file, b"new", REPLACE_WAIT, RETRY_EVERY, &mut other)
            .unwrap_err();
        assert_ne!(e.kind(), io::ErrorKind::TimedOut);
        assert_eq!(tries, 1);
        assert!(began.elapsed() < Duration::from_secs(1));
        assert_eq!(fs::read(&file).unwrap(), b"old");
        assert_eq!(leftovers(&dir), ["x.json"]);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Opens `path` the way a process that shares only read would (the prototype's keeper
    /// readers), which blocks a rename over it.
    #[cfg(windows)]
    fn hold_sharing_read(path: &Path) -> File {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(path)
            .unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_a_file_held_without_sharing_delete_times_out_within_5_seconds() {
        let dir = folder("winheld");
        let file = dir.join("board-holder.json");
        fs::write(&file, b"old").unwrap();
        let held = hold_sharing_read(&file);
        let began = Instant::now();
        let e = write_atomically(&file, b"new").unwrap_err();
        let took = began.elapsed();
        drop(held);
        assert_eq!(e.kind(), io::ErrorKind::TimedOut, "{e:?}");
        assert!(took >= Duration::from_millis(4900), "{took:?}");
        assert!(took < Duration::from_millis(6500), "{took:?}");
        assert_eq!(fs::read(&file).unwrap(), b"old");
        assert_eq!(leftovers(&dir), ["board-holder.json"]);
        write_atomically(&file, b"new").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"new");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_a_hold_that_ends_during_the_wait_lets_the_write_through() {
        let dir = folder("winreleased");
        let file = dir.join("board-holder.json");
        fs::write(&file, b"old").unwrap();
        let held = hold_sharing_read(&file);
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(held);
        });
        let began = Instant::now();
        write_atomically(&file, b"new").unwrap();
        let took = began.elapsed();
        releaser.join().unwrap();
        assert!(took >= Duration::from_millis(250), "{took:?}");
        assert_eq!(fs::read(&file).unwrap(), b"new");
        assert_eq!(leftovers(&dir), ["board-holder.json"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
