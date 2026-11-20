//! Shared fixtures of the implementer's own tests of the Windows half (v0.06-T02): a throwaway
//! sandbox, a stand-in ssh (a `.cmd` that starts a PowerShell script), a dummy credential written
//! with `CryptProtectData`, and checks that a process is, or is not, still running.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Windows only.
//!
//! Every value is a dummy. The stand-in writes only into its own folder, which is a throwaway one
//! under the temp folder, with a space in its name; it runs with a throwaway working folder and a
//! throwaway home. A test here never runs the real ssh, never reads the real `LOCALAPPDATA` and
//! never reaches a box. It stops only processes that its own stand-in started.
#![cfg(windows)]
#![allow(dead_code, unsafe_code)]

use agentspace_push::{Context, Say};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
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

pub const INSTANCE: &str = "i-0123456789abcdef0";
pub const BASE: &str = "http://127.0.0.1:8811";
/// A dummy token, not a real one.
pub const TOKEN: &str = "dummy-token_Zk3Qp9xL2mVb7RtYw5NcHs8DfGa1JeUo0iX4";
pub const CONTENT: &str = "LOCALSWIM_URL=http://127.0.0.1:8811\nLOCALSWIM_TOKEN=dummy-token_Zk3Qp9xL2mVb7RtYw5NcHs8DfGa1JeUo0iX4\n";
pub const ALIAS: &str = "agentspace";

/// A person's `Say`: the lines that would reach stderr.
#[derive(Default)]
pub struct Lines(pub RefCell<Vec<String>>);

impl Lines {
    pub fn printed(&self) -> Vec<String> {
        self.0.borrow().clone()
    }
}

impl Say for Lines {
    fn mode(&self) -> agentspace_operator_api::Mode {
        agentspace_operator_api::Mode::Person
    }
    fn progress(&self, person: &str, _caller: Option<agentspace_operator_api::CallerLine>) {
        self.0.borrow_mut().push(person.to_string());
    }
    fn warning(&self, person: &str) {
        self.0.borrow_mut().push(person.to_string());
    }
    fn detail(&self, _text: &str) {}
}

/// The stand-in's PowerShell script. It records its arguments, its environment, its process ID and
/// its stdin into its own folder, then acts as `mode.txt` says.
const STANDIN_PS1: &str = r#"
$ErrorActionPreference = 'Stop'
$dir = $PSScriptRoot
$mode = (Get-Content -LiteralPath (Join-Path $dir 'mode.txt') -Raw).Trim()
$n = @(Get-ChildItem -LiteralPath $dir -Filter 'args-*.json').Count + 1
function Rec([string]$name, [string]$text) { [IO.File]::WriteAllText((Join-Path $dir $name), $text) }
# A process is recorded by its ID and its creation time, so that a test ends it only while the ID is
# still that process's.
Rec "pid-ps-$n.txt" "$PID $([Diagnostics.Process]::GetCurrentProcess().StartTime.ToFileTimeUtc())"
Rec "args-$n.json" (ConvertTo-Json -InputObject @($args) -Compress)
Rec "env-$n.txt" ((Get-ChildItem Env: | ForEach-Object { $_.Name + '=' + $_.Value }) -join "`n")
Rec "cwd-$n.txt" ((Get-Location).Path)
$bytes = [byte[]]@()
if ($mode -notmatch '^noread') {
  $m = New-Object IO.MemoryStream
  [Console]::OpenStandardInput().CopyTo($m)
  $bytes = $m.ToArray()
  [IO.File]::WriteAllBytes((Join-Path $dir "stdin-$n.bin"), $bytes)
}
function Start-Sleeper([int]$seconds) {
  $psi = New-Object Diagnostics.ProcessStartInfo
  $psi.FileName = (Join-Path $PSHOME 'powershell.exe')
  $psi.Arguments = "-NoProfile -Command Start-Sleep -Seconds $seconds"
  $psi.UseShellExecute = $false
  $p = [Diagnostics.Process]::Start($psi)
  Rec "pid-grand-$n.txt" "$($p.Id) $($p.StartTime.ToFileTimeUtc())"
}
function Write-HashLine {
  $text = [Text.Encoding]::UTF8.GetString($bytes).Replace("`r", '')
  if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }
  $lines = @($text -split "`n" | Where-Object { $_ -ne '' })
  $norm = (($lines -join "`n") + "`n")
  $sha = [Security.Cryptography.SHA256]::Create()
  $hash = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($norm))).Replace('-', '').ToLowerInvariant()
  [Console]::Out.Write($hash + '  -' + "`n")
}
if ($mode -match '^exit (\d+)$') { exit [int]$Matches[1] }
if ($mode -match '^noread-exit$') { exit 0 }
if ($mode -match 'hang') {
  if ($mode -eq 'hangtree') {
    Start-Sleeper 120
    $parent = (Get-CimInstance Win32_Process -Filter "ProcessId=$PID").ParentProcessId
    Rec "pid-parent-$n.txt" "$parent $((Get-Process -Id $parent).StartTime.ToFileTimeUtc())"
  }
  Start-Sleep -Seconds 120
  exit 0
}
if ($mode -eq 'lingering') {
  Start-Sleeper 8
  Write-HashLine
  exit 0
}
if ($mode -match '^ok(:(.*))?$') {
  Write-HashLine
  if ($Matches[2]) { [Console]::Out.Write('zone ' + $Matches[2] + "`n") }
  exit 0
}
if ($mode -eq 'zonefailed') { Write-HashLine; [Console]::Out.Write("zone-failed`n"); exit 0 }
if ($mode -eq 'noise') {
  [Console]::Out.Write("Welcome to the dummy box`n")
  Write-HashLine
  exit 0
}
exit 99
"#;

