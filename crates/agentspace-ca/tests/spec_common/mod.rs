//! Shared helpers for the spec tests of agentspace-ca (E0.06-T02-S01, #0118).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The tests are written from docs/operator.md
//! (ca; sync step 1; Programs it starts), docs/state.md (The CA, Access rules) and the pinned
//! agentspace-operator-api alone. Every home is a fresh throwaway folder under the temp folder, and
//! a test MUST NOT touch the real profile. Every access rule is read and set through the Windows
//! API, never icacls (gap FB-15). The one module here that calls Windows is `win`.
//!
//! **A test MUST NOT print what names this machine or its user** (AGENTS.md, Terry's real files in
//! a review; operator.md, ca, The forms of the placeholders): a message carries `<user>`, every
//! path under the temp folder carries the profile folder's name, `<owner>` and `<who>` carry the
//! computer's name for a local account, and a new CA's public key carries it in its comment. So:
//! - **A message, a line or a path is compared with `same`, `same_for` or `same_lines`**, which
//!   compare the texts as they are and, on a failure, show both only as `masked` leaves them.
//! - **A SID, a set of access rules or a snapshot of the CA is compared with `assert!`**, which
//!   shows no value. `Ace`, `Acl` and `Snapshot` have no `Debug`, so `assert_eq!` cannot print one.
//! - **A `Failure` is never unwrapped with `expect` or `unwrap`**, whose panic prints its message;
//!   `Outcome::ok` and `sync_ok` show it masked.
//!
//! **The library reads no environment variable** (operator.md, ca, Rulings for the rewrite's ca:
//! architect, 2026-11-19, for E0.06-T02-S01), so every test builds its `CaEnv` here, with the
//! made-up user [`USER`] and computer [`COMPUTER`], and never from the environment. A new CA's
//! public key carries the made-up computer name. `masked` still reads this machine's names, only
//! to hide them: `<user>` falls back to the token's own account name when a `CaEnv` has none.
#![cfg(windows)]
#![allow(dead_code)]

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use agentspace_ca::CaEnv;
use agentspace_operator_api::{CallerLine, Context, Failure, Mode, Say};

/// The made-up `USERNAME` every test's `CaEnv` carries.
pub const USER: &str = "someone";
/// The made-up `COMPUTERNAME` every test's `CaEnv` carries.
pub const COMPUTER: &str = "EXAMPLEPC";

/// The operator's environment as the tests make it up: [`USER`], [`COMPUTER`], and no OneDrive
/// folder.
pub fn ca_env() -> CaEnv {
    CaEnv {
        user_name: Some(USER.to_string()),
        computer_name: Some(COMPUTER.to_string()),
        ..CaEnv::default()
    }
}

pub const FULL: u32 = 0x1F01FF;
pub const OBJECT_INHERIT: u8 = 0x1;
pub const CONTAINER_INHERIT: u8 = 0x2;
pub const INHERITED: u8 = 0x10;
pub const SID_USERS: &str = "S-1-5-32-545";
/// BUILTIN\Guests, which an ordinary user is not in.
pub const SID_GUESTS: &str = "S-1-5-32-546";
/// A domain SID that no machine can translate to a name.
pub const SID_UNKNOWN: &str = "S-1-5-21-1111111111-2222222222-3333333333-4444";

