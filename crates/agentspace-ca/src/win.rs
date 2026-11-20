//! The Windows half of `ca`: the user's SID, accounts' names, the elevated test, the access rules
//! read and set through the Windows API, and the local date (docs/operator.md, ca; gap FB-15).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **This is the one module of the crate that calls Windows.** Every `unsafe` block in the crate
//!   MUST be in this file.
//! - **Access rules MUST be set and read through the Windows API, never by starting `icacls`**
//!   (D42, FB-15).
//! - **The user is the user SID of the process's token**, never a group such as `Administrators`.
//! - **Only `lock_folder` and `lock_key` change rules**, and `ca` calls them only on the folder and
//!   the key it is creating.
//! - **A function here MUST NOT write to stdout or stderr, and MUST NOT panic on bad input.**

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, GetLastError, HANDLE, LocalFree, SYSTEMTIME,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSidToSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, ACL_REVISION, ACL_SIZE_INFORMATION, AclSizeInformation, AddAccessAllowedAceEx,
    CheckTokenMembership, CreateWellKnownSid, DACL_SECURITY_INFORMATION, GetAce, GetAclInformation,
    GetLengthSid, GetSecurityDescriptorControl, GetTokenInformation, InitializeAcl,
    LookupAccountSidW, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SID_NAME_USE, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER,
    TokenElevation, TokenUser, WinBuiltinAdministratorsSid,
};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::Host;
use crate::access::{Ace, AceKind, FULL_CONTROL, Rules};

/// `SE_DACL_PROTECTED` in a security descriptor's control word.
const SE_DACL_PROTECTED: u16 = 0x1000;
/// `OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE`: `(OI)(CI)` in `icacls` terms.
const OBJECT_AND_CONTAINER_INHERIT: u32 = 0x1 | 0x2;
/// The largest SID, in bytes (`SECURITY_MAX_SID_SIZE`).
const MAX_SID_SIZE: usize = 68;

/// `ACCESS_ALLOWED_ACE_TYPE`, the plain allow entry.
const ACCESS_ALLOWED: u8 = 0;
/// The allow types: `ACCESS_ALLOWED`, `ACCESS_ALLOWED_OBJECT`, `ACCESS_ALLOWED_CALLBACK` and
/// `ACCESS_ALLOWED_CALLBACK_OBJECT`. Each names its SID where a plain entry does, or after an
/// object entry's flags and GUIDs.
const ALLOW_TYPES: [u8; 4] = [ACCESS_ALLOWED, 5, 9, 11];
/// The deny types: `ACCESS_DENIED`, `ACCESS_DENIED_OBJECT`, `ACCESS_DENIED_CALLBACK` and
/// `ACCESS_DENIED_CALLBACK_OBJECT`.
const DENY_TYPES: [u8; 4] = [1, 6, 10, 12];
/// The object types, whose SID follows a flags word and up to two GUIDs.
const OBJECT_TYPES: [u8; 4] = [5, 6, 11, 12];

/// A block that Windows allocated with `LocalAlloc`, freed when this drops.
struct LocalMem(*mut c_void);

