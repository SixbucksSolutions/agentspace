//! The Windows access rules (docs/state.md, "Access rules" and "The lease folder").
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **This is the one module of the crate that calls Windows.** Every `unsafe` block in the crate
//!   MUST be in this file.
//! - **The user is the user SID of the process's token**, never a group such as `Administrators`.
//! - **A function here MUST NOT change a file's or a folder's rules except `create_private_file`,
//!   on the file it has just made.** The CA's checks only read.
//! - **A function here MUST NOT write to stdout or stderr, and MUST NOT panic on bad input.**

use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
    ConvertStringSidToSidW, GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, ACL_SIZE_INFORMATION, AclSizeInformation, DACL_SECURITY_INFORMATION, GetAce,
    GetAclInformation, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    GetTokenInformation, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// `SE_DACL_PROTECTED` in a security descriptor's control word.
const SE_DACL_PROTECTED: u16 = 0x1000;
/// `ACCESS_ALLOWED_ACE_TYPE`.
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
/// Every right of full control for a file or a folder (`FILE_ALL_ACCESS`).
const FULL_CONTROL: u32 = 0x1F_01FF;
/// What reading a file needs (`FILE_GENERIC_READ`).
const GENERIC_READ_RIGHTS: u32 = 0x12_0089;
/// What writing a file needs (`FILE_GENERIC_WRITE`).
const GENERIC_WRITE_RIGHTS: u32 = 0x12_0116;
/// The well-known SID of `SYSTEM`.
const SYSTEM_SID: &str = "S-1-5-18";
/// The well-known SID of `Administrators`.
const ADMINISTRATORS_SID: &str = "S-1-5-32-544";

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

/// A path or a string as a NUL-ended UTF-16 buffer.
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn wide_str(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A SID in its string form, `S-1-5-...`.
fn sid_to_string(sid: PSID) -> Option<String> {
    if sid.is_null() {
        return None;
    }
    let mut text: *mut u16 = null_mut();
    // SAFETY: `sid` is a valid SID that the caller got from Windows, and `text` receives a
    // LocalAlloc'd, NUL-ended string, which the guard frees.
    let ok = unsafe { ConvertSidToStringSidW(sid, &mut text) };
    if ok == 0 || text.is_null() {
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

/// The user SID of the process's token, as a string.
pub fn user_sid() -> Option<String> {
    let mut token: HANDLE = null_mut();
    // SAFETY: the current process's pseudo handle is always valid, and `token` receives a handle
    // that is closed below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let mut needed: u32 = 0;
    // SAFETY: a null buffer of length 0 asks only for the size, as the API documents.
    unsafe {
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed);
    }
    let mut result = None;
    if needed > 0 {
        // A u64 buffer keeps the TOKEN_USER structure aligned.
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        // SAFETY: the buffer is at least `needed` bytes and 8-byte aligned.
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        };
        if ok != 0 {
            // SAFETY: on success the buffer holds a TOKEN_USER, whose SID points into the buffer.
            let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
            result = sid_to_string(sid);
        }
    }
    // SAFETY: `token` was opened above and is closed once.
    unsafe {
        CloseHandle(token);
    }
    result
}

/// One access control entry of a DACL.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ace {
    /// Whether this is a plain allow entry. Every other kind of entry is `false`, with no SID.
    allow: bool,
    sid: String,
    mask: u32,
}

/// What a file's or a folder's owner and access control list are.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rules {
    owner: String,
    protected: bool,
    /// `None` for a NULL DACL, which grants everyone everything.
    aces: Option<Vec<Ace>>,
}

/// Reads a file's or a folder's owner and DACL, by SID.
fn read_rules(path: &Path) -> Option<Rules> {
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
        return None;
    }
    let _guard = LocalMem(descriptor);

    let owner = sid_to_string(owner)?;
    let mut control: u16 = 0;
    let mut revision: u32 = 0;
    // SAFETY: `descriptor` is the valid descriptor just returned.
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0 {
        return None;
    }
    let protected = control & SE_DACL_PROTECTED != 0;

    if dacl.is_null() {
        return Some(Rules {
            owner,
            protected,
            aces: None,
        });
    }
    let mut size = ACL_SIZE_INFORMATION {
        AceCount: 0,
        AclBytesInUse: 0,
        AclBytesFree: 0,
    };
    // SAFETY: `dacl` is a valid ACL, and `size` is the structure this class fills.
    let ok = unsafe {
        GetAclInformation(
            dacl,
            (&mut size as *mut ACL_SIZE_INFORMATION).cast(),
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    };
    if ok == 0 {
        return None;
    }
    let mut aces = Vec::new();
    for index in 0..size.AceCount {
        let mut ace: *mut c_void = null_mut();
        // SAFETY: `index` is below the ACL's count.
        if unsafe { GetAce(dacl, index, &mut ace) } == 0 || ace.is_null() {
            return None;
        }
        // SAFETY: every ACE starts with a header of one type byte, one flag byte and a size.
        let kind = unsafe { *ace.cast::<u8>() };
        if kind != ACCESS_ALLOWED_ACE_TYPE {
            aces.push(Ace {
                allow: false,
                sid: String::new(),
                mask: 0,
            });
            continue;
        }
        // SAFETY: an access-allowed ACE is a header, a 4-byte mask, then the SID, which starts
        // 8 bytes in. The ACE is not read past its own length, which Windows checked.
        let (mask, sid) = unsafe {
            let mask = std::ptr::read_unaligned(ace.cast::<u8>().add(4).cast::<u32>());
            let sid: PSID = ace.cast::<u8>().add(8).cast();
            (mask, sid_to_string(sid))
        };
        aces.push(Ace {
            allow: true,
            sid: sid?,
            mask,
        });
    }
    Some(Rules {
        owner,
        protected,
        aces: Some(aces),
    })
}