pub mod win {
    //! The only module of the tests that calls Windows.
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
        ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW,
        GetNamedSecurityInfoW, SE_FILE_OBJECT, SetNamedSecurityInfoW,
    };
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, GetSecurityDescriptorControl,
        GetSecurityDescriptorDacl, GetTokenInformation, LookupAccountSidW,
        OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SID_NAME_USE,
        TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser,
        UNPROTECTED_DACL_SECURITY_INFORMATION,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    const SE_DACL_PROTECTED: u16 = 0x1000;

    #[derive(Clone, PartialEq, Eq)]
    pub struct Ace {
        pub sid: String,
        pub allow: bool,
        /// The ACE type byte: 0 allow, 1 deny, anything else is something these tests never make.
        pub kind: u8,
        pub rights: u32,
        pub flags: u8,
    }

    #[derive(Clone, PartialEq, Eq)]
    pub struct Acl {
        pub owner: String,
        pub protected: bool,
        pub aces: Vec<Ace>,
    }

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    unsafe fn from_wide(p: *const u16) -> String {
        let mut len = 0;
        // SAFETY: The caller passes a NUL-terminated wide string that Windows returned.
        unsafe {
            while *p.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
        }
    }

    fn sid_to_string(sid: *mut c_void) -> String {
        let mut out: *mut u16 = null_mut();
        // SAFETY: `sid` is a valid SID that stays alive for the call, and the result is freed.
        unsafe {
            assert!(
                ConvertSidToStringSidW(sid, &mut out) != 0,
                "A SID could not be written as text"
            );
            let s = from_wide(out);
            LocalFree(out as *mut c_void);
            s
        }
    }

    fn token_query<T>(class: i32, read: impl FnOnce(*const u8) -> T) -> T {
        // SAFETY: A token handle is opened for query, read from a buffer of the size Windows asked
        // for, and closed.
        unsafe {
            let mut token: HANDLE = null_mut();
            assert!(
                OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) != 0,
                "The process token could not be opened"
            );
            let mut need = 0u32;
            GetTokenInformation(token, class, null_mut(), 0, &mut need);
            let mut buf = vec![0u64; (need as usize).div_ceil(8).max(1)];
            let ok = GetTokenInformation(
                token,
                class,
                buf.as_mut_ptr() as *mut c_void,
                need,
                &mut need,
            );
            assert!(ok != 0, "The token information could not be read");
            let r = read(buf.as_ptr() as *const u8);
            CloseHandle(token);
            r
        }
    }

    /// The user SID of the process's token, as `S-1-...`.
    pub fn user_sid() -> String {
        token_query(TokenUser, |p| {
            // SAFETY: The buffer holds a TOKEN_USER that Windows filled in.
            unsafe { sid_to_string((*(p as *const TOKEN_USER)).User.Sid) }
        })
    }

    /// Whether the process runs with an elevated token.
    pub fn elevated() -> bool {
        token_query(TokenElevation, |p| {
            // SAFETY: The buffer holds a TOKEN_ELEVATION that Windows filled in.
            unsafe { (*(p as *const TOKEN_ELEVATION)).TokenIsElevated != 0 }
        })
    }

    /// An account as Windows names it, `<domain>\<name>`, or `None` when it cannot be translated.
    pub fn account_name(sid_text: &str) -> Option<String> {
        // SAFETY: The SID is made from text, looked up into buffers of known size, and freed.
        unsafe {
            let w = wide(std::ffi::OsStr::new(sid_text));
            let mut sid: *mut c_void = null_mut();
            if ConvertStringSidToSidW(w.as_ptr(), &mut sid) == 0 {
                return None;
            }
            let mut name = vec![0u16; 256];
            let mut domain = vec![0u16; 256];
            let (mut nl, mut dl) = (256u32, 256u32);
            let mut kind: SID_NAME_USE = 0;
            let ok = LookupAccountSidW(
                null(),
                sid,
                name.as_mut_ptr(),
                &mut nl,
                domain.as_mut_ptr(),
                &mut dl,
                &mut kind,
            );
            LocalFree(sid);
            if ok == 0 {
                return None;
            }
            let n = String::from_utf16_lossy(&name[..nl as usize]);
            let d = String::from_utf16_lossy(&domain[..dl as usize]);
            Some(if d.is_empty() { n } else { format!("{d}\\{n}") })
        }
    }

    /// The owner, the protected flag and every entry of a file's or folder's access list.
    pub fn read_acl(path: &Path) -> Acl {
        let w = wide(path.as_os_str());
        // SAFETY: The security descriptor Windows returns is read while alive, then freed.
        unsafe {
            let mut owner: *mut c_void = null_mut();
            let mut dacl: *mut ACL = null_mut();
            let mut sd: *mut c_void = null_mut();
            let rc = GetNamedSecurityInfoW(
                w.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut sd,
            );
            assert_eq!(rc, 0, "The access rules of a path could not be read");
            let mut control = 0u16;
            let mut rev = 0u32;
            assert!(GetSecurityDescriptorControl(sd, &mut control, &mut rev) != 0);
            let mut aces = Vec::new();
            if !dacl.is_null() {
                let mut info = ACL_SIZE_INFORMATION {
                    AceCount: 0,
                    AclBytesInUse: 0,
                    AclBytesFree: 0,
                };
                assert!(
                    GetAclInformation(
                        dacl,
                        &mut info as *mut _ as *mut c_void,
                        size_of::<ACL_SIZE_INFORMATION>() as u32,
                        AclSizeInformation,
                    ) != 0
                );
                for i in 0..info.AceCount {
                    let mut ace: *mut c_void = null_mut();
                    assert!(GetAce(dacl, i, &mut ace) != 0);
                    let header = &*(ace as *const ACE_HEADER);
                    let body = &*(ace as *const ACCESS_ALLOWED_ACE);
                    let sid = &body.SidStart as *const u32 as *mut c_void;
                    aces.push(Ace {
                        sid: sid_to_string(sid),
                        allow: header.AceType == 0,
                        kind: header.AceType,
                        rights: body.Mask,
                        flags: header.AceFlags,
                    });
                }
            }
            let acl = Acl {
                owner: sid_to_string(owner),
                protected: control & SE_DACL_PROTECTED != 0,
                aces,
            };
            LocalFree(sd);
            acl
        }
    }

    /// The path's access list as an SDDL string, `D:...`.
    pub fn read_sddl(path: &Path) -> String {
        let w = wide(path.as_os_str());
        // SAFETY: The descriptor Windows returns is converted while alive, then freed.
        unsafe {
            let mut sd: *mut c_void = null_mut();
            let rc = GetNamedSecurityInfoW(
                w.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut sd,
            );
            assert_eq!(rc, 0, "The access rules of a path could not be read");
            let mut text: *mut u16 = null_mut();
            let ok = ConvertSecurityDescriptorToStringSecurityDescriptorW(
                sd,
                1,
                DACL_SECURITY_INFORMATION,
                &mut text,
                null_mut(),
            );
            assert!(ok != 0, "The access rules could not be written as SDDL");
            let s = from_wide(text);
            LocalFree(text as *mut c_void);
            LocalFree(sd);
            s
        }
    }

    /// Replaces a path's access list with the one `sddl` describes, protected when it says `P`.
    pub fn set_dacl(path: &Path, sddl: &str) -> Result<(), u32> {
        let wp = wide(path.as_os_str());
        let ws = wide(std::ffi::OsStr::new(sddl));
        // SAFETY: The descriptor made from the SDDL text lives until the DACL has been applied.
        unsafe {
            let mut sd: *mut c_void = null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                ws.as_ptr(),
                1,
                &mut sd,
                null_mut(),
            ) == 0
            {
                return Err(1);
            }
            let mut present = 0i32;
            let mut defaulted = 0i32;
            let mut dacl: *mut ACL = null_mut();
            let mut control = 0u16;
            let mut rev = 0u32;
            GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted);
            GetSecurityDescriptorControl(sd, &mut control, &mut rev);
            let flag = if control & SE_DACL_PROTECTED != 0 {
                PROTECTED_DACL_SECURITY_INFORMATION
            } else {
                UNPROTECTED_DACL_SECURITY_INFORMATION
            };
            let rc = SetNamedSecurityInfoW(
                wp.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | flag,
                null_mut(),
                null_mut(),
                dacl,
                null_mut(),
            );
            LocalFree(sd);
            if rc == 0 { Ok(()) } else { Err(rc) }
        }
    }
}