impl Drop for LocalMem {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the pointer came from an API documented to return LocalAlloc memory, and it
            // is freed once, here.
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

/// A token handle, closed when this drops.
struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by OpenProcessToken and is closed once, here.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// The system's own text for a Windows error code, trimmed, without Rust's ` (os error <n>)`
/// (operator.md, Exit codes: architect, 2026-11-19).
fn error_text(code: u32) -> String {
    crate::io_text(&std::io::Error::from_raw_os_error(code as i32))
}

/// The system's text for the calling thread's last error.
fn last_error_text() -> String {
    // SAFETY: GetLastError only reads the thread's last-error value.
    error_text(unsafe { GetLastError() })
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn wide_str(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A NUL-ended UTF-16 buffer's text, up to its first NUL.
fn from_wide(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// A SID in its string form, `S-1-5-...`.
fn sid_to_string(sid: PSID) -> Option<String> {
    if sid.is_null() {
        return None;
    }
    let mut text: *mut u16 = null_mut();
    // SAFETY: `sid` is a valid SID that the caller got from Windows, and `text` receives a
    // LocalAlloc'd, NUL-ended string, which the guard frees.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 || text.is_null() {
        return None;
    }
    let _guard = LocalMem(text.cast());
    // SAFETY: `text` is NUL-ended, so this reads up to and not past its terminator.
    let length = unsafe {
        let mut n = 0usize;
        while *text.add(n) != 0 {
            n += 1;
        }
        n
    };
    // SAFETY: `length` units were just read from the same buffer.
    let units = unsafe { std::slice::from_raw_parts(text, length) };
    String::from_utf16(units).ok()
}

/// A SID from its string form, in a LocalAlloc'd block that the returned guard frees.
fn string_to_sid(text: &str) -> Result<LocalMem, String> {
    let wide = wide_str(text);
    let mut sid: PSID = null_mut();
    // SAFETY: `wide` is NUL-ended, and `sid` receives a LocalAlloc'd SID.
    if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut sid) } == 0 || sid.is_null() {
        return Err(last_error_text());
    }
    Ok(LocalMem(sid))
}

fn open_token() -> Result<Token, String> {
    let mut token: HANDLE = null_mut();
    // SAFETY: the current process's pseudo handle is always valid, and `token` receives a handle
    // that the guard closes.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(last_error_text());
    }
    Ok(Token(token))
}

/// An account's name and domain, by SID.
fn lookup(sid: PSID) -> Option<(String, String)> {
    let mut name_len: u32 = 0;
    let mut domain_len: u32 = 0;
    let mut kind: SID_NAME_USE = 0;
    // SAFETY: null buffers of length 0 ask only for the sizes, as the API documents.
    unsafe {
        LookupAccountSidW(
            null(),
            sid,
            null_mut(),
            &mut name_len,
            null_mut(),
            &mut domain_len,
            &mut kind,
        );
    }
    if name_len == 0 {
        return None;
    }
    let mut name = vec![0u16; name_len as usize + 1];
    let mut domain = vec![0u16; domain_len as usize + 1];
    let mut name_len = name.len() as u32;
    let mut domain_len = domain.len() as u32;
    // SAFETY: each buffer holds the number of units its length says.
    let ok = unsafe {
        LookupAccountSidW(
            null(),
            sid,
            name.as_mut_ptr(),
            &mut name_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut kind,
        )
    };
    if ok == 0 {
        return None;
    }
    Some((from_wide(&name), from_wide(&domain)))
}

/// The SID that an entry names, at `offset` bytes into it, when the entry is long enough to hold
/// one.
///
/// # Safety
///
/// `ace` MUST point to a whole entry of `size` bytes, as `GetAce` returns.
unsafe fn sid_in(ace: *const u8, size: usize, offset: usize) -> Option<String> {
    // A SID is at least 8 bytes: revision, count, a 6-byte authority.
    if offset + 8 > size {
        return None;
    }
    // SAFETY: `offset + 8` is within the entry, so the SID's header can be read; its count says
    // how many 4-byte parts follow, and the whole is checked against the entry's size before the
    // SID is handed to Windows.
    unsafe {
        let count = *ace.add(offset + 1) as usize;
        if offset + 8 + 4 * count > size {
            return None;
        }
        sid_to_string(ace.add(offset).cast_mut().cast())
    }
}

/// One entry of a list, raw: its type, flags, bytes and pointer.
struct RawAce {
    kind: u8,
    flags: u8,
    size: usize,
    ptr: *const u8,
}

