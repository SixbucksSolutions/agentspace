//! The one module that calls Windows, and so the only one that MAY use `unsafe`.
//!
//! RFC 2119 keywords; the capitals are load-bearing.
//!
//! Each `unsafe` block is one Win32 or ntdll call, given buffers this module owns and handles it
//! holds open, and every handle it opens is an `OwnedHandle`, closed when dropped. Nothing it reads
//! (a command line, an image path, a user) leaves it but to `walk`, which keeps it in memory.

use crate::walk::{Access, ERROR_INVALID_PARAMETER, Entry, ProcessTable};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::time::Duration;
use windows_sys::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessBasicInformation, ProcessCommandLineInformation,
};
use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_FILES, FALSE, FILETIME, GetLastError, HANDLE,
    INVALID_HANDLE_VALUE, UNICODE_STRING, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessId, GetProcessTimes, OpenProcess, OpenProcessToken,
    PROCESS_BASIC_INFORMATION, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess,
    WaitForSingleObject,
};

/// The rights a look asks for, and all it asks for: to read limited information (the token's
/// user, the image, the command line, the times) and to wait.
const QUERY_ACCESS: u32 = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE;

/// The rights an end asks for: a look's and the right to terminate.
const TERMINATE_ACCESS: u32 = QUERY_ACCESS | PROCESS_TERMINATE;

/// `STATUS_INFO_LENGTH_MISMATCH`, and the two others ntdll gives for a buffer that is too small.
const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023_u32 as i32;
const STATUS_BUFFER_OVERFLOW: i32 = 0x8000_0005_u32 as i32;

/// The longest Win32 path, in UTF-16 units, an image path is read up to.
const MAX_IMAGE_PATH: usize = 32_768;

/// The real process table, with the walking process's own user read once.
pub(crate) struct WinTable {
    /// The walking process's token's user, as the bytes of its SID.
    own_user: Vec<u8>,
}

impl WinTable {
    /// Reads the walking process's own user. `Err` is the Windows error when it cannot be read,
    /// since no process could then be shown to run as that user.
    pub(crate) fn new() -> Result<WinTable, u32> {
        // SAFETY: a pseudo-handle for the current process, which needs no closing.
        let me = unsafe { GetCurrentProcess() };
        let own_user = token_user(me)?;
        Ok(WinTable { own_user })
    }
}

fn last_error() -> u32 {
    // SAFETY: reads the calling thread's last error, and has no preconditions.
    unsafe { GetLastError() }
}

fn raw(h: &OwnedHandle) -> HANDLE {
    h.as_raw_handle() as HANDLE
}

/// The SID of the user of the token of the process `process`, as its bytes.
fn token_user(process: HANDLE) -> Result<Vec<u8>, u32> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: `process` is an open process handle or the current process's pseudo-handle, and
    // `token` is writable.
    let ok = unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) };
    if ok == FALSE || token.is_null() {
        return Err(last_error());
    }
    // SAFETY: `token` was just opened and is owned by nothing else.
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut words = 16usize;
    for _ in 0..4 {
        // A buffer of u64s keeps the TOKEN_USER aligned.
        let mut buf = vec![0u64; words];
        let size = u32::try_from(words * 8).map_err(|_| ERROR_INSUFFICIENT_BUFFER)?;
        let mut needed = 0u32;
        // SAFETY: `buf` is a writable buffer of `size` bytes, and `token` has TOKEN_QUERY.
        let ok = unsafe {
            GetTokenInformation(
                raw(&token),
                TokenUser,
                buf.as_mut_ptr().cast::<core::ffi::c_void>(),
                size,
                &mut needed,
            )
        };
        if ok == FALSE {
            let e = last_error();
            if e == ERROR_INSUFFICIENT_BUFFER {
                words = (needed as usize).div_ceil(8).max(words * 2);
                continue;
            }
            return Err(e);
        }
        return sid_bytes(&buf).ok_or(ERROR_INSUFFICIENT_BUFFER);
    }
    Err(ERROR_INSUFFICIENT_BUFFER)
}