/// The stand-in's `.cmd`. In the `roothang` mode the `.cmd` itself never ends and starts no
/// process: it appends a line to `spin.txt` beside it, over and over, so a test can see whether the
/// ssh's own process is still running. It stops on its own when `stop.txt` appears beside it, or
/// after 100000 rounds, so that a test that fails leaves nothing running for long.
const STANDIN_CMD: &str = "@echo off\r\n\
set /p MODE=<\"%~dp0mode.txt\"\r\n\
if \"%MODE%\"==\"roothang\" goto spin\r\n\
\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoProfile -ExecutionPolicy Bypass -File \"%~dp0standin.ps1\" %*\r\n\
exit /b %ERRORLEVEL%\r\n\
:spin\r\n\
set /a ROUNDS+=1\r\n\
if exist \"%~dp0stop.txt\" exit /b 0\r\n\
if %ROUNDS% GEQ 100000 exit /b 0\r\n\
>>\"%~dp0spin.txt\" echo x\r\n\
goto spin\r\n";

/// A throwaway folder with a home, a `LOCALAPPDATA`, a working folder and the stand-in.
pub struct Sandbox {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
    pub home: PathBuf,
    pub lad: PathBuf,
    pub work: PathBuf,
    /// A system root with no OpenSSH in it, so the default ssh is never found.
    pub sysroot: PathBuf,
    /// The stand-in's folder, with a space in its name.
    pub standin: PathBuf,
}

impl Sandbox {
    pub fn new() -> Sandbox {
        let dir = tempfile::tempdir().expect("A throwaway folder");
        let root = dir.path().to_path_buf();
        let sb = Sandbox {
            home: root.join("home"),
            lad: root.join("home").join("lad"),
            work: root.join("work"),
            sysroot: root.join("sysroot"),
            standin: root.join("stand in"),
            root,
            _dir: dir,
        };
        for d in [&sb.home, &sb.lad, &sb.work, &sb.sysroot, &sb.standin] {
            fs::create_dir_all(d).expect("A throwaway subfolder");
        }
        fs::write(sb.standin.join("ssh.cmd"), STANDIN_CMD).expect("The stand-in");
        fs::write(sb.standin.join("standin.ps1"), STANDIN_PS1).expect("The stand-in");
        sb.mode("ok");
        sb
    }

    pub fn ssh_cmd(&self) -> PathBuf {
        self.standin.join("ssh.cmd")
    }

    /// What the stand-in does next: `ok`, `ok:<zone>`, `zonefailed`, `exit <n>`, `noise`,
    /// `noread-exit`, `noread-hang`, `hang`, `hangtree`, `lingering` or `roothang`.
    pub fn mode(&self, mode: &str) {
        fs::write(self.standin.join("mode.txt"), mode).expect("The mode");
    }

