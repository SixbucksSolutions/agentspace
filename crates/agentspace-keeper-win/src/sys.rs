//! The one module that calls Windows, and so the only one that MAY use `unsafe`.
//!
//! Each `unsafe` block is one Win32 or ntdll call, given buffers this module owns and handles it
//! holds open, and every handle it opens is an `OwnedHandle`, closed when dropped.

use crate::process::{self, Entry, Os, Timing};
use agentspace_keeper_api::{HolderRecord, Matcher, Processes, Query, Stopped};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::time::Duration;
use windows_sys::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessBasicInformation, ProcessCommandLineInformation,
};
use windows_sys::Win32::Foundation::{
    ERROR_NO_MORE_FILES, FALSE, FILETIME, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    UNICODE_STRING, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::Threading::{
    GetProcessId, GetProcessTimes, OpenProcess, PROCESS_BASIC_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess,
    WaitForSingleObject,
};

/// The rights a query asks for, and all it asks for: to read limited information and to wait
/// (keeper.md, Trusting the record, item 2).
pub(crate) const QUERY_ACCESS: u32 = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE;

/// The rights a stop asks for: a query's and the right to terminate (keeper.md, Stopping a
/// holder).
pub(crate) const STOP_ACCESS: u32 = QUERY_ACCESS | PROCESS_TERMINATE;

/// The exit code a stop gives each process it terminates.
// A terminated process's exit code is 1 (keeper.md, Stopping a holder; architect, 2026-11-19,
// for E0.06-T02-S03).
const TERMINATED_EXIT_CODE: u32 = 1;

/// `STATUS_INFO_LENGTH_MISMATCH`, and the two others ntdll gives for a buffer that is too small.
const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023_u32 as i32;
const STATUS_BUFFER_OVERFLOW: i32 = 0x8000_0005_u32 as i32;

/// The pinned `Processes` on Windows (keeper.md, Trusting the record; Stopping a holder).
#[derive(Debug, Clone, Copy, Default)]
pub struct WinProcesses;

impl WinProcesses {
    pub fn new() -> WinProcesses {
        WinProcesses
    }
}

impl Processes for WinProcesses {
    /// Opens the process with only `PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE`, and reads
    /// its creation time in UTC to the millisecond and its command line. A PID no process has, or
    /// a process that has exited, is `Gone`; any other failure to open it is `CannotOpen` with the
    /// Windows error.
    fn query(&self, pid: u32) -> Query {
        process::query(&Windows, pid)
    }

    /// Stops the holder's tree, and nothing else (keeper.md, Stopping a holder): never by
    /// `taskkill`, and never by a command line alone.
    fn stop(&self, holder: &HolderRecord, ssh_exe: &Path, matcher: &dyn Matcher) -> Stopped {
        process::stop(&Windows, holder, ssh_exe, matcher, Timing::KEEPER)
    }
}

/// The Windows system folder, as `GetSystemDirectoryW` reports it: keeper.md's `<System32>`, whose
/// `cmd.exe` runs a `.cmd` or `.bat` holder.
pub fn system_folder() -> std::io::Result<PathBuf> {
    let mut buf = vec![0u16; 260];
    loop {
        let cap = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        // SAFETY: `buf` is a writable buffer of `cap` UTF-16 units.
        let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), cap) } as usize;
        if n == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if n < buf.len() {
            buf.truncate(n);
            return Ok(PathBuf::from(OsString::from_wide(&buf)));
        }
        // Too small: `n` is the size needed, its terminating NUL included.
        buf.resize(n, 0);
    }
}

/// The real operating system behind `process`'s logic.
pub(crate) struct Windows;

fn last_error() -> u32 {
    // SAFETY: reads the calling thread's last error, and has no preconditions.
    unsafe { GetLastError() }
}

fn raw(h: &OwnedHandle) -> HANDLE {
    h.as_raw_handle() as HANDLE
}

/// Reads one fixed-size class of `NtQueryInformationProcess` into `T`.
fn query_fixed<T: Copy>(h: &OwnedHandle, class: i32, zeroed: T) -> Result<T, u32> {
    let mut out = zeroed;
    let mut len = 0u32;
    let size = u32::try_from(std::mem::size_of::<T>()).unwrap_or(u32::MAX);
    // SAFETY: `out` is a writable `T` of `size` bytes, and `h` is an open process handle.
    let status = unsafe {
        NtQueryInformationProcess(
            raw(h),
            class,
            (&raw mut out).cast::<core::ffi::c_void>(),
            size,
            &mut len,
        )
    };
    if status < 0 {
        Err(status as u32)
    } else {
        Ok(out)
    }
}

