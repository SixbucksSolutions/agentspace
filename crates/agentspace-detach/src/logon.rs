//! The user's logon environment, as `CreateEnvironmentBlock` builds it from this process's token
//! with `bInherit` FALSE: the system's and the user's variables, and nothing of this process's own
//! environment. No other user's profile is loaded.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{TOKEN_DUPLICATE, TOKEN_QUERY};
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// This process's token, closed when dropped.
struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: the handle is ours, and is closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// An environment block, destroyed when dropped.
struct Block(*mut core::ffi::c_void);

impl Drop for Block {
    fn drop(&mut self) {
        // SAFETY: the block came from CreateEnvironmentBlock, and is destroyed once.
        let _ = unsafe { DestroyEnvironmentBlock(self.0) };
    }
}

/// Every `NAME=VALUE` entry of the logon environment, in the block's order.
pub(crate) fn environment() -> Result<Vec<OsString>, String> {
    let mut token = HANDLE::default();
    // SAFETY: the current process's pseudo handle and an out-parameter of ours.
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY | TOKEN_DUPLICATE,
            &mut token,
        )
    }
    .map_err(|e| format!("This process's token could not be opened: {}", text(&e)))?;
    let token = Token(token);
    let mut raw = std::ptr::null_mut();
    // SAFETY: a token opened with the access CreateEnvironmentBlock needs, and an out-parameter
    // of ours.
    unsafe { CreateEnvironmentBlock(&mut raw, Some(token.0), false) }
        .map_err(|e| format!("The logon environment could not be read: {}", text(&e)))?;
    if raw.is_null() {
        return Err("The logon environment could not be read.".to_string());
    }
    let block = Block(raw);
    let mut entries = Vec::new();
    let mut at = block.0.cast::<u16>().cast_const();
    loop {
        let mut length = 0usize;
        // SAFETY: a Unicode environment block is NUL-ended strings, ended by an empty one, so
        // every unit read here, up to and including that last NUL, is inside the block.
        unsafe {
            while *at.add(length) != 0 {
                length += 1;
            }
        }
        if length == 0 {
            break;
        }
        // SAFETY: the `length` units just read.
        let entry = unsafe { std::slice::from_raw_parts(at, length) };
        entries.push(OsString::from_wide(entry));
        // SAFETY: past this entry's NUL, which is still inside the block.
        at = unsafe { at.add(length + 1) };
    }
    Ok(entries)
}

/// A Windows error as agentspace reports it: Windows' text and the HRESULT.
fn text(e: &windows::core::Error) -> String {
    let message = e.message();
    let message = message.trim_end().trim_end_matches('.');
    if message.is_empty() {
        format!("HRESULT 0x{:08X}.", e.code().0 as u32)
    } else {
        format!("{message} (HRESULT 0x{:08X}).", e.code().0 as u32)
    }
}
