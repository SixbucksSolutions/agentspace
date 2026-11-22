//! The push's Windows calls, and the only module of this crate with `unsafe` (operator.md, The Bot
//! credential; What the laptop sends, steps 4 and 5).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - [`unprotect`] is `CryptUnprotectData`, with the arguments The Bot credential's table gives.
//! - [`wait_for_exit`] waits on the ssh's process handle until a deadline.
//! - [`terminate_tree`] stops the ssh and every process descended from it through the Windows
//!   API, the ssh last, and no other process: it starts neither `taskkill` nor anything else
//!   (operator.md, Programs it starts, the `icacls.exe`, `taskkill.exe` row).
//! - [`StderrPipe`] is the ssh's stderr pipe as the caller-mode relay reads it: `PeekNamedPipe`
//!   says how much it holds, so the relay never blocks on it (operator.md, What the laptop sends,
//!   step 2).

use std::collections::HashMap;
use std::io::Read;
use std::os::windows::io::AsRawHandle;
use std::process::{Child, ChildStderr};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    TerminateProcess, WaitForSingleObject,
};
use zeroize::{Zeroize, Zeroizing};

/// Decrypts a DPAPI blob for the Windows user the program runs as: `CryptUnprotectData` on `blob`
/// with no description wanted, no entropy, no reserved argument, no prompt structure and
/// `CRYPTPROTECT_UI_FORBIDDEN`, so it never shows a prompt. `None` when the call fails: a blob
/// written by another user or on another computer, a corrupted one, or a profile whose DPAPI keys
/// are not available.
///
/// The plaintext is copied into a buffer that zeroes itself when dropped, and the buffer the
/// system allocated is zeroed and then freed with `LocalFree` before this returns.
pub fn unprotect(blob: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    let len = u32::try_from(blob.len()).ok()?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: len,
        pbData: blob.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    // SAFETY: `input` points at `blob`, which outlives the call and is only read. `output` is a
    // valid blob for the call to fill; the optional arguments are the NULLs the spec gives.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            null_mut(),
            null(),
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if output.pbData.is_null() {
        return None;
    }
    let plain = if ok != 0 {
        // SAFETY: on success `pbData` points at `cbData` bytes that the system allocated for us.
        let bytes =
            unsafe { std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize) };
        let copy = Zeroizing::new(bytes.to_vec());
        bytes.zeroize();
        Some(copy)
    } else {
        None
    };
    // SAFETY: the buffer was allocated by the call above, which the spec says is freed with
    // `LocalFree`, and nothing refers to it any more.
    unsafe { LocalFree(output.pbData.cast()) };
    plain
}

/// The ssh's stderr pipe, for the caller-mode relay.
pub struct StderrPipe(ChildStderr);

/// The relay's reader of the ssh's stderr. Always `Some` here.
pub fn stderr_pipe(pipe: ChildStderr) -> Option<StderrPipe> {
    Some(StderrPipe(pipe))
}

impl crate::relay::Source for StderrPipe {
    /// What `PeekNamedPipe` says the pipe holds. A pipe whose every writer has closed it and that
    /// holds nothing makes the call fail (`ERROR_BROKEN_PIPE`), which is its end; any other failure
    /// is taken as the end too.
    fn available(&mut self) -> Option<usize> {
        let handle = self.0.as_raw_handle() as HANDLE;
        let mut total: u32 = 0;
        // SAFETY: `handle` is the pipe's read end, which `self.0` keeps open. No buffer is passed,
        // so nothing is copied; `total` is a local the call fills.
        let ok =
            unsafe { PeekNamedPipe(handle, null_mut(), 0, null_mut(), &mut total, null_mut()) };
        (ok != 0).then_some(total as usize)
    }

    /// A read of no more than the pipe holds, which returns at once.
    fn read(&mut self, buf: &mut [u8]) -> usize {
        self.0.read(buf).unwrap_or(0)
    }
}