impl Os for Windows {
    type Handle = OwnedHandle;

    fn open(&self, pid: u32, terminate: bool) -> Result<OwnedHandle, u32> {
        let access = if terminate { STOP_ACCESS } else { QUERY_ACCESS };
        // SAFETY: no pointers; `FALSE` makes the handle not inheritable.
        let h = unsafe { OpenProcess(access, FALSE, pid) };
        if h.is_null() {
            return Err(last_error());
        }
        // SAFETY: `h` is a process handle just opened and owned by nothing else.
        let h = unsafe { OwnedHandle::from_raw_handle(h) };
        // Windows ignores the two low bits of a PID, so a number that is no process's ID opens
        // the process just below it. The process opened MUST be the one asked for: a record whose
        // PID is not that process's own names no process (state.md, The holder record), and a
        // stop finds a tree by the PID it was given.
        // SAFETY: `h` is an open process handle with the right to read limited information.
        let opened = unsafe { GetProcessId(raw(&h)) };
        if opened == 0 {
            return Err(last_error());
        }
        if opened != pid {
            return Err(process::ERROR_INVALID_PARAMETER);
        }
        Ok(h)
    }

    fn exited(&self, h: &OwnedHandle) -> Result<bool, u32> {
        self.wait_exit(h, Duration::ZERO)
    }

    fn wait_exit(&self, h: &OwnedHandle, timeout: Duration) -> Result<bool, u32> {
        // Below INFINITE (u32::MAX) always.
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
        // SAFETY: `h` is an open process handle with the right to wait on it.
        let r = unsafe { WaitForSingleObject(raw(h), ms) };
        match r {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            WAIT_FAILED => Err(last_error()),
            other => Err(other),
        }
    }

    fn created(&self, h: &OwnedHandle) -> Result<u64, u32> {
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        // SAFETY: four writable FILETIMEs, and `h` has the right to read limited information.
        let ok =
            unsafe { GetProcessTimes(raw(h), &mut created, &mut exited, &mut kernel, &mut user) };
        if ok == FALSE {
            return Err(last_error());
        }
        Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }

    fn command_line(&self, h: &OwnedHandle) -> Option<String> {
        // `ProcessCommandLineInformation` needs only the right to read limited information. It
        // gives a UNICODE_STRING whose text follows it in the same buffer. A buffer of u64s keeps
        // the UNICODE_STRING aligned.
        let mut words = 64usize;
        for _ in 0..4 {
            let mut buf = vec![0u64; words];
            let size = u32::try_from(words * 8).ok()?;
            let mut needed = 0u32;
            // SAFETY: `buf` is a writable buffer of `size` bytes, and `h` is an open process
            // handle.
            let status = unsafe {
                NtQueryInformationProcess(
                    raw(h),
                    ProcessCommandLineInformation,
                    buf.as_mut_ptr().cast::<core::ffi::c_void>(),
                    size,
                    &mut needed,
                )
            };
            if matches!(
                status,
                STATUS_INFO_LENGTH_MISMATCH | STATUS_BUFFER_TOO_SMALL | STATUS_BUFFER_OVERFLOW
            ) {
                words = (needed as usize).div_ceil(8).max(words * 2);
                continue;
            }
            if status < 0 {
                return None;
            }
            return unicode_string_text(&buf);
        }
        None
    }

    fn parent_pid(&self, h: &OwnedHandle) -> Result<u32, u32> {
        // SAFETY: PROCESS_BASIC_INFORMATION is plain data, for which all zeroes is valid.
        let zeroed: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let info = query_fixed(h, ProcessBasicInformation, zeroed)?;
        u32::try_from(info.InheritedFromUniqueProcessId).map_err(|_| 0)
    }

