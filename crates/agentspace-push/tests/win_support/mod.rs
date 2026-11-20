//! The Windows tests' rig for the token push (docs/operator.md, The token push; The test seam).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! One rig is one throwaway world: a home, a `LOCALAPPDATA` laid out as operator.md says, a folder
//! of stand-ins, and a working folder, all under the temp folder. It points `USERPROFILE`, `HOME`,
//! `LOCALAPPDATA` and `APPDATA` of the test process at them, sets the seam's variables, and takes a
//! lock so that two tests of one file never share the process environment. A rig puts everything
//! back when it drops.
//!
//! **A test MUST NOT print a captured environment's values, nor the token.** Every helper here
//! reports a mismatch by name or count.
#![allow(dead_code)]
#![allow(unsafe_code)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use agentspace_operator_api::{CallerLine, Context, Mode, Say};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, GetLastError, HANDLE, LocalFree, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};

// ---------------------------------------------------------------------------------------------
// The fixed texts of operator.md (The token push)
// ---------------------------------------------------------------------------------------------

pub const SENDING: &str = "Sending the localswim Bot token to the instance...";
pub const NOT_CONFIRMED: &str = "agentspace: The instance did not confirm the localswim token; the board will not work on the box this session.";
pub const NOT_READ: &str = "agentspace: Could not read the localswim Bot token; the board will not work on the box this session.";
pub const COULD_NOT_SEND: &str = "agentspace: Could not send the localswim token; the board will not work on the box this session.";
pub const TOOK_TOO_LONG: &str = "agentspace: Sending the localswim token took over 20 seconds and was stopped; the board will not work on the box this session.";
pub const NO_ZONE: &str =
    "agentspace: Could not work out the laptop's time zone; times on the box stay in its own zone.";
pub const BAD_ALIAS: &str = "agentspace: The SSH host alias for the localswim token is not valid; the board will not work on the box this session.";

/// The warning for a box that did not take the zone it was sent.
pub fn zone_not_taken(zone: &str) -> String {
    format!(
        "agentspace: The instance did not take the time zone {zone}; times on the box stay in its own zone."
    )
}

/// A name for a warning's text that is safe to print: the fixed texts by name, anything else by its
/// length alone.
fn label(text: &str) -> String {
    match text {
        NOT_CONFIRMED => "not-confirmed".into(),
        NOT_READ => "could-not-read".into(),
        COULD_NOT_SEND => "could-not-send".into(),
        TOOK_TOO_LONG => "took-too-long".into(),
        NO_ZONE => "no-zone".into(),
        BAD_ALIAS => "bad-alias".into(),
        t if t.starts_with("agentspace: The instance did not take the time zone ") => {
            "zone-not-taken".into()
        }
        t => format!("other text of {} characters", t.chars().count()),
    }
}

// ---------------------------------------------------------------------------------------------
// A fake Say
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Progress(String, Option<CallerLine>),
    Warning(String),
    Detail(String),
}

/// A `Say` that records what the push says. Nothing is printed.
pub struct FakeSay {
    mode: Mode,
    events: Mutex<Vec<Event>>,
}

impl FakeSay {
    pub fn person() -> Self {
        Self::with_mode(Mode::Person)
    }

    pub fn caller() -> Self {
        Self::with_mode(Mode::Caller)
    }

    fn with_mode(mode: Mode) -> Self {
        FakeSay {
            mode,
            events: Mutex::new(Vec::new()),
        }
    }

    fn events(&self) -> Vec<Event> {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn warnings(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Warning(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    /// The person's text of every progress line.
    pub fn progress_lines(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Progress(t, _) => Some(t),
                _ => None,
            })
            .collect()
    }

    /// The caller's line of every progress call.
    pub fn progress_caller_lines(&self) -> Vec<Option<CallerLine>> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Progress(_, c) => Some(c),
                _ => None,
            })
            .collect()
    }

    pub fn details(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Detail(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    pub fn silent(&self) -> bool {
        self.events().is_empty()
    }

    /// Whether any message of any kind holds `needle`.
    pub fn any_text_contains(&self, needle: &str) -> bool {
        self.events().iter().any(|e| match e {
            Event::Progress(t, _) | Event::Warning(t) | Event::Detail(t) => t.contains(needle),
        })
    }
}

impl Say for FakeSay {
    fn mode(&self) -> Mode {
        self.mode
    }

    fn progress(&self, person: &str, caller: Option<CallerLine>) {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Event::Progress(person.to_string(), caller));
    }

    fn warning(&self, person: &str) {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Event::Warning(person.to_string()));
    }

    fn detail(&self, text: &str) {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Event::Detail(text.to_string()));
    }
}