pub use win::{Acl, elevated, read_acl, user_sid};

/// A guard that puts a path's access rules back as they were when it was made.
pub struct AclGuard {
    path: PathBuf,
    saved: String,
}

impl AclGuard {
    pub fn new(path: &Path) -> AclGuard {
        AclGuard {
            path: path.to_path_buf(),
            saved: win::read_sddl(path),
        }
    }
}

impl Drop for AclGuard {
    fn drop(&mut self) {
        // A failed restore is not hidden: the throwaway folder cannot then be deleted by a person
        // either, so the test says so.
        if let Err(code) = win::set_dacl(&self.path, &self.saved) {
            eprintln!("The access rules of a throwaway path could not be restored: {code}");
        }
    }
}

/// An SDDL access list: `entries` are (`A` or `D`, rights, SID); a folder's entries carry
/// object and container inherit.
pub fn sddl(dir: bool, protected: bool, entries: &[(char, &str, &str)]) -> String {
    let mut s = String::from("D:");
    if protected {
        s.push('P');
    }
    for (kind, rights, sid) in entries {
        let flags = if dir { "OICI" } else { "" };
        s.push_str(&format!("({kind};{flags};{rights};;;{sid})"));
    }
    s
}

pub fn set_acl(path: &Path, sddl: &str) {
    win::set_dacl(path, sddl).expect("A throwaway path's access rules could not be changed");
}

