//! The Windows half: the lease state lock, the read and the atomic replace as state.md asks for
//! them (The lease state lock, Replacing a file). This is the one module of the crate that calls
//! Windows, and so the one that holds `unsafe`.

#![allow(unsafe_code)]

use crate::sys::{OpenFailure, ReplaceFailure};
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_BAD_PATHNAME, ERROR_INVALID_PARAMETER, ERROR_LOCK_VIOLATION,
    ERROR_NOT_SUPPORTED, ERROR_SHARING_VIOLATION, GENERIC_WRITE,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FileRenameInfoEx, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    SetFileInformationByHandle,
};

/// `FILE_RENAME_FLAG_REPLACE_IF_EXISTS`, which windows-sys does not define.
const RENAME_REPLACE_IF_EXISTS: u32 = 0x1;
/// `FILE_RENAME_FLAG_POSIX_SEMANTICS`, which windows-sys does not define.
const RENAME_POSIX_SEMANTICS: u32 = 0x2;

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

/// Makes a new temporary file, failing if it exists, opened for writing with DELETE access, so that
/// its own handle can rename it (`replace`), and sharing read, write and delete.
pub fn create_temp_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .access_mode(GENERIC_WRITE | DELETE)
        .open(path)
}

/// What a Windows error code from a rename says: another process holds a file without sharing
/// delete (access denied, a sharing violation or a lock violation) and the caller tries again, or
/// anything else, which fails at once.
fn classify(code: Option<u32>) -> ReplaceFailure {
    match code {
        Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION) => {
            ReplaceFailure::Held
        }
        _ => ReplaceFailure::Other,
    }
}

/// Renames the file `temp` is open on to `to`, replacing `to`, with POSIX semantics
/// (`SetFileInformationByHandle` with `FileRenameInfoEx`). `Err` is the Windows error code.
fn posix_rename(temp: &File, to: &Path) -> Result<(), u32> {
    let full = std::path::absolute(to).map_err(|_| ERROR_BAD_PATHNAME)?;
    let name: Vec<u16> = full.as_os_str().encode_wide().collect();
    if name.contains(&0) {
        return Err(ERROR_BAD_PATHNAME);
    }
    let name_bytes = name.len() * 2;
    let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    // The variable-length FileName follows the fixed part, with room for its NUL. The buffer is made
    // of u64 so that it is aligned for the struct, which holds a pointer-sized handle.
    let total = offset + name_bytes + 2;
    let Ok(total_u32) = u32::try_from(total) else {
        return Err(ERROR_BAD_PATHNAME);
    };
    let mut buf = vec![0u64; total.div_ceil(8)];
    let base = buf.as_mut_ptr().cast::<u8>();
    // SAFETY: `buf` is zeroed, 8-byte aligned and at least `total` bytes, which holds the fixed part
    // of FILE_RENAME_INFO at `offset` bytes and then the name's UTF-16 units and a NUL. `temp`'s
    // handle is open for the whole call. The name's length in bytes excludes the NUL, as the call
    // asks.
    let done = unsafe {
        let info = base.cast::<FILE_RENAME_INFO>();
        (*info).Anonymous.Flags = RENAME_REPLACE_IF_EXISTS | RENAME_POSIX_SEMANTICS;
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = name_bytes as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), base.add(offset).cast::<u16>(), name.len());
        SetFileInformationByHandle(
            temp.as_raw_handle(),
            FileRenameInfoEx,
            base.cast::<c_void>(),
            total_u32,
        )
    };
    if done != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error()
            .raw_os_error()
            .map_or(0, |c| c as u32))
    }
}

/// The fallback: `MoveFileExW`, which classic Windows rename semantics make fail while any process
/// has the target open.
fn move_file_ex(from: &Path, to: &Path) -> Result<(), ReplaceFailure> {
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
    Err(classify(
        io::Error::last_os_error().raw_os_error().map(|c| c as u32),
    ))
}

/// Moves the temporary file `from`, open as `temp`, over `to` in one rename. `temp` MUST already be
/// written and flushed (`FlushFileBuffers`, which `File::sync_all` calls).
///
/// The rename is `SetFileInformationByHandle` with `FileRenameInfoEx` and
/// `FILE_RENAME_FLAG_REPLACE_IF_EXISTS | FILE_RENAME_FLAG_POSIX_SEMANTICS`, which replaces a file
/// that a reader sharing read, write and delete holds open (architect, 2026-11-19; state.md,
/// Replacing a file). Only where the file system refuses POSIX semantics (`ERROR_INVALID_PARAMETER`
/// or `ERROR_NOT_SUPPORTED`) is it `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` and
/// `MOVEFILE_WRITE_THROUGH`. A failure that says another process holds a file without sharing
/// delete is `Held`, and the caller tries again.
pub fn replace(temp: &File, from: &Path, to: &Path) -> Result<(), ReplaceFailure> {
    match posix_rename(temp, to) {
        Ok(()) => Ok(()),
        Err(ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED) => move_file_ex(from, to),
        Err(code) => Err(classify(Some(code))),
    }
}