    fn processes(&self) -> Result<Vec<Entry>, u32> {
        // SAFETY: no pointers.
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return Err(last_error());
        }
        // SAFETY: `snap` is a handle just opened and owned by nothing else.
        let snap = unsafe { OwnedHandle::from_raw_handle(snap) };
        // SAFETY: PROCESSENTRY32W is plain data, for which all zeroes is valid.
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = u32::try_from(std::mem::size_of::<PROCESSENTRY32W>()).unwrap_or(0);
        let mut list = Vec::new();
        // SAFETY: `entry` is a writable PROCESSENTRY32W with its size set.
        let mut ok = unsafe { Process32FirstW(raw(&snap), &mut entry) };
        while ok != FALSE {
            list.push(Entry {
                pid: entry.th32ProcessID,
                parent: entry.th32ParentProcessID,
            });
            // SAFETY: as above.
            ok = unsafe { Process32NextW(raw(&snap), &mut entry) };
        }
        // ERROR_NO_MORE_FILES ends the list; anything else is an error.
        let e = last_error();
        if e == ERROR_NO_MORE_FILES {
            Ok(list)
        } else {
            Err(e)
        }
    }

    fn terminate(&self, h: &OwnedHandle) -> Result<(), u32> {
        // SAFETY: `h` is an open process handle with the right to terminate it.
        let ok = unsafe { TerminateProcess(raw(h), TERMINATED_EXIT_CODE) };
        if ok == FALSE {
            Err(last_error())
        } else {
            Ok(())
        }
    }

    fn own_pid(&self) -> u32 {
        std::process::id()
    }
}

/// The text of the UNICODE_STRING at the start of `buf`, whose characters MUST lie inside `buf`.
fn unicode_string_text(buf: &[u64]) -> Option<String> {
    let bytes = std::mem::size_of_val(buf);
    if bytes < std::mem::size_of::<UNICODE_STRING>() {
        return None;
    }
    // SAFETY: `buf` holds at least one UNICODE_STRING, and is aligned for it (8 bytes).
    let us: UNICODE_STRING = unsafe { std::ptr::read(buf.as_ptr().cast::<UNICODE_STRING>()) };
    let len = usize::from(us.Length);
    if len == 0 {
        return Some(String::new());
    }
    let start = buf.as_ptr() as usize;
    let at = (us.Buffer as usize).checked_sub(start)?;
    if at % 2 != 0 || at.checked_add(len)? > bytes || len % 2 != 0 {
        return None;
    }
    // SAFETY: the `len` bytes at offset `at` lie inside `buf`, as checked just above, and `at` is
    // even in a buffer aligned to 8 bytes, so they are `len / 2` aligned UTF-16 units. They are
    // read through `buf`'s own pointer, never through the one Windows wrote.
    let units = unsafe {
        std::slice::from_raw_parts(buf.as_ptr().cast::<u8>().add(at).cast::<u16>(), len / 2)
    };
    String::from_utf16(units).ok()
}

/// A job object that ends every process in it when it is dropped, so a unit test that fails
/// leaves none of the processes it started running.
#[cfg(test)]
pub(crate) struct KillOnDrop(OwnedHandle);

#[cfg(test)]
impl KillOnDrop {
    pub(crate) fn new() -> KillOnDrop {
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };
        // SAFETY: no security attributes and no name.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        assert!(!job.is_null(), "CreateJobObjectW failed");
        // SAFETY: `job` was just created and is owned by nothing else.
        let job = unsafe { OwnedHandle::from_raw_handle(job) };
        // SAFETY: plain data, for which all zeroes is valid.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `info` is a readable JOBOBJECT_EXTENDED_LIMIT_INFORMATION of the size given.
        let ok = unsafe {
            SetInformationJobObject(
                raw(&job),
                JobObjectExtendedLimitInformation,
                (&raw const info).cast::<core::ffi::c_void>(),
                u32::try_from(std::mem::size_of_val(&info)).unwrap(),
            )
        };
        assert!(ok != FALSE, "SetInformationJobObject failed");
        KillOnDrop(job)
    }

    pub(crate) fn add(&self, child: &std::process::Child) {
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
        // SAFETY: both handles are open, the child's for as long as `child` lives.
        let ok = unsafe { AssignProcessToJobObject(raw(&self.0), child.as_raw_handle() as HANDLE) };
        assert!(ok != FALSE, "AssignProcessToJobObject failed");
    }
}

/// Whether a handle would be inherited by a child process.
#[cfg(test)]
pub(crate) fn is_inheritable(h: &impl AsRawHandle) -> bool {
    use windows_sys::Win32::Foundation::{GetHandleInformation, HANDLE_FLAG_INHERIT};
    let mut flags = 0u32;
    // SAFETY: `h` is an open handle, and `flags` is writable.
    let ok = unsafe { GetHandleInformation(h.as_raw_handle() as HANDLE, &mut flags) };
    assert!(ok != FALSE, "GetHandleInformation failed");
    flags & HANDLE_FLAG_INHERIT != 0
}
