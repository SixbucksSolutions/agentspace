//! The guard's stand-ins and the keeper's lines for the guard (docs/keeper.md, Starting the guard;
//! Conformance cases for the guard's start).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. **Every check that runs a
//! keeper MUST name a stand-in guard, `AGENTSPACE_GUARD_CMD`, and SHOULD name tripwires for
//! `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD` and `AGENTSPACE_SCP_CMD`** (AGENTS.md; the
//! architect's ruling P15). Unset, a keeper started by a test would start the build's real
//! operator as a guard, which would see every host as unexplained under a throwaway home, and
//! could stop a real host. Every world of these tests gets them from `guard_vars`, in its
//! throwaway home, and none sets a variable of the test process.
//!
//! The stand-in is a `.cmd`, as keeper.md's conformance cases make one: it appends one block per
//! run to a file beside it. A test reads the block and compares the variables it names, and
//! reports a mismatch by the variable's name, never by its value.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// The system folder, `SystemRoot`: the real one, since a `.cmd` stand-in is run through
/// `<system folder>\cmd.exe` (keeper.md, How the keeper starts it).
pub fn system_root() -> PathBuf {
    std::env::var_os("SystemRoot")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

/// A canonical path without the `\\?\` prefix `canonicalize` gives on Windows. The guard's
/// working folder is the keeper's `USERPROFILE`, and a detached start cannot use a verbatim path
/// as a folder.
pub fn plain(p: PathBuf) -> PathBuf {
    let text = p.to_string_lossy().into_owned();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => p,
    }
}

/// The name of the stand-in guard in a throwaway home.
pub const STANDIN: &str = "guard-standin.cmd";
/// The file the stand-in appends its runs to, beside itself.
pub const STANDIN_RUNS: &str = "guard-runs.txt";
/// A file beside the stand-in whose presence, when it starts, makes it stay alive: it appends a
/// line to `guard-ticks.txt` every second, for at most about 100 seconds, so that a test can see
/// the process outlive its keeper. The test ends it by making `guard-stop`, which `StayingGuard`
/// does when it is dropped, a failed test included.
pub const STANDIN_STAY: &str = "guard-stay";
pub const STANDIN_STOP: &str = "guard-stop";
pub const STANDIN_TICKS: &str = "guard-ticks.txt";

/// The number of ticks a staying stand-in in `dir` has made.
pub fn ticks_in(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join(STANDIN_TICKS))
        .map(|t| t.lines().count())
        .unwrap_or(0)
}

/// Makes the stand-ins of `dir` that start from now on stay alive, and ends them all when it is
/// dropped.
pub struct StayingGuard {
    dir: PathBuf,
}

impl StayingGuard {
    pub fn new(dir: &Path) -> StayingGuard {
        let _ = std::fs::remove_file(dir.join(STANDIN_STOP));
        std::fs::write(dir.join(STANDIN_STAY), b"").unwrap();
        StayingGuard {
            dir: dir.to_path_buf(),
        }
    }

    /// The stand-ins that start after this call do not stay.
    pub fn no_more_stays(&self) {
        let _ = std::fs::remove_file(self.dir.join(STANDIN_STAY));
    }

    /// Ends the staying stand-ins and waits, up to 15 seconds, until their ticks stop.
    pub fn end(&self) {
        std::fs::write(self.dir.join(STANDIN_STOP), b"").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut last = ticks_in(&self.dir);
        loop {
            std::thread::sleep(Duration::from_millis(2500));
            let now = ticks_in(&self.dir);
            if now == last || std::time::Instant::now() > deadline {
                return;
            }
            last = now;
        }
    }
}

impl Drop for StayingGuard {
    fn drop(&mut self) {
        let _ = std::fs::write(self.dir.join(STANDIN_STOP), b"");
    }
}
pub const TRIPWIRE: &str = "tripwire.cmd";
pub const TRIPWIRE_HITS: &str = "tripwire-hits.txt";