/// Passes when the path is owner-only as docs/state.md, Access rules, defines it.
#[track_caller]
pub fn assert_owner_only(path: &Path, dir: bool) {
    let me = user_sid();
    let a = read_acl(path);
    assert!(a.owner == me, "The owner is not the user's own SID");
    assert!(
        a.protected,
        "The access list is not protected from inheritance"
    );
    assert_eq!(
        a.aces.len(),
        1,
        "The access list does not hold exactly one entry"
    );
    let ace = &a.aces[0];
    assert!(ace.allow, "The one entry is not an allow entry");
    assert!(ace.sid == me, "The one entry is not for the user's SID");
    assert_eq!(
        ace.rights & FULL,
        FULL,
        "The one entry is short of full control"
    );
    assert_eq!(
        ace.flags & INHERITED,
        0,
        "The one entry is inherited, not explicit"
    );
    if dir {
        assert_eq!(
            ace.flags & (OBJECT_INHERIT | CONTAINER_INHERIT),
            OBJECT_INHERIT | CONTAINER_INHERIT,
            "A folder's one entry is not inheritable by files and folders"
        );
    }
}

/// A throwaway folder with a `home` in it. Dropping it deletes the folder.
pub struct Lab {
    tmp: tempfile::TempDir,
    pub home: PathBuf,
}

impl Lab {
    pub fn new() -> Lab {
        // The error's own text names the temp folder, so only its kind is shown.
        let tmp = tempfile::Builder::new()
            .prefix("agentspace-ca-spec-")
            .tempdir()
            .map_err(|e| e.kind())
            .expect("A throwaway folder could not be made");
        check_temp_is_neutral(tmp.path());
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        Lab { tmp, home }
    }

    /// The folder that holds the home, for files that are not in it.
    pub fn root(&self) -> &Path {
        self.tmp.path()
    }

    pub fn ctx(&self) -> Context {
        ctx_for(&self.home)
    }

    pub fn state_dir(&self) -> PathBuf {
        self.home.join(".agentspace")
    }

    pub fn ca_dir(&self) -> PathBuf {
        self.state_dir().join("ca")
    }

    pub fn key(&self) -> PathBuf {
        self.ca_dir().join("agentspace_ca")
    }

    pub fn pubkey(&self) -> PathBuf {
        self.ca_dir().join("agentspace_ca.pub")
    }

    /// A made-up OpenSSH folder that holds no ssh-keygen, as a `SystemRoot` for step 1.
    pub fn empty_system_root(&self) -> PathBuf {
        self.tmp.path().join("no-windows")
    }
}

pub fn real_system_root() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot MUST be set"))
}

pub fn ctx_for(home: &Path) -> Context {
    Context {
        home: home.to_path_buf(),
        local_app_data: Some(home.join("AppData").join("Local")),
        system_root: real_system_root(),
        nested: false,
    }
}