/// The entries of a list, in order. `acl` MUST be a valid, non-null list that outlives the result.
fn raw_aces(acl: *mut ACL) -> Result<Vec<RawAce>, String> {
    let mut info = ACL_SIZE_INFORMATION {
        AceCount: 0,
        AclBytesInUse: 0,
        AclBytesFree: 0,
    };
    // SAFETY: `acl` is a valid list, and `info` is the structure this class fills.
    let ok = unsafe {
        GetAclInformation(
            acl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    };
    if ok == 0 {
        return Err(last_error_text());
    }
    let mut aces = Vec::new();
    for index in 0..info.AceCount {
        let mut ace: *mut c_void = null_mut();
        // SAFETY: `index` is below the list's count.
        if unsafe { GetAce(acl, index, &mut ace) } == 0 || ace.is_null() {
            return Err(last_error_text());
        }
        let ptr = ace.cast::<u8>().cast_const();
        // SAFETY: every entry starts with a 4-byte header: type, flags, and its size in bytes.
        let (kind, flags, size) = unsafe {
            (
                *ptr,
                *ptr.add(1),
                std::ptr::read_unaligned(ptr.add(2).cast::<u16>()) as usize,
            )
        };
        aces.push(RawAce {
            kind,
            flags,
            size,
            ptr,
        });
    }
    Ok(aces)
}

/// The SID an entry names, whatever its type, or `None` for a type this module does not read.
fn raw_sid(ace: &RawAce) -> Option<String> {
    if ace.size < 8 {
        return None;
    }
    let offset = if OBJECT_TYPES.contains(&ace.kind) {
        if ace.size < 12 {
            return None;
        }
        // SAFETY: the entry is at least 12 bytes, so its flags word at 8 can be read.
        let object_flags = unsafe { std::ptr::read_unaligned(ace.ptr.add(8).cast::<u32>()) };
        12 + 16 * (object_flags & 1) as usize + 16 * ((object_flags >> 1) & 1) as usize
    } else if ALLOW_TYPES.contains(&ace.kind) || DENY_TYPES.contains(&ace.kind) {
        8
    } else {
        return None;
    };
    // SAFETY: `ace.ptr` points to a whole entry of `ace.size` bytes, from `GetAce`.
    unsafe { sid_in(ace.ptr, ace.size, offset) }
}

/// The real Windows: the process's token and the files' own access rules.
pub(crate) struct Windows;

impl Host for Windows {
    fn user_sid(&self) -> Result<String, String> {
        let token = open_token()?;
        let mut needed: u32 = 0;
        // SAFETY: a null buffer of length 0 asks only for the size, as the API documents.
        unsafe {
            GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed);
        }
        if needed == 0 {
            return Err(last_error_text());
        }
        // A u64 buffer keeps the TOKEN_USER structure aligned.
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        // SAFETY: the buffer is at least `needed` bytes and 8-byte aligned.
        let ok = unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        };
        if ok == 0 {
            return Err(last_error_text());
        }
        // SAFETY: on success the buffer holds a TOKEN_USER, whose SID points into the buffer.
        let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        sid_to_string(sid).ok_or_else(last_error_text)
    }

    fn account_name(&self, sid: &str) -> Option<String> {
        let sid = string_to_sid(sid).ok()?;
        let (name, domain) = lookup(sid.0)?;
        if domain.is_empty() {
            Some(name)
        } else {
            Some(format!("{domain}\\{name}"))
        }
    }

    fn user_name(&self) -> Option<String> {
        let sid = self.user_sid().ok()?;
        let sid = string_to_sid(&sid).ok()?;
        lookup(sid.0).map(|(name, _)| name)
    }

    fn elevated(&self) -> Result<bool, String> {
        let token = open_token()?;
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned: u32 = 0;
        // SAFETY: `elevation` is the structure this class fills, and its size is given.
        let ok = unsafe {
            GetTokenInformation(
                token.0,
                TokenElevation,
                (&mut elevation as *mut TOKEN_ELEVATION).cast(),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut returned,
            )
        };
        if ok == 0 {
            return Err(last_error_text());
        }
        if elevation.TokenIsElevated == 0 {
            return Ok(false);
        }
        let mut sid = [0u64; MAX_SID_SIZE.div_ceil(8)];
        let mut size = MAX_SID_SIZE as u32;
        // SAFETY: the buffer holds `size` bytes, enough for any SID.
        let ok = unsafe {
            CreateWellKnownSid(
                WinBuiltinAdministratorsSid,
                null_mut(),
                sid.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if ok == 0 {
            return Err(last_error_text());
        }
        let mut member = 0;
        // SAFETY: a null token checks the calling thread's own; the SID was just made.
        if unsafe { CheckTokenMembership(null_mut(), sid.as_mut_ptr().cast(), &mut member) } == 0 {
            return Err(last_error_text());
        }
        Ok(member != 0)
    }

    fn rules(&self, path: &Path) -> Result<Rules, String> {
        let name = wide_path(path);
        let mut owner: PSID = null_mut();
        let mut dacl: *mut ACL = null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: `name` is NUL-ended, and each out pointer is a valid local.
        let status = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut descriptor,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(error_text(status));
        }
        let _guard = LocalMem(descriptor);

        let owner = sid_to_string(owner).ok_or_else(last_error_text)?;
        let mut control: u16 = 0;
        let mut revision: u32 = 0;
        // SAFETY: `descriptor` is the valid descriptor just returned.
        if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0 {
            return Err(last_error_text());
        }
        let protected = control & SE_DACL_PROTECTED != 0;
        if dacl.is_null() {
            return Ok(Rules {
                owner,
                protected,
                aces: None,
            });
        }
        let aces = raw_aces(dacl)?
            .iter()
            .map(|ace| {
                // Only a plain allow entry can be the one entry of an owner-only list, as in the
                // lease CLI's check; an allow with a condition or for an object is another rule.
                let kind = if ace.kind == ACCESS_ALLOWED {
                    AceKind::Allow
                } else if DENY_TYPES.contains(&ace.kind) {
                    AceKind::Deny
                } else {
                    AceKind::Other
                };
                let mask = if ace.size >= 8 {
                    // SAFETY: the entry is at least 8 bytes, so its mask at 4 can be read.
                    unsafe { std::ptr::read_unaligned(ace.ptr.add(4).cast::<u32>()) }
                } else {
                    0
                };
                Ace {
                    kind,
                    // `None` for a type whose SID this module does not know how to find.
                    sid: raw_sid(ace),
                    mask,
                    flags: ace.flags,
                }
            })
            .collect();
        Ok(Rules {
            owner,
            protected,
            aces: Some(aces),
        })
    }

    /// Locks the folder to the user alone, through the API: its whole list is replaced by a
    /// protected one holding a single entry, the user's full control, inherited by the files and
    /// folders in it (`(OI)(CI)`). Every other entry, inherited or explicit, is gone. The owner is
    /// not changed, so a folder someone else owns still fails the check that follows.
    // Locking replaces the folder's whole access list with one protected rule giving the user
    // full control; "Remove the other rules" then fires only for a folder someone else owns, or a
    // lock that did not take (operator.md, ca, Rulings for the rewrite's ca: architect,
    // 2026-11-19, for E0.06-T02-S01).
    fn lock_folder(&self, path: &Path, user_sid: &str) -> Result<(), String> {
        lock_alone(path, user_sid, OBJECT_AND_CONTAINER_INHERIT)
    }

    /// What `icacls <key> /reset` then `icacls <key> /inheritance:d` leave on a key made in a
    /// locked folder, through the API: a protected list of one explicit entry, the user's full
    /// control. The owner is not changed.
    fn lock_key(&self, path: &Path, user_sid: &str) -> Result<(), String> {
        lock_alone(path, user_sid, 0)
    }

    fn local_date(&self) -> String {
        let mut now = SYSTEMTIME {
            wYear: 0,
            wMonth: 0,
            wDayOfWeek: 0,
            wDay: 0,
            wHour: 0,
            wMinute: 0,
            wSecond: 0,
            wMilliseconds: 0,
        };
        // SAFETY: `now` is the structure the call fills.
        unsafe { GetLocalTime(&mut now) };
        format!("{:04}-{:02}-{:02}", now.wYear, now.wMonth, now.wDay)
    }
}