/// Whether a file or a folder is owner-only (state.md, "Access rules"): owned by the user, with a
/// protected DACL that holds exactly one entry, an allow for the user with every right of full
/// control. It only reads.
pub fn owner_only(path: &Path) -> bool {
    let (Some(user), Some(rules)) = (user_sid(), read_rules(path)) else {
        return false;
    };
    if rules.owner != user || !rules.protected {
        return false;
    }
    match rules.aces.as_deref() {
        Some([ace]) => ace.allow && ace.sid == user && ace.mask & FULL_CONTROL == FULL_CONTROL,
        _ => false,
    }
}

/// Whether a private key file has exactly the rules of state.md, "The lease folder": owned by the
/// user, a protected DACL, and only allow entries for `SYSTEM` and `Administrators` with full
/// control and for the user with at least read and write. It only reads.
pub fn verify_private_key(path: &Path) -> bool {
    let (Some(user), Some(rules)) = (user_sid(), read_rules(path)) else {
        return false;
    };
    if rules.owner != user || !rules.protected {
        return false;
    }
    let Some(aces) = rules.aces else {
        return false;
    };
    let (mut system, mut administrators, mut mine) = (0u32, 0u32, 0u32);
    for ace in &aces {
        if !ace.allow {
            return false;
        }
        if ace.sid == SYSTEM_SID {
            system |= ace.mask;
        } else if ace.sid == ADMINISTRATORS_SID {
            administrators |= ace.mask;
        } else if ace.sid == user {
            mine |= ace.mask;
        } else {
            return false;
        }
    }
    let wanted = GENERIC_READ_RIGHTS | GENERIC_WRITE_RIGHTS;
    system & FULL_CONTROL == FULL_CONTROL
        && administrators & FULL_CONTROL == FULL_CONTROL
        && mine & wanted == wanted
}

/// Sets a file's or a folder's DACL from SDDL entries, protected from inheritance, and its owner
/// to `owner` when one is given.
fn set_rules(path: &Path, owner: Option<&str>, sddl: &str) -> bool {
    let sddl = wide_str(sddl);
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: `sddl` is NUL-ended, and `descriptor` receives a LocalAlloc'd descriptor.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    };
    if ok == 0 {
        return false;
    }
    let _descriptor = LocalMem(descriptor);

    let mut present: i32 = 0;
    let mut defaulted: i32 = 0;
    let mut dacl: *mut ACL = null_mut();
    // SAFETY: `descriptor` is the valid descriptor just made.
    let ok =
        unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) };
    if ok == 0 || present == 0 || dacl.is_null() {
        return false;
    }

    let mut owner_sid: PSID = null_mut();
    let mut _owner_guard = LocalMem(null_mut());
    let mut info = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
    if let Some(owner) = owner {
        let text = wide_str(owner);
        // SAFETY: `text` is NUL-ended, and `owner_sid` receives a LocalAlloc'd SID.
        if unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut owner_sid) } == 0 {
            return false;
        }
        _owner_guard = LocalMem(owner_sid);
        info |= OWNER_SECURITY_INFORMATION;
    }

    let mut name = wide_path(path);
    // SAFETY: `name` is NUL-ended; the owner is null or valid, and the DACL is valid.
    let status = unsafe {
        SetNamedSecurityInfoW(
            name.as_mut_ptr(),
            SE_FILE_OBJECT,
            info,
            owner_sid,
            null_mut(),
            dacl,
            null(),
        )
    };
    status == ERROR_SUCCESS
}

/// Why `create_private_file` failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateError {
    /// No file of this call's making exists at the path.
    NotCreated,
    /// The file was made, and could not be removed again after its rules failed to be set.
    CreatedButFailed,
}

/// Makes a new, empty file at `path` and gives it the lease key's rules before it returns: owner
/// the user, a protected DACL, and three allow entries, `SYSTEM` and `Administrators` with full
/// control and the user with full control. The caller writes the key's bytes through the returned
/// handle, so they never exist in a file with other rules. It never replaces an existing file.
pub fn create_private_file(path: &Path) -> Result<File, CreateError> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| CreateError::NotCreated)?;
    let done = user_sid().is_some_and(|user| {
        let sddl = format!("D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;{user})");
        set_rules(path, Some(&user), &sddl)
    });
    if done {
        return Ok(file);
    }
    drop(file);
    match fs::remove_file(path) {
        Ok(()) => Err(CreateError::NotCreated),
        Err(_) => Err(CreateError::CreatedButFailed),
    }
}

/// Makes `path` owner-only, as the CA's folder (inheritable by what is in it) or the CA's key
/// (a single explicit entry). A test helper: the program never sets the CA's rules (D17).
#[cfg(test)]
pub fn make_owner_only(path: &Path, folder: bool) -> bool {
    let Some(user) = user_sid() else {
        return false;
    };
    let flags = if folder { "OICI" } else { "" };
    set_rules(path, Some(&user), &format!("D:P(A;{flags};FA;;;{user})"))
}

/// Replaces a file's or folder's DACL with an arbitrary SDDL list, protected. A test helper.
#[cfg(test)]
pub fn set_sddl_rules(path: &Path, sddl: &str) -> bool {
    set_rules(path, None, sddl)
}

/// The user SID, for tests that build an SDDL list.
#[cfg(test)]
pub fn user_sid_for_tests() -> Option<String> {
    user_sid()
}
