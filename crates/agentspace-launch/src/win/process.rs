//! Looking at a process (docs/keeper.md, Trusting the record): its creation time and its command
//! line, read through a handle with only the rights to read its limited information and to wait
//! on it.

use super::{Owned, filetime_ms};
use crate::time::format_ms;
use agentspace_keeper_api::{ProcessInfo, Query};
use windows_sys::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessCommandLineInformation,
};
use windows_sys::Win32::Foundation::{
    ERROR_INVALID_PARAMETER, FILETIME, GetLastError, HANDLE, UNICODE_STRING, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    WaitForSingleObject,
};

/// A process's creation time, UTC, truncated to the millisecond: `YYYY-MM-DDTHH:MM:SS.fffZ`.
pub(crate) fn creation_time(process: HANDLE) -> Option<String> {
    let mut times = [FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    }; 4];
    let [created, exited, kernel, user] = &mut times;
    // SAFETY: the handle is open with at least limited query rights; the four out-parameters are
    // ours.
    let ok = unsafe { GetProcessTimes(process, created, exited, kernel, user) };
    if ok == 0 {
        return None;
    }
    Some(format_ms(filetime_ms(times[0])))
}

/// A process's command line, read back from it, or `None` when it cannot be read.
pub(crate) fn command_line(process: HANDLE) -> Option<String> {
    // The answer is a UNICODE_STRING followed by its text. A u64 buffer keeps it aligned.
    let mut size: u32 = 0;
    // SAFETY: a call with no buffer only gives the size the answer needs.
    unsafe {
        NtQueryInformationProcess(
            process,
            ProcessCommandLineInformation,
            std::ptr::null_mut(),
            0,
            &mut size,
        )
    };
    let size = size.max(std::mem::size_of::<UNICODE_STRING>() as u32);
    let mut buffer = vec![0u64; (size as usize).div_ceil(8) + 1];
    let length = (buffer.len() * 8) as u32;
    let mut written: u32 = 0;
    // SAFETY: the buffer is ours, aligned for a UNICODE_STRING, and its length is given.
    let status = unsafe {
        NtQueryInformationProcess(
            process,
            ProcessCommandLineInformation,
            buffer.as_mut_ptr().cast(),
            length,
            &mut written,
        )
    };
    if status < 0 {
        return None;
    }
    // SAFETY: on success the buffer starts with a UNICODE_STRING whose text lies inside the
    // buffer, as the call wrote it.
    unsafe {
        let s = &*(buffer.as_ptr().cast::<UNICODE_STRING>());
        if s.Buffer.is_null() || s.Length == 0 {
            return Some(String::new());
        }
        let start = s.Buffer as usize;
        let base = buffer.as_ptr() as usize;
        let units = usize::from(s.Length) / 2;
        if start < base || start + units * 2 > base + buffer.len() * 8 {
            return None;
        }
        let text = std::slice::from_raw_parts(s.Buffer, units);
        Some(String::from_utf16_lossy(text))
    }
}

/// Looks at a process once, with no wait.
pub(crate) fn query(pid: u32) -> Query {
    // SAFETY: OpenProcess takes no pointers; a null handle is checked.
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        // SAFETY: reads this thread's last error.
        let error = unsafe { GetLastError() };
        return if error == ERROR_INVALID_PARAMETER {
            Query::Gone
        } else {
            Query::CannotOpen(error)
        };
    }
    let handle = Owned(handle);
    // SAFETY: the handle is open with the right to wait on it; a wait of 0 never blocks.
    if unsafe { WaitForSingleObject(handle.0, 0) } == WAIT_OBJECT_0 {
        return Query::Gone;
    }
    let Some(process_started_at) = creation_time(handle.0) else {
        // SAFETY: reads this thread's last error.
        return Query::CannotOpen(unsafe { GetLastError() });
    };
    Query::Running(ProcessInfo {
        process_started_at,
        command_line: command_line(handle.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_running_with_its_own_command_line_and_a_millisecond_time() {
        let Query::Running(info) = query(std::process::id()) else {
            panic!("This process was not found running.");
        };
        assert_eq!(info.process_started_at.len(), 24);
        assert!(info.process_started_at.ends_with('Z'));
        let line = info.command_line.unwrap();
        let exe = std::env::current_exe().unwrap();
        let name = exe.file_stem().unwrap().to_string_lossy().into_owned();
        assert!(
            line.contains(&name),
            "The command line does not name the test."
        );
    }

    #[test]
    fn a_process_that_ended_is_gone() {
        let home = crate::testing::Scratch::new("gone");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .current_dir(home.path())
            .env("USERPROFILE", home.path())
            .env("HOME", home.path())
            .env("LOCALAPPDATA", home.path())
            .env("APPDATA", home.path())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        drop(child);
        assert_eq!(query(pid), Query::Gone);
    }
}