/// The stand-in guard's script. Per run it appends one block to `guard-runs.txt` beside it: the
/// first two arguments, the working folder, `USERPROFILE`, every variable whose name starts
/// `AGENTSPACE_`, and whether `<USERPROFILE>\.agentspace\guard.lock` could be opened for append,
/// so that a keeper which still holds its own handle on it is seen. It then exits 0, or stays
/// alive (`STANDIN_STAY`). It never touches `guard.log`, which only a guard writes.
const STANDIN_SCRIPT: &str = "@echo off\r\n\
(\r\n\
echo run\r\n\
echo arg1=[%~1]\r\n\
echo arg2=[%~2]\r\n\
echo cwd=[%CD%]\r\n\
echo USERPROFILE=%USERPROFILE%\r\n\
set AGENTSPACE_\r\n\
if exist \"%USERPROFILE%\\.agentspace\\guard.lock\" (type nul >> \"%USERPROFILE%\\.agentspace\\guard.lock\" && echo lockprobe=ok|| echo lockprobe=fail) else (echo lockprobe=none)\r\n\
echo end\r\n\
) >> \"%~dp0guard-runs.txt\" 2>nul\r\n\
if not exist \"%~dp0guard-stay\" exit /b 0\r\n\
set /a n=0\r\n\
:wait\r\n\
if exist \"%~dp0guard-stop\" exit /b 0\r\n\
>> \"%~dp0guard-ticks.txt\" echo tick\r\n\
ping -n 2 127.0.0.1 >nul\r\n\
set /a n+=1\r\n\
if %n% lss 100 goto wait\r\n\
exit /b 0\r\n";

const TRIPWIRE_SCRIPT: &str = "@echo off\r\necho hit>> \"%~dp0tripwire-hits.txt\"\r\nexit /b 1\r\n";

/// Writes the stand-in guard into `dir` as `name`, and returns its full path.
pub fn write_standin(dir: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, STANDIN_SCRIPT).unwrap();
    p
}

/// Makes the stand-in guard and the tripwire in `home`, and returns the variables that name them:
/// `AGENTSPACE_GUARD_CMD`, then `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD` and
/// `AGENTSPACE_SCP_CMD`.
pub fn guard_vars(home: &Path) -> Vec<(&'static str, PathBuf)> {
    write_standin(home, STANDIN);
    std::fs::write(home.join(TRIPWIRE), TRIPWIRE_SCRIPT).unwrap();
    guard_var_paths(home)
}

/// The variables of `guard_vars`, without writing anything.
pub fn guard_var_paths(home: &Path) -> Vec<(&'static str, PathBuf)> {
    let trip = home.join(TRIPWIRE);
    vec![
        ("AGENTSPACE_GUARD_CMD", home.join(STANDIN)),
        ("AGENTSPACE_AWS_CMD", trip.clone()),
        ("AGENTSPACE_SSH_CMD", trip.clone()),
        ("AGENTSPACE_SCP_CMD", trip),
    ]
}

/// Whether a tripwire in `home` ran.
pub fn tripwire_hit(home: &Path) -> bool {
    home.join(TRIPWIRE_HITS).exists()
}

/// One run of the stand-in guard, as it recorded itself.
#[derive(Debug, Clone)]
pub struct GuardRun {
    lines: Vec<String>,
}

impl GuardRun {
    /// The text after `key=` of the first line that starts so, the key compared without regard to
    /// case, as Windows compares a variable's name.
    pub fn get(&self, key: &str) -> Option<String> {
        let prefix = format!("{}=", key.to_ascii_lowercase());
        self.lines
            .iter()
            .find(|l| l.to_ascii_lowercase().starts_with(&prefix))
            .map(|l| l[prefix.len()..].to_string())
    }

