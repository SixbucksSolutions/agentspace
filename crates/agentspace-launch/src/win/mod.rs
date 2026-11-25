//! The Windows half: the only modules of this crate that call Windows, and so the only ones with
//! `unsafe` code. `wmi` creates the launcher, `spawn` creates the holder, and `process` looks at
//! a process; this module holds what they share.

pub(crate) mod process;
pub(crate) mod spawn;
pub(crate) mod wmi;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS, FormatMessageW, GetErrorMode,
    SEM_FAILCRITICALERRORS, SEM_NOOPENFILEERRORBOX, SetErrorMode,
};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

/// FILETIME ticks (100 ns since 1601-01-01) at the Unix epoch.
const EPOCH_AS_FILETIME: i64 = 116_444_736_000_000_000;

/// A string as Windows takes it: UTF-16, ending in a NUL.
pub(crate) fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// A FILETIME as milliseconds since the Unix epoch, truncated.
pub(crate) fn filetime_ms(ft: FILETIME) -> i64 {
    let ticks = (i64::from(ft.dwHighDateTime) << 32) | i64::from(ft.dwLowDateTime);
    (ticks - EPOCH_AS_FILETIME).div_euclid(10_000)
}

/// Windows' own text for an error number, without its line ending, or `Windows error <n>.` when
/// Windows has none.
pub(crate) fn error_text(code: u32) -> String {
    let mut buffer = [0u16; 1024];
    // SAFETY: the buffer is ours and its length is given; no inserts are read.
    let n = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
            std::ptr::null(),
            code,
            0,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            std::ptr::null(),
        )
    } as usize;
    let text = String::from_utf16_lossy(&buffer[..n.min(buffer.len())]);
    let text = text.trim_end();
    if text.is_empty() {
        format!("Windows error {code}.")
    } else {
        text.to_string()
    }
}

/// `<System32>`, the Windows system folder as Windows reports it.
pub(crate) fn system32() -> Option<PathBuf> {
    let mut buffer = [0u16; 1024];
    // SAFETY: the buffer is ours and its length is given.
    let n = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if n == 0 || n >= buffer.len() {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..n])))
}

/// The process's error mode gains `SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX`, keeping every
/// other bit it had (`GetErrorMode`, then `SetErrorMode` with the union). The crate root's
/// `quiet_hard_errors` documents it.
pub(crate) fn quiet_hard_errors() {
    // SAFETY: reads and sets this process's own error mode; no pointer is passed.
    unsafe { SetErrorMode(GetErrorMode() | SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX) };
}

/// A handle closed when it is dropped.
pub(crate) struct Owned(pub HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle is ours, and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_in_filetime_is_zero_milliseconds() {
        let ft = FILETIME {
            dwLowDateTime: (EPOCH_AS_FILETIME & 0xFFFF_FFFF) as u32,
            dwHighDateTime: (EPOCH_AS_FILETIME >> 32) as u32,
        };
        assert_eq!(filetime_ms(ft), 0);
    }

    #[test]
    fn an_error_number_has_windows_text_starting_with_a_capital() {
        let text = error_text(2);
        assert!(text.starts_with(char::is_uppercase), "{text}");
        assert!(!text.ends_with('\n'));
        assert!(system32().is_some_and(|p| p.is_absolute()));
    }
}