/// The library looks in every folder above the home for `.git` itself, so a temp folder inside a
/// git working tree would refuse every CA a test makes. Say so by name. (The OneDrive folders are
/// the `CaEnv`'s, which the tests make up, so the environment's own play no part.)
fn check_temp_is_neutral(tmp: &Path) {
    for dir in tmp.ancestors() {
        assert!(
            !dir.join(".git").exists(),
            "The temp folder is inside a git working tree; point TMP elsewhere to run these tests"
        );
    }
}

/// `<user>` as the messages carry it: the made-up `USERNAME` of [`ca_env`].
pub fn user_name() -> String {
    USER.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Progress(String, Option<CallerLine>),
    Warning(String),
    Detail(String),
}

/// A `Say` that records every call.
#[derive(Default)]
pub struct Recorder {
    pub events: RefCell<Vec<Event>>,
}

impl Say for Recorder {
    fn mode(&self) -> Mode {
        Mode::Person
    }
    fn progress(&self, person: &str, caller: Option<CallerLine>) {
        self.events
            .borrow_mut()
            .push(Event::Progress(person.to_string(), caller));
    }
    fn warning(&self, person: &str) {
        self.events
            .borrow_mut()
            .push(Event::Warning(person.to_string()));
    }
    fn detail(&self, text: &str) {
        self.events
            .borrow_mut()
            .push(Event::Detail(text.to_string()));
    }
}

impl Recorder {
    /// Every progress text, split into lines, in order.
    pub fn lines(&self) -> Vec<String> {
        self.events
            .borrow()
            .iter()
            .filter_map(|e| match e {
                Event::Progress(t, _) => Some(t.clone()),
                _ => None,
            })
            .flat_map(|t| {
                t.split('\n')
                    .map(|l| l.trim_end_matches('\r').to_string())
                    .collect::<Vec<_>>()
            })
            .filter(|l| !l.is_empty())
            .collect()
    }

    /// Whether nothing but progress was said.
    pub fn only_progress(&self) -> bool {
        self.events
            .borrow()
            .iter()
            .all(|e| matches!(e, Event::Progress(..)))
    }

    pub fn is_silent(&self) -> bool {
        self.events.borrow().is_empty()
    }
}

/// The two entry points that make the CA's checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// The operator's `ca`.
    Ca,
    /// `sync`'s step 1, `check_for_sync`.
    Sync,
}

pub const BOTH: [Entry; 2] = [Entry::Ca, Entry::Sync];

pub struct Outcome {
    pub result: Result<(), Failure>,
    pub lines: Vec<String>,
    pub only_progress: bool,
    pub silent: bool,
    /// What `check_for_sync` returned.
    pub fingerprint: Option<String>,
}

impl Outcome {
    pub fn message(&self) -> String {
        match &self.result {
            Ok(()) => panic!("The call succeeded; a failure was expected"),
            Err(f) => f.message.clone(),
        }
    }
    #[track_caller]
    pub fn ok(&self) {
        if let Err(f) = &self.result {
            panic!("The call failed: {}", masked(&f.message));
        }
    }
}

// ---- What a failing test may print ----

/// `text` with `what` replaced by `with` wherever it stands, in any case.
fn replace_ignoring_case(text: &str, what: &str, with: &str) -> String {
    if what.is_empty() {
        return text.to_string();
    }
    // ASCII lower case keeps every byte offset, so the two texts line up.
    let (hay, needle) = (text.to_ascii_lowercase(), what.to_ascii_lowercase());
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = hay[at..].find(&needle) {
        out.push_str(&text[at..at + found]);
        out.push_str(with);
        at += found + needle.len();
    }
    out.push_str(&text[at..]);
    out
}