/// Asserts the warnings are exactly `expected`, in order. A mismatch is reported by label and
/// count, never by the text.
pub fn assert_warnings(say: &FakeSay, expected: &[&str], what: &str) {
    let got = say.warnings();
    let same = got.len() == expected.len() && got.iter().zip(expected).all(|(a, b)| a == b);
    if !same {
        let want: Vec<String> = expected.iter().map(|t| label(t)).collect();
        let have: Vec<String> = got.iter().map(|t| label(t)).collect();
        panic!(
            "{what}: expected {} warning(s) {:?}, got {} warning(s) {:?}",
            want.len(),
            want,
            have.len(),
            have
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The process-wide state a rig changes
// ---------------------------------------------------------------------------------------------

static LOCK: Mutex<()> = Mutex::new(());
static COUNTER: AtomicU64 = AtomicU64::new(0);

pub const INSTANCE: &str = "i-00000000000000000";
pub const DEFAULT_BASE: &str = "http://127.0.0.1:18811";
pub const DEFAULT_ALIAS: &str = "conformance-push-alias";
pub const DEFAULT_ZONE: &str = "America/New_York";
/// Planted in the test process's environment, so that every stand-in could inherit them.
pub const PLANTED_FD_STATE: [&str; 2] = [
    "0F1E2D3C_4B5A_6978_8796_A5B4C3D2E1F0_POSIX_FD_STATE",
    "lower_case_posix_fd_state",
];
const GUID: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

/// A made-up token that no other text in a run holds.
pub fn dummy_token() -> String {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "Dummy_{:016x}{:08x}_{:08x}",
        t as u64,
        std::process::id(),
        n
    )
}

pub struct Rig {
    _guard: MutexGuard<'static, ()>,
    saved_cwd: PathBuf,
    saved_env: Vec<(String, Option<OsString>)>,
    owned: Vec<Child>,
    _root: tempfile::TempDir,
    pub home: PathBuf,
    pub lad: PathBuf,
    pub standin_dir: PathBuf,
    pub work: PathBuf,
    /// A folder with no OpenSSH in it: the context's `SystemRoot`, so the real ssh is out of reach.
    pub sysroot: PathBuf,
    pub token: String,
    pub base: String,
    pub nested: bool,
}

impl Rig {
    /// A rig with a receipt, a credential, a stand-in ssh answering `ok`, and `AGENTSPACE_TIMEZONE`
    /// set to `America/New_York`.
    pub fn new() -> Rig {
        Rig::in_folder("standins")
    }

    /// The same, with the stand-ins in a folder of the given name (which may hold spaces).
    pub fn in_folder(standin_folder: &str) -> Rig {
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = tempfile::tempdir().expect("a throwaway folder");
        let base_dir = root.path().to_path_buf();
        let home = base_dir.join("home");
        let lad = base_dir.join("local");
        let standin_dir = base_dir.join(standin_folder);
        let work = base_dir.join("work");
        let sysroot = base_dir.join("sysroot");
        for d in [&home, &lad, &standin_dir, &work, &sysroot] {
            fs::create_dir_all(d).expect("a throwaway subfolder");
        }
        fs::create_dir_all(home.join("AppData").join("Roaming")).expect("a throwaway subfolder");
        let mut rig = Rig {
            _guard: guard,
            saved_cwd: std::env::current_dir().expect("the working folder"),
            saved_env: Vec::new(),
            owned: Vec::new(),
            _root: root,
            home,
            lad,
            standin_dir,
            work,
            sysroot,
            token: dummy_token(),
            base: DEFAULT_BASE.to_string(),
            nested: false,
        };
        // The stand-in's working folder is the process's: a throwaway one, never the repository.
        std::env::set_current_dir(&rig.work).expect("the throwaway working folder");
        let home = rig.home.clone();
        let lad = rig.lad.clone();
        rig.set_env("USERPROFILE", Some(home.as_os_str()));
        rig.set_env("HOME", Some(home.as_os_str()));
        rig.set_env("LOCALAPPDATA", Some(lad.as_os_str()));
        rig.set_env(
            "APPDATA",
            Some(home.join("AppData").join("Roaming").as_os_str()),
        );
        rig.set_env("AGENTSPACE_NESTED", None);
        for name in PLANTED_FD_STATE {
            rig.set_env(name, Some(std::ffi::OsStr::new("planted")));
        }
        rig.write_standins();
        rig.scenario("ok");
        rig.answer_zone(DEFAULT_ZONE);
        rig.set_timezone(Some(DEFAULT_ZONE));
        rig.use_standin();
        rig.write_receipt();
        rig.write_credential();
        rig
    }

    // ---- the environment ----

    /// Sets (or, for `None`, removes) a variable of the test process, to be put back on drop.
    pub fn set_env(&mut self, name: &str, value: Option<&std::ffi::OsStr>) {
        if !self.saved_env.iter().any(|(n, _)| n == name) {
            self.saved_env
                .push((name.to_string(), std::env::var_os(name)));
        }
        // SAFETY: every test of a file holds the rig's lock for its whole run, so no other test
        // thread reads or writes the environment while this runs.
        unsafe {
            match value {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
    }

    pub fn set_timezone(&mut self, zone: Option<&str>) {
        self.set_env("AGENTSPACE_TIMEZONE", zone.map(std::ffi::OsStr::new));
    }

    pub fn set_ssh_seam(&mut self, value: Option<&std::ffi::OsStr>) {
        self.set_env("AGENTSPACE_SSH_CMD", value);
    }

    /// Points `AGENTSPACE_SSH_CMD` at this rig's stand-in.
    pub fn use_standin(&mut self) {
        let cmd = self.standin_cmd();
        self.set_ssh_seam(Some(cmd.as_os_str()));
    }

    pub fn standin_cmd(&self) -> PathBuf {
        self.standin_dir.join("ssh.cmd")
    }

    // ---- the stand-in ----

    fn write_standins(&self) {
        fs::write(
            self.standin_dir.join("ssh-standin.ps1"),
            include_str!("ssh-standin.ps1"),
        )
        .expect("the stand-in's script");
        let cmd = "@echo off\r\n\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"%~dp0ssh-standin.ps1\" %*\r\nexit /b %ERRORLEVEL%\r\n";
        fs::write(self.standin_cmd(), cmd).expect("the stand-in's .cmd");
    }

    /// What the stand-in does: `ok`, `exit255`, `nohash`, `hang`, ... (ssh-standin.ps1 lists them).
    pub fn scenario(&self, name: &str) {
        fs::write(self.standin_dir.join("scenario.txt"), name).expect("the scenario file");
    }

    /// The zone the stand-in answers with in its `zone <zone>` line (empty: no line).
    pub fn answer_zone(&self, zone: &str) {
        fs::write(self.standin_dir.join("zone.txt"), zone).expect("the zone file");
    }

    /// How many times the stand-in ran.
    pub fn calls(&self) -> usize {
        fs::read_dir(&self.standin_dir)
            .map(|d| {
                d.filter_map(Result::ok)
                    .filter(|e| {
                        let n = e.file_name().to_string_lossy().into_owned();
                        n.starts_with("args-") && n.ends_with(".json")
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    /// The arguments of the n-th call (1-based).
    pub fn args(&self, n: usize) -> Vec<String> {
        let text = fs::read_to_string(self.standin_dir.join(format!("args-{n}.json")))
            .expect("the recorded arguments");
        let v: serde_json::Value = serde_json::from_str(&text).expect("the recorded arguments");
        v.as_array()
            .expect("the recorded arguments are an array")
            .iter()
            .map(|a| a.as_str().expect("an argument is text").to_string())
            .collect()
    }

    /// The standard input of the n-th call.
    pub fn stdin(&self, n: usize) -> Vec<u8> {
        fs::read(self.standin_dir.join(format!("stdin-{n}.bin"))).expect("the recorded stdin")
    }

    /// The names and values of the environment the n-th call saw. A caller MUST NOT print them.
    pub fn env_pairs(&self, n: usize) -> Vec<(String, String)> {
        let text = fs::read_to_string(self.standin_dir.join(format!("env-{n}.txt")))
            .expect("the recorded environment");
        text.lines()
            .filter_map(|l| {
                let (k, v) = l.split_once('=')?;
                if k.is_empty() {
                    None
                } else {
                    Some((k.to_string(), v.to_string()))
                }
            })
            .collect()
    }

    /// The working folder the n-th call ran in.
    pub fn cwd_of(&self, n: usize) -> PathBuf {
        PathBuf::from(
            fs::read_to_string(self.standin_dir.join(format!("cwd-{n}.txt")))
                .expect("the recorded working folder"),
        )
    }

    /// The processes the stand-in recorded, as (tag, call, process). A line without a creation
    /// time is skipped, since its process could not be told from a later one with the same ID.
    pub fn recorded_procs(&self) -> Vec<(String, usize, Proc)> {
        let Ok(text) = fs::read_to_string(self.standin_dir.join("pids.txt")) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                let tag = it.next()?.to_string();
                let n = it.next()?.parse().ok()?;
                let pid = it.next()?.parse().ok()?;
                let created = it.next()?.parse().ok()?;
                Some((tag, n, Proc { pid, created }))
            })
            .collect()
    }

    pub fn procs_tagged(&self, tag: &str) -> Vec<Proc> {
        self.recorded_procs()
            .into_iter()
            .filter(|(t, _, _)| t == tag)
            .map(|(_, _, p)| p)
            .collect()
    }

    // ---- the receipt and the credential ----

    pub fn local_swim_dir(&self) -> PathBuf {
        self.lad.join("localswim-v2")
    }

    pub fn credential_dir(&self) -> PathBuf {
        self.local_swim_dir().join("credentials").join(GUID)
    }

    pub fn receipt_path(&self) -> PathBuf {
        self.local_swim_dir().join("service.json")
    }

    pub fn bot_path(&self) -> PathBuf {
        self.credential_dir().join("bot.xml")
    }

    /// Writes the receipt, as localswim does, from `self.base`.
    pub fn write_receipt(&self) {
        self.write_receipt_text(
            &serde_json::json!({
                "base": self.base,
                "credential_directory": self.credential_dir().to_string_lossy(),
                "executable": "not-used.exe",
            })
            .to_string(),
        );
    }

    pub fn write_receipt_text(&self, text: &str) {
        fs::create_dir_all(self.local_swim_dir()).expect("localswim's folder");
        fs::write(self.receipt_path(), text).expect("the receipt");
    }

    /// Writes `bot.xml` with `self.token`, protected for the current user, as PowerShell 7 writes it
    /// (UTF-8, no mark).
    pub fn write_credential(&self) {
        self.write_credential_for(&self.token.clone());
    }

    pub fn write_credential_for(&self, token: &str) {
        self.write_credential_plain(&utf16le(token));
    }

    /// Writes `bot.xml` holding `plain` (the plaintext bytes) protected for the current user.
    pub fn write_credential_plain(&self, plain: &[u8]) {
        let hex = to_hex(&protect(plain));
        self.write_bot_bytes(clixml(&hex).as_bytes());
    }

    pub fn write_bot_bytes(&self, bytes: &[u8]) {
        fs::create_dir_all(self.credential_dir()).expect("the credential folder");
        fs::write(self.bot_path(), bytes).expect("bot.xml");
    }

    // ---- the push ----

    pub fn context(&self) -> Context {
        Context {
            home: self.home.clone(),
            local_app_data: Some(self.lad.clone()),
            system_root: self.sysroot.clone(),
            nested: self.nested,
        }
    }

    /// Runs the push and gives how long it took.
    pub fn push(&self, say: &FakeSay, alias: &str, forced: bool) -> Duration {
        let ctx = self.context();
        let started = Instant::now();
        agentspace_push::push(&ctx, say, INSTANCE, alias, forced);
        started.elapsed()
    }

    /// A push over the default alias, not forced, with a fresh person's `Say`.
    pub fn up(&self) -> FakeSay {
        let say = FakeSay::person();
        self.push(&say, DEFAULT_ALIAS, false);
        say
    }

    // ---- the content and the stamp ----

    pub fn content(&self) -> String {
        format!(
            "LOCALSWIM_URL={}\nLOCALSWIM_TOKEN={}\n",
            self.base, self.token
        )
    }

    pub fn agentspace_dir(&self) -> PathBuf {
        self.home.join(".agentspace")
    }

    pub fn stamp_path(&self) -> PathBuf {
        self.agentspace_dir().join("localswim-pushed.json")
    }

    pub fn stamp_bytes(&self) -> Option<Vec<u8>> {
        fs::read(self.stamp_path()).ok()
    }

    /// The stamp operator.md defines, as uppercase hex, for this rig's content and `zone`.
    pub fn expected_stamp(&self, zone: &str) -> String {
        expected_stamp(INSTANCE, zone, &self.content())
    }

    /// The stamp file's exact bytes (docs/state.md, The push stamp).
    pub fn expected_stamp_file(&self, zone: &str) -> Vec<u8> {
        format!(
            "{{\"version\":0,\"stamp\":\"{}\"}}",
            self.expected_stamp(zone)
        )
        .into_bytes()
    }

    /// Writes a stamp file for `zone`, as the push would have left it.
    pub fn write_stamp_for(&self, zone: &str) {
        fs::create_dir_all(self.agentspace_dir()).expect("the state folder");
        fs::write(self.stamp_path(), self.expected_stamp_file(zone)).expect("the stamp");
    }

    pub fn stamp_matches(&self, zone: &str) -> bool {
        self.stamp_bytes().as_deref() == Some(self.expected_stamp_file(zone).as_slice())
    }

    /// Whether the state folder holds a temporary file of the stamp's atomic replace.
    pub fn stamp_temp_left(&self) -> bool {
        fs::read_dir(self.agentspace_dir())
            .map(|d| {
                d.filter_map(Result::ok)
                    .any(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            })
            .unwrap_or(false)
    }

    // ---- processes ----

    /// Starts a process of the test's own that is no relation of the push's ssh, and gives its PID.
    /// It is ended when the rig drops.
    pub fn spawn_sibling(&mut self) -> u32 {
        let ps = Path::new(&std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        let child = Command::new(ps)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 90",
            ])
            .current_dir(&self.work)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("a sibling process");
        let pid = child.id();
        self.owned.push(child);
        pid
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        // Only a process that is still the one the stand-in recorded: an ID whose process has
        // ended may already belong to another process, which MUST NOT be touched.
        for (_, _, proc) in self.recorded_procs() {
            end_if_same(proc);
        }
        for child in &mut self.owned {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::env::set_current_dir(&self.saved_cwd);
        for (name, value) in self.saved_env.iter().rev() {
            // SAFETY: the rig's lock is still held (`_guard` drops after this).
            unsafe {
                match value {
                    Some(v) => std::env::set_var(name, v),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------------------------

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

/// The SHA-256 of `<instance> LF <zone> LF <content>`, as uppercase hex (operator.md, The stamp).
pub fn expected_stamp(instance: &str, zone: &str, content: &str) -> String {
    let mut h = Sha256::new();
    h.update(format!("{instance}\n{zone}\n{content}").as_bytes());
    to_hex(&h.finalize()).to_ascii_uppercase()
}

/// A credential document as both PowerShell runtimes write it (operator.md, The Bot credential).
pub fn clixml(password_hex: &str) -> String {
    format!(
        "<Objs Version=\"1.1.0.1\" xmlns=\"http://schemas.microsoft.com/powershell/2004/04\"><Obj RefId=\"0\"><TN RefId=\"0\"><T>System.Management.Automation.PSCredential</T><T>System.Object</T></TN><ToString>System.Management.Automation.PSCredential</ToString><Props><S N=\"UserName\">bot</S><SS N=\"Password\">{password_hex}</SS></Props></Obj></Objs>"
    )
}

/// The same as Windows PowerShell 5.1 writes it: UTF-16LE with the mark `FF FE`.
pub fn utf16le_with_mark(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    out.extend(utf16le(text));
    out
}

/// `CryptProtectData` on `plain` for the current user, with no entropy and
/// `CRYPTPROTECT_UI_FORBIDDEN`, as operator.md says a test writes a credential.
pub fn try_protect(plain: &[u8]) -> Option<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `input` points at `plain`, which outlives the call; `out` is written by the call and
    // freed below with `LocalFree`, as the API requires.
    unsafe {
        let ok = CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        );
        if ok == 0 || out.pbData.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as *mut core::ffi::c_void);
        Some(bytes)
    }
}

pub fn protect(plain: &[u8]) -> Vec<u8> {
    try_protect(plain).expect("CryptProtectData failed for a dummy credential")
}

/// Whether the process is still running. A process that cannot be found is not.
pub fn process_alive(pid: u32) -> bool {
    // SAFETY: plain Win32 calls on a handle this function opens and closes.
    unsafe {
        let h = OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if h.is_null() {
            return GetLastError() != ERROR_INVALID_PARAMETER;
        }
        let r = WaitForSingleObject(h, 0);
        CloseHandle(h);
        r == WAIT_TIMEOUT
    }
}

/// A process the stand-in recorded: its ID, and its creation time as a FILETIME, which together
/// name one process, whatever process the ID is given to later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub created: u64,
}

/// The creation time of an open process, as a FILETIME.
///
/// # Safety
///
/// `h` MUST be an open process handle with at least `PROCESS_QUERY_LIMITED_INFORMATION`.
unsafe fn created_at(h: HANDLE) -> Option<u64> {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: the caller gives an open handle; the four times are valid for the call to fill.
    let ok = unsafe { GetProcessTimes(h, &mut created, &mut exited, &mut kernel, &mut user) };
    (ok != 0).then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

/// Whether the recorded process is still running. An ID now held by another process, one created
/// at another time, is not.
pub fn proc_alive(p: Proc) -> bool {
    // SAFETY: plain Win32 calls on a handle this function opens and closes.
    unsafe {
        let h = OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            p.pid,
        );
        if h.is_null() {
            // A process that cannot be opened but exists is taken as running, so that a check
            // that it ended never passes on a process it could not see.
            return GetLastError() != ERROR_INVALID_PARAMETER;
        }
        let same = created_at(h) == Some(p.created);
        let running = same && WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
        CloseHandle(h);
        running
    }
}

/// Ends a process the stand-in's tree started, for the clean-up of a rig only, and only while its
/// ID is still that process's: the check and the termination go through one handle, which keeps the
/// ID from being given to another process in between.
pub fn end_if_same(p: Proc) {
    // SAFETY: plain Win32 calls on a handle this function opens and closes.
    unsafe {
        let h = OpenProcess(
            PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            p.pid,
        );
        if h.is_null() {
            return;
        }
        if created_at(h) == Some(p.created) {
            TerminateProcess(h, 1);
        }
        CloseHandle(h);
    }
}

/// Waits up to `limit` for every recorded process to be gone.
pub fn wait_all_gone(procs: &[Proc], limit: Duration) -> bool {
    let started = Instant::now();
    loop {
        if procs.iter().all(|p| !proc_alive(*p)) {
            return true;
        }
        if started.elapsed() > limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Whether a name of an environment variable ends `_POSIX_FD_STATE`, in any case.
pub fn is_fd_state(name: &str) -> bool {
    name.to_ascii_uppercase().ends_with("_POSIX_FD_STATE")
}

/// The Windows ID of the laptop's zone, from `tzutil /g`.
pub fn laptop_windows_zone_id() -> Option<String> {
    let root = std::env::var_os("SystemRoot")?;
    let out = Command::new(Path::new(&root).join("System32").join("tzutil.exe"))
        .arg("/g")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Every file under `dir`, recursively.
pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// Whether the bytes hold `needle`.
pub fn bytes_contain(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}