    pub fn context(&self) -> Context {
        Context {
            home: self.home.clone(),
            local_app_data: Some(self.lad.clone()),
            system_root: self.sysroot.clone(),
            nested: false,
        }
    }

    /// The setup the ssh's command gets: a throwaway working folder and home, and no part of the
    /// real ones.
    pub fn setup(&self) -> impl Fn(&mut Command) + Send + Sync + 'static {
        let (home, lad, work) = (self.home.clone(), self.lad.clone(), self.work.clone());
        move |cmd: &mut Command| {
            cmd.current_dir(&work)
                .env("USERPROFILE", &home)
                .env("HOME", &home)
                .env("LOCALAPPDATA", &lad)
                .env("APPDATA", &lad)
                .env("TEMP", &work)
                .env("TMP", &work);
        }
    }

    /// A look-up of the environment that names the stand-in, and the zone when one is given.
    pub fn lookup(&self, zone: Option<&str>) -> std::sync::Arc<agentspace_push::Lookup> {
        let ssh = self.ssh_cmd().into_os_string();
        let zone = zone.map(std::ffi::OsString::from);
        std::sync::Arc::new(move |name| match name {
            "AGENTSPACE_SSH_CMD" => Some(ssh.clone()),
            "AGENTSPACE_TIMEZONE" => zone.clone(),
            _ => None,
        })
    }

    /// How many times the stand-in has run.
    pub fn calls(&self) -> usize {
        fs::read_dir(&self.standin)
            .expect("The stand-in's folder")
            .filter_map(Result::ok)
            .filter(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.starts_with("args-") && name.ends_with(".json")
            })
            .count()
    }

    pub fn args(&self, n: usize) -> Vec<String> {
        let text = fs::read_to_string(self.standin.join(format!("args-{n}.json")))
            .expect("The recorded arguments");
        serde_json::from_str(&text).expect("The recorded arguments are JSON")
    }

    pub fn stdin(&self, n: usize) -> Vec<u8> {
        fs::read(self.standin.join(format!("stdin-{n}.bin"))).expect("The recorded stdin")
    }

    /// The stand-in's recorded environment as name and value pairs.
    pub fn env(&self, n: usize) -> Vec<(String, String)> {
        let text = fs::read_to_string(self.standin.join(format!("env-{n}.txt")))
            .expect("The recorded environment");
        text.split('\n')
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    pub fn env_text(&self, n: usize) -> String {
        fs::read_to_string(self.standin.join(format!("env-{n}.txt"))).expect("The environment")
    }

    pub fn cwd(&self, n: usize) -> PathBuf {
        PathBuf::from(
            fs::read_to_string(self.standin.join(format!("cwd-{n}.txt"))).expect("The folder"),
        )
    }

    /// A process the stand-in recorded (`ps`, `grand` or `parent`): its ID and its creation time.
    pub fn pid(&self, kind: &str, n: usize) -> Option<Proc> {
        let text = fs::read_to_string(self.standin.join(format!("pid-{kind}-{n}.txt"))).ok()?;
        let mut it = text.split_whitespace();
        let pid = it.next()?.parse().ok()?;
        let created = it.next()?.parse().ok()?;
        Some(Proc { pid, created })
    }

    // ---- the receipt and the credential ----

    pub fn credential_dir(&self) -> PathBuf {
        self.lad
            .join("localswim-v2")
            .join("credentials")
            .join("00000000-dummy")
    }

    pub fn receipt_path(&self) -> PathBuf {
        self.lad.join("localswim-v2").join("service.json")
    }

    /// Writes a receipt, and a `bot.xml` that holds `TOKEN` protected for the current user, as
    /// PowerShell 7 writes it (UTF-8, no mark).
    pub fn write_localswim(&self) {
        fs::create_dir_all(self.credential_dir()).expect("The credential folder");
        fs::write(
            self.receipt_path(),
            serde_json::json!({
                "base": BASE,
                "credential_directory": self.credential_dir().to_string_lossy(),
            })
            .to_string(),
        )
        .expect("The receipt");
        let hex = hex_lower(&protect(&utf16le(TOKEN), None));
        fs::write(self.credential_dir().join("bot.xml"), clixml(&hex)).expect("bot.xml");
    }

    pub fn stamp_path(&self) -> PathBuf {
        self.home.join(".agentspace").join("localswim-pushed.json")
    }
}

