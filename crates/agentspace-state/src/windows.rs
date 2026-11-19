//! The Windows half: the lease state lock, the read and the atomic replace as state.md asks for
//! them (The lease state lock, Replacing a file). This is the one module of the crate that calls
//! Windows, and so the one that holds `unsafe`.

#![allow(unsafe_code)]

use crate::sys::{OpenFailure, ReplaceFailure};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_LOCK_VIOLATION, ERROR_SHARING_VIOLATION,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING,
    MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

fn is_held(e: &io::Error) -> bool {
    matches!(
        e.raw_os_error().map(|c| c as u32),
        Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
    )
}

/// Opens `leases.lock` for reading and writing with share mode 0, creating it when missing. The
/// share mode is what makes the lock, and what makes the prototype's lock exclude this one. Only a
/// sharing or lock violation says the file is held by another process.
pub fn open_lock(path: &Path) -> Result<File, OpenFailure> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(path)
        .map_err(|e| {
            if is_held(&e) {
                OpenFailure::Held
            } else {
                OpenFailure::Other
            }
        })
}

/// Opens a file for reading sharing read, write and delete, so that a reader never blocks the
/// rename of the file it reads.
pub fn open_read(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(path)
}

/// Moves `from` over `to` in one rename, written through to the disk (MoveFileExW with
/// REPLACE_EXISTING and WRITE_THROUGH). A failure that says another process holds `to` or `from`
/// is `Held`: without sharing delete it is access denied or a sharing violation, and the caller
/// tries again.
pub fn replace(from: &Path, to: &Path) -> Result<(), ReplaceFailure> {
    let wide = |p: &Path| -> Option<Vec<u16>> {
        let mut w: Vec<u16> = p.as_os_str().encode_wide().collect();
        if w.contains(&0) {
            return None;
        }
        w.push(0);
        Some(w)
    };
    let (Some(from_w), Some(to_w)) = (wide(from), wide(to)) else {
        return Err(ReplaceFailure::Other);
    };
    // SAFETY: both buffers are NUL-terminated UTF-16 that outlive the call, and have no NUL inside.
    let moved = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved != 0 {
        return Ok(());
    }
    let e = io::Error::last_os_error();
    // Found on Windows 11 (10.0.26300): MoveFileExW with REPLACE_EXISTING fails with access denied
    // when any handle is open on the target, even one that shares read, write and delete, which
    // state.md says a reader never does to the rename. std's rename replaces with POSIX semantics
    // (FileRenameInfoEx), which a handle sharing delete does not stop, and which a handle without
    // sharing delete still does. It is tried second, so the contract's call is made first.
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    let code = e.raw_os_error().map(|c| c as u32);
    if is_held(&e) || code == Some(ERROR_ACCESS_DENIED) {
        Err(ReplaceFailure::Held)
    } else {
        Err(ReplaceFailure::Other)
    }
}