/// `text` with whatever names this machine or its user replaced by a placeholder: the temp folder
/// and the home folder, as paths, then the computer's, the domain's and the user's names, and the
/// user's SID. The values are read only to be hidden, and are never shown.
pub fn masked(text: &str) -> String {
    let variable = |name: &str| {
        std::env::var_os(name)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let temp = std::env::temp_dir().to_string_lossy().into_owned();
    let mut hidden = [
        (temp.trim_end_matches(['\\', '/']).to_string(), "<temp>"),
        (variable("USERPROFILE"), "<home>"),
        (variable("COMPUTERNAME"), "<computer>"),
        (variable("USERDOMAIN"), "<domain>"),
        (variable("USERNAME"), "<user>"),
        (user_sid(), "<sid>"),
    ];
    hide(text, &mut hidden)
}

/// `text` with each of `hidden`'s values replaced by its placeholder, in any case.
fn hide(text: &str, hidden: &mut [(String, &str)]) -> String {
    // The longest first: one name may hold another (a user named for the computer, say), and the
    // shorter one replaced first would leave the rest of the longer one showing.
    hidden.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
    let mut out = text.to_string();
    for (value, placeholder) in hidden {
        // A value of one or two characters would hide half the text and name nobody.
        if value.len() > 2 {
            out = replace_ignoring_case(&out, value, placeholder);
        }
    }
    out
}

/// Passes when the two texts are equal, compared as they are. A failure shows both only as
/// `masked` leaves them.
#[track_caller]
pub fn same(actual: impl AsRef<str>, expected: impl AsRef<str>) {
    same_for(actual, expected, "");
}

/// `same`, with a word on which case failed.
#[track_caller]
pub fn same_for(actual: impl AsRef<str>, expected: impl AsRef<str>, what: impl std::fmt::Display) {
    let (actual, expected) = (actual.as_ref(), expected.as_ref());
    if actual == expected {
        return;
    }
    let what = match what.to_string() {
        what if what.is_empty() => what,
        what => format!("{what}: "),
    };
    let (shown, wanted) = (masked(actual), masked(expected));
    if shown == wanted {
        panic!(
            "{what}The two texts differ only in what is hidden: a name or a path of this machine's"
        );
    }
    panic!("{what}The text is not the one expected.\n  Actual:   {shown}\n  Expected: {wanted}");
}

/// Passes when the two lists of lines are equal, and shows them only as `masked` leaves them.
#[track_caller]
pub fn same_lines(actual: &[String], expected: &[String]) {
    same(actual.join("\n"), expected.join("\n"));
    assert!(
        actual.len() == expected.len(),
        "The lines are split differently"
    );
}

/// `check_for_sync`'s fingerprint. A failure's message is shown only as `masked` leaves it.
#[track_caller]
pub fn sync_ok(ctx: &Context) -> String {
    match agentspace_ca::check_for_sync(ctx, &ca_env()) {
        Ok(fingerprint) => fingerprint,
        Err(f) => panic!("check_for_sync failed: {}", masked(&f.message)),
    }
}

/// Runs `entry` with the tests' [`ca_env`].
pub fn run(entry: Entry, ctx: &Context) -> Outcome {
    run_with(entry, ctx, &ca_env())
}

/// Runs `entry` with the given `CaEnv`.
pub fn run_with(entry: Entry, ctx: &Context, env: &CaEnv) -> Outcome {
    let say = Recorder::default();
    match entry {
        Entry::Ca => {
            let result = agentspace_ca::ca(ctx, env, &say);
            Outcome {
                result,
                lines: say.lines(),
                only_progress: say.only_progress(),
                silent: say.is_silent(),
                fingerprint: None,
            }
        }
        Entry::Sync => {
            let r = agentspace_ca::check_for_sync(ctx, env);
            let (result, fingerprint) = match r {
                Ok(fp) => (Ok(()), Some(fp)),
                Err(f) => (Err(f), None),
            };
            Outcome {
                result,
                lines: Vec::new(),
                only_progress: true,
                silent: true,
                fingerprint,
            }
        }
    }
}

/// Runs `ca` and expects it to succeed.
pub fn make_ca(lab: &Lab) -> Outcome {
    let o = run(Entry::Ca, &lab.ctx());
    o.ok();
    o
}

/// Tests that make a CA cannot run from an elevated shell, where `ca` refuses (state.md, The CA).
/// Returns `true` and says so when this one is.
pub fn skip_when_elevated() -> bool {
    if elevated() {
        eprintln!("Skipped: the process is elevated, and ca refuses to create a CA then");
        true
    } else {
        false
    }
}

/// The fingerprint of a public key file, worked out here from its first two words.
pub fn fingerprint_of(pubfile: &Path) -> String {
    let text = fs::read_to_string(pubfile).expect("The public key could not be read");
    let words: Vec<&str> = text.split_whitespace().collect();
    let key = ssh_key::PublicKey::from_openssh(&format!("{} {}", words[0], words[1]))
        .expect("The public key does not parse");
    key.fingerprint(ssh_key::HashAlg::Sha256).to_string()
}

/// A new ed25519 public key line, which is nobody's CA.
pub fn other_public_key_line() -> String {
    let key =
        ssh_key::PrivateKey::random(&mut ssh_key::rand_core::OsRng, ssh_key::Algorithm::Ed25519)
            .expect("A key could not be made");
    key.public_key()
        .to_openssh()
        .expect("A key could not be written")
}

/// Everything in a folder, with sizes and contents of files, sorted, for "nothing changed".
pub fn tree(dir: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    let mut out = Vec::new();
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, Option<Vec<u8>>)>) {
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
                if p.is_dir() {
                    out.push((rel, None));
                    walk(base, &p, out);
                } else {
                    out.push((rel, Some(fs::read(&p).unwrap_or_default())));
                }
            }
        }
    }
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// What a check MUST leave alone: both files' bytes, modification times and access rules, and the
/// folder's listing and access rules. It has no `Debug`: the public key's comment names the
/// computer, so a snapshot is compared with `assert_unchanged`, which names what changed and
/// shows no value.
#[derive(PartialEq, Eq)]
pub struct Snapshot {
    key: Vec<u8>,
    pubkey: Vec<u8>,
    key_time: std::time::SystemTime,
    pub_time: std::time::SystemTime,
    dir_acl: Acl,
    key_acl: Acl,
    listing: Vec<(String, Option<Vec<u8>>)>,
}