    /// A bracketed value, `key=[value]`, without its brackets.
    pub fn bracket(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|v| {
            v.strip_prefix('[')
                .and_then(|v| v.strip_suffix(']'))
                .map(str::to_string)
        })
    }

    /// Whether a variable of this name was in the stand-in's environment, by name alone.
    pub fn has_var(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Asserts that the variable `name` has exactly `want`, and reports a mismatch by the name
    /// alone. Names compare without regard to case, values exactly.
    pub fn assert_var(&self, name: &str, want: &str) {
        match self.get(name) {
            None => panic!("The guard did not get {name}"),
            Some(v) => assert!(v == want, "The guard got {name} with another value"),
        }
    }

    /// As `assert_var`, for a path: its text compares without regard to case.
    pub fn assert_path_var(&self, name: &str, want: &Path) {
        match self.get(name) {
            None => panic!("The guard did not get {name}"),
            Some(v) => assert!(
                v.eq_ignore_ascii_case(&want.to_string_lossy()),
                "The guard got {name} with another value"
            ),
        }
    }

    pub fn assert_absent(&self, name: &str) {
        assert!(
            !self.has_var(name),
            "The guard got {name}, which no guard is given"
        );
    }
}

/// The runs a stand-in made, in `dir`, whole ones only.
pub fn guard_runs_in(dir: &Path) -> Vec<GuardRun> {
    let Ok(bytes) = std::fs::read(dir.join(STANDIN_RUNS)) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let mut runs = Vec::new();
    let mut current: Option<Vec<String>> = None;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line == "run" {
            current = Some(Vec::new());
        } else if line == "end" {
            if let Some(lines) = current.take() {
                runs.push(GuardRun { lines });
            }
        } else if let Some(lines) = current.as_mut() {
            lines.push(line.to_string());
        }
    }
    runs
}