pub fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

pub fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn clixml(hex: &str) -> String {
    format!(
        "<Objs Version=\"1.1.0.1\" xmlns=\"http://schemas.microsoft.com/powershell/2004/04\">\
         <Obj RefId=\"0\"><TN RefId=\"0\"><T>System.Management.Automation.PSCredential</T>\
         <T>System.Object</T></TN><ToString>System.Management.Automation.PSCredential</ToString>\
         <Props><S N=\"UserName\">bot</S><SS N=\"Password\">{hex}</SS></Props></Obj></Objs>"
    )
}

/// `CryptProtectData` for the current user, with `CRYPTPROTECT_UI_FORBIDDEN`, and the entropy the
/// caller gives (none for a credential as localswim writes it).
pub fn protect(plain: &[u8], entropy: Option<&[u8]>) -> Vec<u8> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr().cast_mut(),
    };
    let entropy_blob = entropy.map(|e| CRYPT_INTEGER_BLOB {
        cbData: e.len() as u32,
        pbData: e.as_ptr().cast_mut(),
    });
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: the blobs point at slices that outlive the call; `output` is filled by it.
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            entropy_blob
                .as_ref()
                .map_or(std::ptr::null(), std::ptr::from_ref),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    assert!(ok != 0, "CryptProtectData failed");
    // SAFETY: on success `pbData` holds `cbData` bytes allocated for us, freed once below.
    let blob =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(output.pbData.cast()) };
    blob
}

/// Whether the process with this ID is still running.
pub fn is_running(pid: u32) -> bool {
    // SAFETY: plain calls; the handle is closed.
    unsafe {
        let handle = OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if handle.is_null() {
            return false;
        }
        let state = WaitForSingleObject(handle, 0);
        CloseHandle(handle);
        state == WAIT_TIMEOUT
    }
}

/// A process a stand-in recorded: its ID, and its creation time as a FILETIME, which together name
/// one process, whatever process the ID is given to later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub created: u64,
}

/// The creation time of an open process, as a FILETIME.
///
/// # Safety
///
/// `handle` MUST be an open process handle with at least `PROCESS_QUERY_LIMITED_INFORMATION`.
unsafe fn created_at(handle: HANDLE) -> Option<u64> {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: the caller gives an open handle; the four times are valid for the call to fill.
    let ok = unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    (ok != 0).then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

/// Whether the recorded process is still running. An ID now held by a process created at another
/// time is another process, and the recorded one has ended.
pub fn proc_running(p: Proc) -> bool {
    // SAFETY: plain calls; the handle is closed.
    unsafe {
        let handle = OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            p.pid,
        );
        if handle.is_null() {
            // A process that exists but cannot be opened is taken as running, so that a check
            // that it ended never passes on a process it could not see.
            return GetLastError() != ERROR_INVALID_PARAMETER;
        }
        let running =
            created_at(handle) == Some(p.created) && WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        CloseHandle(handle);
        running
    }
}

/// Waits until the recorded process has ended, for up to `wait`.
pub fn ended_within(p: Proc, wait: Duration) -> bool {
    let until = Instant::now() + wait;
    while Instant::now() < until {
        if !proc_running(p) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    !proc_running(p)
}

/// Ends a process that a test's own stand-in started, so a test that fails leaves none behind, and
/// only while its ID is still that process's: the check and the termination go through one
/// handle, which keeps the ID from being given to another process in between.
pub fn end_own_process(p: Proc) {
    // SAFETY: plain calls; the handle is closed.
    unsafe {
        let handle = OpenProcess(
            PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            p.pid,
        );
        if handle.is_null() {
            return;
        }
        if created_at(handle) == Some(p.created) {
            TerminateProcess(handle, 1);
        }
        CloseHandle(handle);
    }
}

/// A process the test starts itself, which is not in the ssh's tree: the ssh's tree is stopped and
/// this is not. Ended when dropped.
pub struct Bystander(pub std::process::Child);

impl Bystander {
    pub fn start(work: &Path) -> Bystander {
        let shell = Path::new(&std::env::var_os("SystemRoot").expect("SystemRoot"))
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        let child = Command::new(shell)
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 90"])
            .current_dir(work)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("A bystander");
        Bystander(child)
    }

    pub fn id(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for Bystander {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