/// Sets `path`'s list to a protected one of a single entry: `user_sid`'s full control, with the
/// entry flags `flags`.
fn lock_alone(path: &Path, user_sid: &str, flags: u32) -> Result<(), String> {
    let user = string_to_sid(user_sid)?;
    // SAFETY: `user` holds a valid SID from ConvertStringSidToSidW.
    let sid_len = unsafe { GetLengthSid(user.0) } as usize;
    // The list's header is 8 bytes; an allow entry is a 4-byte header, a 4-byte mask and the SID.
    let bytes = 8 + 8 + sid_len;
    // A u32 buffer keeps the list 4-byte aligned, as InitializeAcl needs.
    let mut words = vec![0u32; bytes.div_ceil(4)];
    let acl: *mut ACL = words.as_mut_ptr().cast();
    let size = (words.len() * 4) as u32;
    // SAFETY: the buffer is `size` bytes long and aligned.
    if unsafe { InitializeAcl(acl, size, ACL_REVISION) } == 0 {
        return Err(last_error_text());
    }
    // SAFETY: the list was initialized and sized for this one entry, and `user` is a valid SID.
    if unsafe { AddAccessAllowedAceEx(acl, ACL_REVISION, flags, FULL_CONTROL, user.0) } == 0 {
        return Err(last_error_text());
    }
    set_protected_dacl(&wide_path(path), acl)
}

