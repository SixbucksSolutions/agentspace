//! The one Windows call the lease CLI makes itself: whether an open file is a file on a disk, and
//! so never a device such as `CON` or `NUL`, or a pipe (lease-api.md, Plan; architect, 2026-11-24,
//! for v0.10-T01, pin P7). The only module of this crate with `unsafe`.

use std::fs::File;

/// Whether `file` is a file on a disk: `GetFileType` says `FILE_TYPE_DISK`.
#[cfg(windows)]
pub fn is_disk_file(file: &File) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{FILE_TYPE_DISK, GetFileType};
    // SAFETY: the handle is the open file's own, and stays open for the call, which only reads its
    // type.
    unsafe { GetFileType(file.as_raw_handle()) == FILE_TYPE_DISK }
}

/// Whether `file` is a file on a disk. Elsewhere than Windows, the file's metadata, which its
/// caller checks, says so.
#[cfg(not(windows))]
pub fn is_disk_file(_file: &File) -> bool {
    true
}