/// Waits until the process has exited or `deadline` passes. `true` when it has exited.
///
/// It waits on the process's handle, so it returns the moment the process ends. If the wait itself
/// fails it polls instead.
pub fn wait_for_exit(child: &mut Child, deadline: Instant) -> bool {
    let handle = child.as_raw_handle() as HANDLE;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let millis =
            u32::try_from(remaining.as_millis()).map_or(u32::MAX - 1, |m| m.min(u32::MAX - 1));
        // SAFETY: `handle` is the child's own process handle, which `child` keeps open.
        let state = unsafe { WaitForSingleObject(handle, millis) };
        if state == WAIT_OBJECT_0 {
            return true;
        }
        if state == WAIT_TIMEOUT {
            if Instant::now() >= deadline {
                return false;
            }
            continue;
        }
        // The wait failed: poll.
        loop {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// A process this module has opened, closed when dropped. Holding the handle keeps the process's
/// ID from being given to another process while the tree is being stopped.
struct Held {
    created: u64,
    handle: HANDLE,
}

impl Drop for Held {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by `OpenProcess` and is closed once, here.
        unsafe { CloseHandle(self.handle) };
    }
}

/// A `FILETIME` as one number of 100-nanosecond ticks.
fn ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

/// When the process was created, or `None` when it cannot be read.
fn created_at(handle: HANDLE) -> Option<u64> {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: `handle` is an open process handle and the four times are valid for the call to fill.
    let ok = unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    (ok != 0).then(|| ticks(created))
}

/// Every process's ID and its parent's ID, as one snapshot of the system gives them.
fn snapshot() -> Option<Vec<(u32, u32)>> {
    // SAFETY: a plain call that returns a snapshot handle, or `INVALID_HANDLE_VALUE`.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE || snap.is_null() {
        return None;
    }
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut found = Vec::new();
    // SAFETY: `snap` is a valid snapshot and `entry` is a valid entry with its size set.
    let mut more = unsafe { Process32FirstW(snap, &mut entry) };
    while more != 0 {
        found.push((entry.th32ProcessID, entry.th32ParentProcessID));
        // SAFETY: as above.
        more = unsafe { Process32NextW(snap, &mut entry) };
    }
    // SAFETY: `snap` was opened above and is closed once, here.
    unsafe { CloseHandle(snap) };
    Some(found)
}

/// The ssh and the processes found to descend from it so far.
struct Tree {
    /// The creation time of each process in the tree, the ssh included, by process ID.
    known: HashMap<u32, u64>,
    /// The processes below the ssh, opened for termination, in the order they were found: a
    /// parent always comes before its children.
    held: Vec<Held>,
}

impl Tree {
    /// Finds the processes that are descendants of one already in the tree and not yet in it, adds
    /// them, and returns their positions in `held`, parents first.
    ///
    /// A process is a descendant when its parent's ID is that of a process in the tree and it was
    /// created no earlier than that parent. The second condition keeps a process that merely
    /// carries a parent ID that was later given to another process, as a former child of a
    /// long-gone process would, from being taken for a descendant: no process is stopped but the
    /// ssh's own.
    fn sweep(&mut self) -> Vec<usize> {
        let Some(processes) = snapshot() else {
            return Vec::new();
        };
        let mut found = Vec::new();
        loop {
            let mut grew = false;
            for &(pid, parent) in &processes {
                if self.known.contains_key(&pid) {
                    continue;
                }
                let Some(&parent_created) = self.known.get(&parent) else {
                    continue;
                };
                // SAFETY: a plain call that returns a handle, or NULL.
                let handle = unsafe {
                    OpenProcess(
                        PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                        0,
                        pid,
                    )
                };
                if handle.is_null() {
                    continue;
                }
                let held = Held {
                    created: created_at(handle).unwrap_or(0),
                    handle,
                };
                if held.created == 0 || held.created < parent_created {
                    continue;
                }
                self.known.insert(pid, held.created);
                self.held.push(held);
                found.push(self.held.len() - 1);
                grew = true;
            }
            if !grew {
                return found;
            }
        }
    }

    /// Terminates the processes at `positions`, the last found first, so a child goes before its
    /// parent and a parent cannot start another.
    fn terminate(&self, positions: &[usize]) {
        for &i in positions.iter().rev() {
            terminate(self.held[i].handle);
        }
    }
}

fn terminate(handle: HANDLE) {
    // SAFETY: `handle` is an open process handle with the right to terminate it. A process that
    // has already ended makes the call fail, which does not matter.
    unsafe { TerminateProcess(handle, 1) };
}

/// Stops the ssh and every process whose chain of parent processes leads to it, forcibly and
/// without asking any of them to exit, through the Windows API, and no other process. Each is
/// terminated at once, a child before its parent, and the ssh itself last. It reads nothing and
/// records nothing.
///
/// The tree is found from the ssh's process ID, afresh, each time round: a process the ssh's tree
/// starts while it is being stopped is found by the next sweep, and the last sweeps run after the
/// ssh has been terminated, with its handle still open so that its ID is still its own.
pub fn terminate_tree(child: &mut Child) {
    let root = child.as_raw_handle() as HANDLE;
    let Some(root_created) = created_at(root) else {
        // Not even its times can be read: stop the ssh alone.
        terminate(root);
        return;
    };
    let mut tree = Tree {
        known: HashMap::from([(child.id(), root_created)]),
        held: Vec::new(),
    };
    for _ in 0..16 {
        let found = tree.sweep();
        if found.is_empty() {
            break;
        }
        tree.terminate(&found);
    }
    terminate(root);
    for _ in 0..4 {
        let found = tree.sweep();
        if found.is_empty() {
            break;
        }
        tree.terminate(&found);
    }
}