/// Sets `name`'s list to `acl`, protected from inheritance.
fn set_protected_dacl(name: &[u16], acl: *mut ACL) -> Result<(), String> {
    // SAFETY: `name` is NUL-ended, and `acl` is a valid, initialized list. The call does not keep
    // either pointer.
    let status = unsafe {
        SetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            acl,
            null(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(error_text(status));
    }
    Ok(())
}

/// Helpers that only the tests use, to loosen a throwaway folder or key, and to lift what they
/// set so that cleanup always works. The program never sets rules but through `lock_folder` and
/// `lock_key`.
#[cfg(test)]
pub(crate) mod testing {
    use std::path::Path;
    use std::ptr::null_mut;

    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        ACL, GetSecurityDescriptorControl, GetSecurityDescriptorDacl, PSECURITY_DESCRIPTOR,
    };

    use super::{LocalMem, SE_DACL_PROTECTED, set_protected_dacl, wide_path, wide_str};
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::Security::Authorization::{SE_FILE_OBJECT, SetNamedSecurityInfoW};
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, UNPROTECTED_DACL_SECURITY_INFORMATION,
    };

    /// Sets a file's or a folder's list from an SDDL string such as `D:P(A;;FA;;;<sid>)`. A list
    /// whose SDDL is protected (`D:P`) is set protected; otherwise it is set unprotected, and so
    /// takes the parent's inheritable entries as well.
    pub(crate) fn set_sddl(path: &Path, sddl: &str) -> bool {
        let text = wide_str(sddl);
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: `text` is NUL-ended, and `descriptor` receives a LocalAlloc'd descriptor.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        };
        if ok == 0 {
            return false;
        }
        let _guard = LocalMem(descriptor);
        let (mut present, mut defaulted) = (0, 0);
        let mut dacl: *mut ACL = null_mut();
        // SAFETY: `descriptor` is the valid descriptor just made.
        let ok = unsafe {
            GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)
        };
        if ok == 0 || present == 0 || dacl.is_null() {
            return false;
        }
        let (mut control, mut revision) = (0u16, 0u32);
        // SAFETY: as above.
        if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0 {
            return false;
        }
        let name = wide_path(path);
        if control & SE_DACL_PROTECTED != 0 {
            return set_protected_dacl(&name, dacl).is_ok();
        }
        // SAFETY: `name` is NUL-ended, and `dacl` is valid while the guard lives.
        let status = unsafe {
            SetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | UNPROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                dacl,
                std::ptr::null(),
            )
        };
        status == ERROR_SUCCESS
    }
}