pub fn snapshot(lab: &Lab) -> Snapshot {
    Snapshot {
        key: fs::read(lab.key()).unwrap(),
        pubkey: fs::read(lab.pubkey()).unwrap(),
        key_time: fs::metadata(lab.key()).unwrap().modified().unwrap(),
        pub_time: fs::metadata(lab.pubkey()).unwrap().modified().unwrap(),
        dir_acl: read_acl(&lab.ca_dir()),
        key_acl: read_acl(&lab.key()),
        listing: tree(&lab.ca_dir()),
    }
}

/// Passes when the CA is as `before` found it. A failure names each thing that changed, and shows
/// none of them.
#[track_caller]
pub fn assert_unchanged(lab: &Lab, before: &Snapshot, what: &str) {
    let now = snapshot(lab);
    let mut changed = Vec::new();
    if now.key != before.key {
        changed.push("the private key's bytes");
    }
    if now.pubkey != before.pubkey {
        changed.push("the public key's bytes");
    }
    if now.key_time != before.key_time {
        changed.push("the private key's modification time");
    }
    if now.pub_time != before.pub_time {
        changed.push("the public key's modification time");
    }
    if now.dir_acl != before.dir_acl {
        changed.push("the folder's access rules");
    }
    if now.key_acl != before.key_acl {
        changed.push("the private key's access rules");
    }
    if now.listing != before.listing {
        changed.push("the folder's listing");
    }
    assert!(
        changed.is_empty() && now == *before,
        "{what}: The CA changed: {}",
        changed.join(", ")
    );
}

// The messages of docs/operator.md, ca, exactly, each as one line.

pub fn m_keygen_missing(system_root: &Path) -> String {
    let path = system_root
        .join("System32")
        .join("OpenSSH")
        .join("ssh-keygen.exe");
    format!(
        "agentspace: {} is missing; install the Windows OpenSSH client.",
        path.display()
    )
}

pub fn m_synced_or_repo(ca_dir: &Path, folder: &str) -> String {
    format!(
        "agentspace: {} is inside {folder}; the CA key MUST NOT live in a synced folder or in the repo.",
        ca_dir.display()
    )
}

