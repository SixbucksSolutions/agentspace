//! The implementer's test support (the spec tests are tests/spec_*.rs, with tests/common/). Every
//! folder is a throwaway one under the temp folder, every child gets a throwaway home, and every
//! process stopped here is one these tests started, checked by its creation time.

#![allow(dead_code, unsafe_code)]

use agentspace_keeper_files::{LaunchRequest, launch_request_file, serialize_launch_request};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_TIMEOUT};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};

pub fn testhost() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_agentspace-launch-testhost"))
}

/// `<System32>`, as Windows reports it.
pub fn system32() -> PathBuf {
    let mut buffer = [0u16; 1024];
    let n = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    assert!(n > 0 && n < buffer.len());
    PathBuf::from(String::from_utf16_lossy(&buffer[..n]))
}

/// A throwaway folder with a home, a state folder and a bin folder in it.
pub struct Scratch {
    pub root: PathBuf,
}

impl Scratch {
    pub fn new(tag: &str) -> Scratch {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "agentspace-launch-impl-{tag}-{}-{nanos:x}",
            std::process::id()
        ));
        for sub in ["home", "state", "bin"] {
            fs::create_dir_all(root.join(sub)).unwrap();
        }
        Scratch { root }
    }
    pub fn home(&self) -> PathBuf {
        self.root.join("home")
    }
    pub fn state(&self) -> PathBuf {
        self.root.join("state")
    }
    pub fn log(&self) -> PathBuf {
        self.state().join("board-holder-ssh.log")
    }
    pub fn log_text(&self) -> String {
        fs::read(self.log())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    }

    /// A `.cmd` stub holder: it says it started, where it runs, whether stdin is empty, whether
    /// `marker` is set and whether `USERPROFILE` is this scratch's home, on stdout and stderr,
    /// then runs the test host's stub holder for `secs` seconds. It prints no variable's value.
    pub fn stub(&self, name: &str, marker: &str, secs: u32) -> PathBuf {
        let path = self.root.join("bin").join(name);
        let text = format!(
            "@echo off\r\n\
             echo holder-started\r\n\
             echo cd=[%CD%]\r\n\
             set line=\r\n\
             set /p line=\r\n\
             if defined line (echo stdin=some) else (echo stdin=empty)\r\n\
             if defined {marker} (echo mark=set) else (echo mark=unset)\r\n\
             if /i \"%USERPROFILE%\"==\"{home}\" (echo profile=keeper) else (echo profile=logon)\r\n\
             echo err-line 1>&2\r\n\
             \"{host}\" --sleep {secs}\r\n\
             echo sleep-ended=%ERRORLEVEL%\r\n",
            home = self.home().display(),
            host = testhost().display(),
        );
        fs::write(&path, text).unwrap();
        path
    }

    /// Writes a launch request for `<System32>\cmd.exe` running `stub`, whose deadline is
    /// `seconds` from now, and gives its path.
    pub fn request(&self, hex: &str, stub: &Path, alias: &str, seconds: i64) -> PathBuf {
        let result = self.state().join(agentspace_keeper_files::result_file(hex));
        let cmd = system32().join("cmd.exe");
        let request = LaunchRequest {
            application: cmd.display().to_string(),
            command_line: format!(
                "\"{}\" /d /c \"\"{}\" -N -o ExitOnForwardFailure=yes {alias}\"",
                cmd.display(),
                stub.display()
            ),
            log: self.log().display().to_string(),
            directory: self.home().display().to_string(),
            not_after: deadline(seconds),
            result: result.display().to_string(),
        };
        let path = self.state().join(launch_request_file(hex));
        fs::write(&path, serialize_launch_request(&request).unwrap()).unwrap();
        path
    }

    /// A child with this scratch's home as its home and its working folder.
    pub fn command(&self, exe: &Path) -> Command {
        let mut c = Command::new(exe);
        c.current_dir(self.home())
            .env("USERPROFILE", self.home())
            .env("HOME", self.home())
            .env("LOCALAPPDATA", self.home())
            .env("APPDATA", self.home())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        c
    }

    /// Runs the test host as the launcher of `request`, and gives its exit code.
    pub fn launcher(&self, request: &Path) -> Child {
        self.command(&testhost())
            .arg("-Launch")
            .arg(request)
            .spawn()
            .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for _ in 0..20 {
            if fs::remove_dir_all(&self.root).is_ok() || !self.root.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

/// `YYYY-MM-DDTHH:MM:SS.fffffffZ`, `seconds` from now.
pub fn deadline(seconds: i64) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let ticks = (now.as_nanos() / 100) as i64 + seconds * 10_000_000;
    let secs = ticks.div_euclid(10_000_000);
    let (y, m, d) = civil(secs.div_euclid(86_400));
    let rest = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:07}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        ticks.rem_euclid(10_000_000)
    )
}

fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// A process's creation time as `YYYY-MM-DDTHH:MM:SS.fffZ`.
fn created(h: HANDLE) -> Option<String> {
    let mut t = [FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    }; 4];
    let [a, b, c, d] = &mut t;
    if unsafe { GetProcessTimes(h, a, b, c, d) } == 0 {
        return None;
    }
    let ticks = (i64::from(t[0].dwHighDateTime) << 32) | i64::from(t[0].dwLowDateTime);
    let ms = (ticks - 116_444_736_000_000_000).div_euclid(10_000);
    let secs = ms.div_euclid(1000);
    let (y, m, dd) = civil(secs.div_euclid(86_400));
    let rest = secs.rem_euclid(86_400);
    Some(format!(
        "{y:04}-{m:02}-{dd:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        ms.rem_euclid(1000)
    ))
}

/// A handle to one process this test started, checked by its creation time, and kept open so its
/// PID is never reused while the test looks at it.
pub struct Proc {
    pub pid: u32,
    pub handle: HANDLE,
}

impl Proc {
    /// Opens the process `pid` if it runs and was created at `started_at`.
    pub fn open(pid: u32, started_at: &str) -> Option<Proc> {
        let h = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
                0,
                pid,
            )
        };
        if h.is_null() {
            return None;
        }
        let p = Proc { pid, handle: h };
        (created(h).as_deref() == Some(started_at)).then_some(p)
    }

    pub fn running(&self) -> bool {
        unsafe { WaitForSingleObject(self.handle, 0) == WAIT_TIMEOUT }
    }

    /// Stops this process and every process it started (its children by PID, created after it),
    /// and waits for it to end.
    pub fn stop_tree(&self) {
        let mine = created(self.handle);
        for child in children(self.pid) {
            let h = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                    0,
                    child,
                )
            };
            if h.is_null() {
                continue;
            }
            if created(h) >= mine {
                unsafe { TerminateProcess(h, 1) };
            }
            unsafe { CloseHandle(h) };
        }
        unsafe {
            TerminateProcess(self.handle, 1);
            WaitForSingleObject(self.handle, 5000);
        }
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

/// The PIDs whose parent PID is `pid`.
fn children(pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() || snap as isize == -1 {
            return out;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e);
        while ok != 0 {
            if e.th32ParentProcessID == pid {
                out.push(e.th32ProcessID);
            }
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
    }
    out
}

/// Waits up to `timeout` for `f` to give a value, looking every 50 ms.
pub fn wait_for<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let began = Instant::now();
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if began.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Waits for a child this test started, up to `timeout`, and gives its exit code.
pub fn wait_child(child: &mut Child, timeout: Duration) -> Option<i32> {
    wait_for(timeout, || {
        child.try_wait().unwrap().map(|s| s.code().unwrap_or(-1))
    })
}
