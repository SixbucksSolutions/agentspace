//! The only module of this crate that calls Windows (docs/state.md, "Replacing a file"). On any
//! other system its stand-ins use std alone, and are NOT the contract's: they exist so the crate
//! builds and its logic can be exercised there.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#![allow(unsafe_code)]

use std::fs::Metadata;
use std::io;
use std::path::Path;

/// Whether `meta`, got without following a link, describes a symbolic link, another reparse point
/// or a read-only file: what a config MUST NOT be when the `Include` line has to be added.
#[cfg(windows)]
pub(crate) fn is_link_or_read_only(meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & (FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
}

/// Not the contract's: a symbolic link or a read-only permission.
#[cfg(not(windows))]
pub(crate) fn is_link_or_read_only(meta: &Metadata) -> bool {
    meta.file_type().is_symlink() || meta.permissions().readonly()
}

/// Replaces `replaced` with `replacement` so that `replaced` keeps its own access rules
/// (`ReplaceFileW`, no backup file, no flags); the owner becomes the running user's. One attempt. On failure `replacement` is left where
/// it is, and so is `replaced` unless the call lost it part way.
#[cfg(windows)]
pub(crate) fn replace_keeping_rules(replaced: &Path, replacement: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    let replaced = wide(replaced);
    let replacement = wide(replacement);
    // SAFETY: both strings are NUL-terminated and outlive the call, the backup name and the two
    // reserved pointers are null as the API allows, and the flags are 0.
    let ok = unsafe {
        ReplaceFileW(
            replaced.as_ptr(),
            replacement.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Not the contract's: the permissions are copied from `replaced` and the file is renamed over it.
#[cfg(not(windows))]
pub(crate) fn replace_keeping_rules(replaced: &Path, replacement: &Path) -> io::Result<()> {
    let perms = std::fs::metadata(replaced)?.permissions();
    std::fs::set_permissions(replacement, perms)?;
    std::fs::rename(replacement, replaced)
}

/// Fills `buf` from the operating system's random source.
#[cfg(windows)]
pub(crate) fn random_bytes(buf: &mut [u8]) -> io::Result<()> {
    use windows_sys::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };
    let len = u32::try_from(buf.len()).map_err(|_| io::Error::other("buffer too long"))?;
    // SAFETY: `buf` is valid for `len` bytes of writes, and the algorithm handle is null as
    // BCRYPT_USE_SYSTEM_PREFERRED_RNG requires.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            len,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        Err(io::Error::other("random source failed"))
    } else {
        Ok(())
    }
}

/// Not the contract's: `/dev/urandom`.
#[cfg(not(windows))]
pub(crate) fn random_bytes(buf: &mut [u8]) -> io::Result<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")?.read_exact(buf)
}