/// The bytes of the SID the TOKEN_USER at the start of `buf` points to, which MUST lie inside
/// `buf`. A SID is 8 bytes and then 4 for each of its sub-authorities, whose count is its second
/// byte.
fn sid_bytes(buf: &[u64]) -> Option<Vec<u8>> {
    let bytes = std::mem::size_of_val(buf);
    if bytes < std::mem::size_of::<TOKEN_USER>() {
        return None;
    }
    // SAFETY: `buf` holds at least one TOKEN_USER, and is aligned for it (8 bytes).
    let user: TOKEN_USER = unsafe { std::ptr::read(buf.as_ptr().cast::<TOKEN_USER>()) };
    let at = (user.User.Sid as usize).checked_sub(buf.as_ptr() as usize)?;
    // SAFETY: `buf` is `bytes` bytes long, and is read as bytes through its own pointer.
    let all = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), bytes) };
    let head = all.get(at..at.checked_add(8)?)?;
    let len = 8 + 4 * usize::from(head[1]);
    all.get(at..at.checked_add(len)?).map(<[u8]>::to_vec)
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
    let at = (us.Buffer as usize).checked_sub(buf.as_ptr() as usize)?;
    if at % 2 != 0 || len % 2 != 0 || at.checked_add(len)? > bytes {
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

impl ProcessTable for WinTable {
    type Handle = OwnedHandle;

    fn list(&self) -> Result<Vec<Entry>, u32> {
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

    fn own_pid(&self) -> u32 {
        std::process::id()
    }

    fn open(&self, pid: u32, access: Access) -> Result<OwnedHandle, u32> {
        let rights = match access {
            Access::Query => QUERY_ACCESS,
            Access::Terminate => TERMINATE_ACCESS,
        };
        // SAFETY: no pointers; `FALSE` makes the handle not inheritable.
        let h = unsafe { OpenProcess(rights, FALSE, pid) };
        if h.is_null() {
            return Err(last_error());
        }
        // SAFETY: `h` is a process handle just opened and owned by nothing else.
        let h = unsafe { OwnedHandle::from_raw_handle(h) };
        // Windows ignores the two low bits of a process ID, so a number that is no process's ID
        // can open the process just below it. The process opened MUST be the one asked for.
        // SAFETY: `h` is an open process handle with the right to read limited information.
        let opened = unsafe { GetProcessId(raw(&h)) };
        if opened == 0 {
            return Err(last_error());
        }
        if opened != pid {
            return Err(ERROR_INVALID_PARAMETER);
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

    fn parent_pid(&self, h: &OwnedHandle) -> Result<u32, u32> {
        // SAFETY: PROCESS_BASIC_INFORMATION is plain data, for which all zeroes is valid.
        let mut info: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let size = u32::try_from(std::mem::size_of::<PROCESS_BASIC_INFORMATION>())
            .map_err(|_| ERROR_INSUFFICIENT_BUFFER)?;
        let mut len = 0u32;
        // SAFETY: `info` is a writable PROCESS_BASIC_INFORMATION of `size` bytes, and `h` is an
        // open process handle with the right to read limited information.
        let status = unsafe {
            NtQueryInformationProcess(
                raw(h),
                ProcessBasicInformation,
                (&raw mut info).cast::<core::ffi::c_void>(),
                size,
                &mut len,
            )
        };
        if status < 0 {
            return Err(status as u32);
        }
        u32::try_from(info.InheritedFromUniqueProcessId).map_err(|_| ERROR_INVALID_PARAMETER)
    }

    fn is_own_user(&self, h: &OwnedHandle) -> Option<bool> {
        token_user(raw(h)).ok().map(|user| user == self.own_user)
    }

    fn image_path(&self, h: &OwnedHandle) -> Option<String> {
        let mut units = 1024usize;
        loop {
            let mut buf = vec![0u16; units];
            let mut size = u32::try_from(units).ok()?;
            // SAFETY: `buf` is a writable buffer of `size` UTF-16 units, and `h` has the right to
            // read limited information.
            let ok = unsafe {
                QueryFullProcessImageNameW(raw(h), PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size)
            };
            if ok != FALSE {
                buf.truncate(usize::try_from(size).ok()?.min(units));
                return String::from_utf16(&buf).ok();
            }
            if last_error() != ERROR_INSUFFICIENT_BUFFER || units >= MAX_IMAGE_PATH {
                return None;
            }
            units = (units * 4).min(MAX_IMAGE_PATH);
        }
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

    fn terminate(&self, h: &OwnedHandle, exit_code: u32) -> Result<(), u32> {
        // SAFETY: `h` is an open process handle with the right to terminate it.
        let ok = unsafe { TerminateProcess(raw(h), exit_code) };
        if ok == FALSE {
            Err(last_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These read only the test's own process, through the real calls: never another process's
    // command line, path or user, and nothing is printed.

    #[test]
    fn the_own_process_reads_as_its_own_user_with_its_image_and_parent() {
        let table = WinTable::new().expect("the own user reads");
        let h = table
            .open(std::process::id(), Access::Query)
            .expect("the own process opens");
        assert_eq!(table.is_own_user(&h), Some(true));
        assert_eq!(table.exited(&h), Ok(false));
        assert!(table.created(&h).is_ok_and(|c| c > 0));
        let image = table.image_path(&h).expect("the own image reads");
        let exe = std::env::current_exe().expect("current_exe");
        let name = exe.file_name().and_then(|n| n.to_str()).expect("a name");
        assert!(image.to_lowercase().ends_with(&name.to_lowercase()));
        assert!(table.command_line(&h).is_some_and(|c| !c.is_empty()));
        let parent = table.parent_pid(&h).expect("the parent reads");
        let list = table.list().expect("the list reads");
        let own = list
            .iter()
            .find(|e| e.pid == std::process::id())
            .expect("the own process is listed");
        assert_eq!(own.parent, parent);
    }

    #[test]
    fn an_id_no_process_has_does_not_open() {
        let table = WinTable::new().expect("the own user reads");
        // Process IDs are multiples of 4, so this one names no process, and MUST NOT open the
        // process just below it.
        let odd = std::process::id() | 3;
        assert_eq!(
            table.open(odd, Access::Query).err(),
            Some(ERROR_INVALID_PARAMETER)
        );
    }
}