/// Waits until the stand-in in `dir` has made at least `n` runs, up to 30 seconds, and gives them.
/// A detached start returns before the program has run.
pub fn wait_for_runs(dir: &Path, n: usize) -> Vec<GuardRun> {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let runs = guard_runs_in(dir);
        if runs.len() >= n {
            return runs;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "The stand-in guard made {} runs, and {n} were expected",
            runs.len()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Waits a moment, then asserts that the stand-in in `dir` made exactly `n` runs: a start that
/// should not have happened has had its time to.
pub fn settle_runs(dir: &Path, n: usize) {
    std::thread::sleep(Duration::from_millis(900));
    assert_eq!(
        guard_runs_in(dir).len(),
        n,
        "The stand-in guard made another number of runs"
    );
}

// ---- The keeper's lines for the guard ----

/// Whether a log line is one of the keeper's lines for the guard's start.
pub fn is_guard_line(line: &str) -> bool {
    line.contains(" guard started pid=")
        || line.contains(" failed lease=- alias=- pid=- reason=The guard could not be started: ")
}

/// `text` without the guard's lines, each of the others kept with its line feed. Cases that
/// existed before v0.08 ignore the guard's lines where they read a keeper's log line for line
/// (keeper.md, Conformance cases for the guard's start: H1).
pub fn without_guard_lines(text: &str) -> String {
    text.split_inclusive('\n')
        .filter(|l| !is_guard_line(l))
        .collect()
}

/// The guard's lines of `text`, each with its line feed.
pub fn guard_lines(text: &str) -> Vec<String> {
    text.split_inclusive('\n')
        .filter(|l| is_guard_line(l))
        .map(str::to_string)
        .collect()
}

/// The `guard started` line for a time and a process ID.
pub fn guard_started_line(time: &str, pid: u32) -> String {
    format!("{time} guard started pid={pid}\n")
}

/// The `failed` line of a start that failed, for `why` (keeper.md, The keeper's lines for it).
pub fn guard_failed_line(time: &str, why: &str) -> String {
    format!("{time} failed lease=- alias=- pid=- reason=The guard could not be started: {why}\n")
}

/// The time and the process ID of a `guard started` line, with every other byte of it checked:
/// `<time> guard started pid=<digits>\n`, with no `host=` field.
pub fn parse_guard_started(line: &str) -> (String, u32) {
    let (time, pid) = parse_guard_started_any(line);
    let pid = pid.unwrap_or_else(|| {
        panic!(
            "The guard started line names no process ID (pid=-): keeper.md gives its decimal \
             digits, and the detached start gives none yet: {line:?}"
        )
    });
    (time, pid)
}

/// As `parse_guard_started`, with a process ID that may be `-`, for a test whose subject is not
/// the process ID: every other byte of the line is checked.
pub fn parse_guard_started_any(line: &str) -> (String, Option<u32>) {
    let (time, rest) = line
        .split_once(" guard started pid=")
        .unwrap_or_else(|| panic!("Not a guard started line: {line:?}"));
    let digits = rest
        .strip_suffix('\n')
        .unwrap_or_else(|| panic!("The line has no line feed: {line:?}"));
    assert_eq!(
        time.len(),
        20,
        "The time is not YYYY-MM-DDTHH:MM:SSZ: {line:?}"
    );
    if digits == "-" {
        return (time.to_string(), None);
    }
    assert!(
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
        "The process ID is not decimal digits: {line:?}"
    );
    let pid: u32 = digits.parse().expect("The process ID fits a u32");
    assert_eq!(line, guard_started_line(time, pid), "The line's bytes");
    (time.to_string(), Some(pid))
}

/// The time of a `guard started` line, whatever its process ID (`parse_guard_started_any`).
pub fn guard_started_time(line: &str) -> String {
    parse_guard_started_any(line).0
}

/// The time and `<why>` of a `failed` line of a start that failed, with the line's every other
/// byte checked.
pub fn parse_guard_failed(line: &str) -> (String, String) {
    let marker = " failed lease=- alias=- pid=- reason=The guard could not be started: ";
    let (time, rest) = line
        .split_once(marker)
        .unwrap_or_else(|| panic!("Not a guard failed line: {line:?}"));
    let why = rest
        .strip_suffix('\n')
        .unwrap_or_else(|| panic!("The line has no line feed: {line:?}"));
    assert_eq!(
        time.len(),
        20,
        "The time is not YYYY-MM-DDTHH:MM:SSZ: {line:?}"
    );
    assert_eq!(line, guard_failed_line(time, why), "The line's bytes");
    (time.to_string(), why.to_string())
}

/// A UTC time `YYYY-MM-DDTHH:MM:SSZ` as seconds.
pub fn secs_of(time: &str) -> i64 {
    let n = |a: usize, b: usize| time[a..b].parse::<i64>().expect("A number in a time");
    let (y, mo, d) = (n(0, 4), n(5, 7), n(8, 10));
    // Proleptic Gregorian, civil from days.
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    ((days * 24 + n(11, 13)) * 60 + n(14, 16)) * 60 + n(17, 19)
}

// ---- Processes the stand-in leaves running ----

/// The command line of the process `pid`, or `None` when there is none.
pub fn process_command_line(pid: u32) -> Option<String> {
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("(Get-CimInstance Win32_Process -Filter 'ProcessId={pid}').CommandLine"),
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Whether the process `pid` exists.
pub fn process_exists(pid: u32) -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")),
        Err(_) => false,
    }
}

/// Ends a process and its tree. A test calls it only for a stand-in guard it made, checked by its
/// command line (`StandInProcess`).
fn end_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// A stand-in guard process that a keeper started and a test has to end. It is checked to be the
/// stand-in (its command line names `needle`) when it is made, and ended when it is dropped, a
/// failed test included.
pub struct StandInProcess {
    pub pid: u32,
}

impl StandInProcess {
    pub fn new(pid: u32, needle: &str) -> StandInProcess {
        let line = process_command_line(pid).unwrap_or_else(|| {
            panic!("No process {pid} runs, and the guard started line named it")
        });
        assert!(
            line.to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase()),
            "The process the guard started line names is not the stand-in guard"
        );
        StandInProcess { pid }
    }

    /// Ends the process and waits, up to 10 seconds, for it to be gone.
    pub fn end(&self) {
        end_tree(self.pid);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while process_exists(self.pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for StandInProcess {
    fn drop(&mut self) {
        end_tree(self.pid);
    }
}