pub fn m_key_without_pub(key: &Path, pubkey: &Path) -> String {
    format!(
        "agentspace: {} exists but {} does not. ca does not repair a half-made CA: restore the public key, or write it again from the private key with ssh-keygen -y, then run ca again.",
        key.display(),
        pubkey.display()
    )
}

pub fn m_pub_without_key(key: &Path, pubkey: &Path) -> String {
    format!(
        "agentspace: {} exists but {} does not: the CA's private key is lost, and nothing more can be signed with it. ca does not replace it on its own. To make a new CA, delete {} and run ca again, then run sync for every host and re-issue every lease's certificate.",
        pubkey.display(),
        key.display(),
        pubkey.display()
    )
}

pub const M_ELEVATED: &str = "agentspace: ca MUST NOT create the CA from an elevated shell; run it without \"Run as administrator\".";

pub fn m_creating(ca_dir: &Path) -> String {
    format!("Creating the agentspace SSH CA in {}...", ca_dir.display())
}

pub fn m_no_key(key: &Path) -> String {
    format!("agentspace: {} does not exist; run ca.", key.display())
}

pub fn m_loose_prefix(path: &Path, user: &str) -> String {
    format!(
        "agentspace: {} MUST be {user}'s alone, but it ",
        path.display()
    )
}

pub fn m_loose_suffix(ca_dir: &Path, key: &Path, user: &str) -> String {
    format!(
        ". ca does not change access rules on an existing CA. If anyone else may have read the key, replace the CA (D17). Otherwise lock it by hand: icacls \"{}\" /inheritance:r /grant:r \"{user}:(OI)(CI)F\", then icacls \"{}\" /reset, then icacls \"{}\" /inheritance:d, then remove any other rule icacls still lists, and run ca again.",
        ca_dir.display(),
        key.display(),
        key.display()
    )
}

pub fn m_loose(path: &Path, problems: &str, ca_dir: &Path, key: &Path, user: &str) -> String {
    format!(
        "{}{problems}{}",
        m_loose_prefix(path, user),
        m_loose_suffix(ca_dir, key, user)
    )
}

/// Compares a message whose `<problems>` holds one `<rights>`, whose exact text is not part of the
/// contract (operator.md, The forms of the placeholders): `template` carries `{RIGHTS}` where the
/// rights go, and anything of one line stands for them.
pub fn matches_with_rights(actual: &str, template: &str) -> bool {
    let Some((head, tail)) = template.split_once("{RIGHTS}") else {
        return actual == template;
    };
    let Some(rest) = actual.strip_prefix(head) else {
        return false;
    };
    let Some(middle) = rest.strip_suffix(tail) else {
        return false;
    };
    !middle.is_empty() && !middle.contains('\n')
}

pub fn m_no_passphrase(key: &Path) -> String {
    format!(
        "agentspace: ssh-keygen could not read {} without a passphrase; the lease CLI signs unattended (D17).",
        key.display()
    )
}

pub fn m_mismatch(pubkey: &Path, key: &Path) -> String {
    format!(
        "agentspace: {} is not the ed25519 public key of {}. ca does not repair a mismatched pair.",
        pubkey.display(),
        key.display()
    )
}

/// Windows OpenSSH's ssh-keygen by its full path, run with a throwaway working folder and a
/// throwaway home in its environment (`cwd`), so that it is never pointed at the real profile.
pub fn ssh_keygen(cwd: &Path, args: &[&str]) -> bool {
    let exe = real_system_root()
        .join("System32")
        .join("OpenSSH")
        .join("ssh-keygen.exe");
    std::process::Command::new(exe)
        .args(args)
        .current_dir(cwd)
        .env("USERPROFILE", cwd)
        .env("HOME", cwd)
        .env("LOCALAPPDATA", cwd.join("AppData").join("Local"))
        .env("APPDATA", cwd.join("AppData").join("Roaming"))
        .env("TEMP", cwd)
        .env("TMP", cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
