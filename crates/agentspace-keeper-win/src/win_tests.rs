//! Tests against real Windows processes and files.
//!
//! Every process a test starts is its own: a `.cmd` stub run by `<System32>\cmd.exe` the way a
//! holder for a `.cmd` `-SshExe` is run, whose tree is cmd, cmd and `PING.EXE` (which sleeps by
//! pinging the loopback address once a second), or this test program run again as a helper that
//! holds a lock. Each is put in a job object that ends it if the test fails, is given a throwaway
//! home and working folder under the temp folder, and opens no window. Lock files live only in
//! throwaway folders under the temp folder.

use crate::process::{Os, query as core_query};
use crate::sys::{KillOnDrop, Windows, is_inheritable};
use crate::time::filetime_text;
use crate::{WinLocks, WinProcesses, system_folder};
use agentspace_keeper_api::{HolderRecord, Locks, Matcher, Processes, Query, Stopped, Taken};
use agentspace_keeper_files::{HolderMatcher, KEEPER_LOCK_FILE, PASS_LOCK_FILE};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ALIAS: &str = "keeper-win-test";
const HELPER_MODE: &str = "AGENTSPACE_KEEPER_WIN_TEST_HELPER";
const HELPER_DIR: &str = "AGENTSPACE_KEEPER_WIN_TEST_DIR";

/// A throwaway folder with a `home` inside it, for a child's profile and working folder.
fn scratch() -> (TempDir, PathBuf) {
    let dir = tempfile::Builder::new()
        .prefix("keeper-win-")
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    (dir, home)
}

/// Points the child's profile folders at the throwaway home, and runs it there with no window.
fn sandboxed(cmd: &mut Command, home: &Path) {
    cmd.env("USERPROFILE", home)
        .env("HOME", home)
        .env("LOCALAPPDATA", home)
        .env("APPDATA", home)
        .current_dir(home)
        .creation_flags(CREATE_NO_WINDOW);
}

fn utc_text(t: SystemTime) -> String {
    let d = t.duration_since(UNIX_EPOCH).unwrap();
    let ticks = (d.as_secs() + 11_644_473_600) * 10_000_000 + u64::from(d.subsec_nanos() / 100);
    filetime_text(ticks)
}

/// A holder this test started: `"<System32>\cmd.exe" /d /c ""<stub>" -N -o
/// ExitOnForwardFailure=yes <alias>"`, the keeper's own form for a `.cmd` `-SshExe`.
struct Holder {
    child: Child,
    ssh: PathBuf,
    matcher: HolderMatcher,
    _job: KillOnDrop,
}

impl Holder {
    fn start(dir: &Path, home: &Path, name: &str) -> Holder {
        let system = system_folder().unwrap();
        let matcher = HolderMatcher::new(&system);
        let child_cmd = dir.join(format!("{name}-child.cmd"));
        let ssh = dir.join(format!("{name} ssh.cmd"));
        // A second holder of the same name runs the same stub, with the same command line.
        let first = !ssh.exists();
        if first {
            fs::write(
                &child_cmd,
                format!(
                    "@\"{}\" -n 120 127.0.0.1 >nul\r\n",
                    system.join("PING.EXE").display()
                ),
            )
            .unwrap();
            fs::write(
                &ssh,
                format!(
                    "@echo off\r\n\"{}\" /d /c \"\"{}\"\"\r\n",
                    matcher.cmd_exe().display(),
                    child_cmd.display()
                ),
            )
            .unwrap();
        }
        let job = KillOnDrop::new();
        let mut cmd = Command::new(matcher.cmd_exe());
        cmd.raw_arg(format!(
            "/d /c \"\"{}\" -N -o ExitOnForwardFailure=yes {ALIAS}\"",
            ssh.display()
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        sandboxed(&mut cmd, home);
        let child = cmd.spawn().unwrap();
        job.add(&child);
        Holder {
            child,
            ssh,
            matcher,
            _job: job,
        }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn record(&self) -> HolderRecord {
        let Query::Running(info) = WinProcesses.query(self.pid()) else {
            panic!("The holder is not running");
        };
        HolderRecord {
            pid: self.pid(),
            process_started_at: info.process_started_at,
            lease_id: "lease-test".to_string(),
            alias: ALIAS.to_string(),
        }
    }

    /// Waits until the holder's tree is three deep (cmd, cmd, ping), and gives its descendants'
    /// PIDs with their creation times.
    fn wait_for_tree(&self) -> Vec<(u32, String)> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let found = descendants(self.pid());
            if found.iter().any(|&(_, depth)| depth >= 2) {
                return found
                    .into_iter()
                    .filter_map(|(pid, _)| match WinProcesses.query(pid) {
                        Query::Running(info) => Some((pid, info.process_started_at)),
                        _ => None,
                    })
                    .collect();
            }
            assert!(Instant::now() < deadline, "The stub's tree never grew");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn stop(&self) -> Stopped {
        WinProcesses.stop(&self.record(), &self.ssh, &self.matcher)
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        // Its own process only; the job ends the rest of its tree.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The descendants of `root` by parent PID alone, with their depth, for the tests' own checks.
fn descendants(root: u32) -> Vec<(u32, usize)> {
    let list = Windows.processes().unwrap();
    let mut out: Vec<(u32, usize)> = Vec::new();
    let mut frontier = vec![(root, 0usize)];
    while let Some((pid, depth)) = frontier.pop() {
        for e in list.iter().filter(|e| e.parent == pid && e.pid != root) {
            if !out.iter().any(|&(p, _)| p == e.pid) {
                out.push((e.pid, depth + 1));
                frontier.push((e.pid, depth + 1));
            }
        }
    }
    out
}

/// Whether the process that ran at `pid` and was created at `at` still runs.
fn still_runs(pid: u32, at: &str) -> bool {
    matches!(WinProcesses.query(pid), Query::Running(info) if info.process_started_at == at)
}

// Query.

#[test]
fn the_query_rights_are_limited_information_and_wait_only() {
    assert_eq!(crate::sys::QUERY_ACCESS, 0x1000 | 0x0010_0000);
    assert_eq!(crate::sys::STOP_ACCESS, 0x1000 | 0x0010_0000 | 0x1);
}

#[test]
fn a_query_of_this_process_reads_its_time_and_command_line() {
    let Query::Running(info) = WinProcesses.query(std::process::id()) else {
        panic!("This process is not running");
    };
    let at = info.process_started_at;
    assert_eq!(at.len(), 24, "{at}");
    assert!(at.ends_with('Z') && at.as_bytes()[19] == b'.', "{at}");
    assert!(at <= utc_text(SystemTime::now()), "{at}");
    let cl = info.command_line.expect("No command line");
    let exe = std::env::current_exe().unwrap();
    let name = exe.file_stem().unwrap().to_string_lossy().into_owned();
    assert!(
        cl.contains(&name),
        "The command line does not name this program"
    );
}

#[test]
fn a_query_of_a_holder_reads_its_creation_time_and_holder_command() {
    let (dir, home) = scratch();
    let before = SystemTime::now() - Duration::from_secs(1);
    let holder = Holder::start(dir.path(), &home, "query");
    let after = SystemTime::now() + Duration::from_secs(1);
    let Query::Running(info) = WinProcesses.query(holder.pid()) else {
        panic!("The holder is not running");
    };
    let at = &info.process_started_at;
    assert!(
        utc_text(before) <= *at && *at <= utc_text(after),
        "{at} is not when it started"
    );
    let cl = info.command_line.expect("No command line");
    assert!(holder.matcher.is_holder(&cl, &holder.ssh, ALIAS));
    assert!(!holder.matcher.is_holder(&cl, &holder.ssh, "another-alias"));
}

#[test]
fn a_query_of_an_ended_process_is_gone() {
    let (dir, home) = scratch();
    let mut holder = Holder::start(dir.path(), &home, "ended");
    holder.child.kill().unwrap();
    holder.child.wait().unwrap();
    // `child` still holds its handle, so the PID names the ended process, not a new one.
    assert_eq!(WinProcesses.query(holder.pid()), Query::Gone);
}

#[test]
fn a_query_of_no_process_is_gone() {
    // PIDs are multiples of 4, so 3 names no process.
    assert_eq!(core_query(&Windows, 3), Query::Gone);
}

#[test]
fn a_pid_that_is_not_a_process_id_never_names_the_process_beside_it() {
    // PIDs are multiples of 4, and Windows opens the process below a number that is not one. A
    // record with such a PID names no process, and MUST NOT be read as this one.
    let own = std::process::id();
    for off in 1..4 {
        assert_eq!(core_query(&Windows, own + off), Query::Gone, "+{off}");
        assert!(Windows.open(own + off, true).is_err(), "+{off}");
    }
}

#[test]
fn a_handle_opened_to_query_cannot_terminate_and_neither_handle_is_inheritable() {
    let (dir, home) = scratch();
    let helper = Helper::start("sleep", dir.path(), &home);
    assert_eq!(helper.said, "READY");
    let query = Windows.open(helper.pid(), false).unwrap();
    assert!(!is_inheritable(&query));
    // Windows error 5, access is denied: the handle carries no right to terminate.
    assert_eq!(Windows.terminate(&query), Err(5));
    assert_eq!(Windows.exited(&query), Ok(false));
    let stop = Windows.open(helper.pid(), true).unwrap();
    assert!(!is_inheritable(&stop));
    drop((query, stop));
    helper.finish();
}

#[test]
fn a_command_line_longer_than_the_first_buffer_is_read_whole() {
    // The first read is given 512 bytes. These lines need more, up to nearly the longest command
    // line Windows starts a process with (32,767 characters).
    let (dir, home) = scratch();
    for len in [300usize, 5_000, 32_000] {
        let last = "x".repeat(len);
        let helper = Helper::start_with("sleep", dir.path(), &home, Some(&last));
        assert_eq!(helper.said, "READY");
        let Query::Running(info) = WinProcesses.query(helper.pid()) else {
            panic!("The helper is not running");
        };
        let cl = info.command_line.expect("No command line");
        assert!(
            cl.len() > len && cl.ends_with(&last) && cl.contains("helper_process"),
            "A command line of {len} characters and more was read as {} characters",
            cl.len()
        );
        helper.finish();
    }
}

// The stop.

/// The tree under `root` by keeper.md's own rule (Stopping a holder), with each process's creation
/// time and depth: a process listed under a parent of the tree belongs to it only when, on its own
/// handle, it still runs, gives that parent's PID, and was created no earlier than that parent.
/// Parent PID alone is not enough on a busy laptop: a process whose parent died lists that PID
/// still, and once Windows gives the PID to a holder, the process looks like the holder's child
/// (#0268). Every handle is held until the walk ends, so no PID in the tree is reused during it.
fn tree_by_the_stops_rule(root: u32) -> Vec<(u32, String, usize)> {
    let Ok(root_handle) = Windows.open(root, false) else {
        return Vec::new();
    };
    let Ok(root_created) = Windows.created(&root_handle) else {
        return Vec::new();
    };
    let list = Windows.processes().unwrap();
    let mut held = vec![root_handle];
    let mut found: Vec<(u32, u64, usize)> = Vec::new();
    let mut frontier = vec![(root, root_created, 0usize)];
    while let Some((parent, parent_created, depth)) = frontier.pop() {
        for e in list.iter().filter(|e| e.parent == parent && e.pid != root) {
            if found.iter().any(|&(pid, _, _)| pid == e.pid) {
                continue;
            }
            let Ok(h) = Windows.open(e.pid, false) else {
                continue;
            };
            let Ok(created) = Windows.created(&h) else {
                continue;
            };
            if Windows.exited(&h) == Ok(false)
                && Windows.parent_pid(&h) == Ok(parent)
                && created >= parent_created
            {
                found.push((e.pid, created, depth + 1));
                frontier.push((e.pid, created, depth + 1));
                held.push(h);
            }
        }
    }
    drop(held);
    found
        .into_iter()
        .map(|(pid, created, depth)| (pid, filetime_text(created), depth))
        .collect()
}

/// Waits, up to 30 seconds, until the tree under `root` by the stop's rule is three deep (cmd,
/// cmd, ping), and gives its processes' PIDs with their creation times.
fn wait_for_tree_by_the_stops_rule(root: u32) -> Vec<(u32, String)> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let tree = tree_by_the_stops_rule(root);
        if tree.iter().any(|&(_, _, depth)| depth >= 2) {
            return tree.into_iter().map(|(pid, at, _)| (pid, at)).collect();
        }
        assert!(Instant::now() < deadline, "The stub's tree never grew");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether the process that ran at `pid` and was created at `at` still runs at `deadline`, asked
/// every 20 ms until it has ended or the deadline has passed. A stop waits a bounded time for each
/// process and then returns, so it does not promise that every process of the tree has ended by
/// then (keeper.md, Stopping a holder).
fn still_runs_at_the_deadline(deadline: Instant, pid: u32, at: &str) -> bool {
    loop {
        if !still_runs(pid, at) {
            return false;
        }
        if Instant::now() >= deadline {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The trees checked are found by keeper.md's rule for a holder's tree, never by parent PID alone,
/// which under load can take in a process that only lists a reused PID as its parent (#0268). The
/// stopped tree is checked by a poll of up to 30 seconds in all, since a stop's return does not
/// promise that every process of it has ended (keeper.md, Stopping a holder).
#[test]
fn a_stop_ends_the_whole_tree_and_leaves_another_holder_with_the_same_command() {
    let (dir, home) = scratch();
    let holder = Holder::start(dir.path(), &home, "tree");
    // Another holder with the very same command line, which is not in the first one's tree.
    let other = Holder::start(dir.path(), &home, "tree");
    let tree = wait_for_tree_by_the_stops_rule(holder.pid());
    let other_tree = wait_for_tree_by_the_stops_rule(other.pid());
    let record = holder.record();
    let other_record = other.record();

    assert_eq!(holder.stop(), Stopped::All);

    let ended_by = Instant::now() + Duration::from_secs(30);
    assert!(!still_runs_at_the_deadline(
        ended_by,
        record.pid,
        &record.process_started_at
    ));
    for (pid, at) in &tree {
        assert!(
            !still_runs_at_the_deadline(ended_by, *pid, at),
            "A process of its tree still runs"
        );
    }
    assert!(still_runs(
        other_record.pid,
        &other_record.process_started_at
    ));
    for (pid, at) in &other_tree {
        assert!(
            still_runs(*pid, at),
            "A process of another tree was stopped"
        );
    }
    assert_eq!(other.stop(), Stopped::All);
}

#[test]
fn a_record_for_another_alias_or_ssh_is_gone_and_the_process_left_running() {
    let (dir, home) = scratch();
    let holder = Holder::start(dir.path(), &home, "other");
    let tree = holder.wait_for_tree();
    let record = holder.record();

    let other_alias = HolderRecord {
        alias: "another-alias".to_string(),
        ..record.clone()
    };
    let got = WinProcesses.stop(&other_alias, &holder.ssh, &holder.matcher);
    assert_eq!(got, Stopped::Gone);
    let other_ssh = dir.path().join("another ssh.cmd");
    let got = WinProcesses.stop(&record, &other_ssh, &holder.matcher);
    assert_eq!(got, Stopped::Gone);

    assert!(still_runs(record.pid, &record.process_started_at));
    for (pid, at) in &tree {
        assert!(still_runs(*pid, at), "A process of its tree was stopped");
    }
    assert_eq!(holder.stop(), Stopped::All);
}

#[test]
fn the_pid_reuse_guard_another_creation_time_is_gone_and_left_running() {
    let (dir, home) = scratch();
    let holder = Holder::start(dir.path(), &home, "reuse");
    let record = holder.record();
    let h = Windows.open(holder.pid(), false).unwrap();
    let ticks = Windows.created(&h).unwrap();
    drop(h);
    for other in [ticks + 10_000, ticks - 10_000] {
        let reused = HolderRecord {
            process_started_at: filetime_text(other),
            ..record.clone()
        };
        assert_ne!(reused.process_started_at, record.process_started_at);
        let got = WinProcesses.stop(&reused, &holder.ssh, &holder.matcher);
        assert_eq!(got, Stopped::Gone);
        assert!(still_runs(record.pid, &record.process_started_at));
    }
    assert_eq!(holder.stop(), Stopped::All);
}

#[test]
fn a_record_whose_pid_is_beside_the_holders_is_gone_and_the_holder_left_running() {
    // Windows opens the holder for its PID plus 1, 2 or 3. Such a record names no process: the
    // stop MUST find nothing, never end the holder under a PID its tree is not listed by.
    let (dir, home) = scratch();
    let holder = Holder::start(dir.path(), &home, "beside");
    let tree = holder.wait_for_tree();
    let record = holder.record();
    for off in 1..4 {
        let beside = HolderRecord {
            pid: record.pid + off,
            ..record.clone()
        };
        let got = WinProcesses.stop(&beside, &holder.ssh, &holder.matcher);
        assert_eq!(got, Stopped::Gone, "+{off}");
        assert!(still_runs(record.pid, &record.process_started_at));
    }
    for (pid, at) in &tree {
        assert!(still_runs(*pid, at), "A process of its tree was stopped");
    }
    assert_eq!(holder.stop(), Stopped::All);
}

#[test]
fn a_stop_of_a_holder_already_ended_is_gone() {
    let (dir, home) = scratch();
    let mut holder = Holder::start(dir.path(), &home, "gone");
    let record = holder.record();
    holder.child.kill().unwrap();
    holder.child.wait().unwrap();
    let got = WinProcesses.stop(&record, &holder.ssh, &holder.matcher);
    assert_eq!(got, Stopped::Gone);
}

// The locks.

/// This test program run again as a helper that takes a lock in `dir`, prints what it got, and
/// holds it until its stdin closes.
struct Helper {
    child: Child,
    stdin: Option<ChildStdin>,
    said: String,
    _job: KillOnDrop,
}

impl Helper {
    fn start(mode: &str, dir: &Path, home: &Path) -> Helper {
        Helper::start_with(mode, dir, home, None)
    }

    /// As `start`, with one more argument at the end of its command line. The test harness reads
    /// it as a second test name, which matches no test.
    fn start_with(mode: &str, dir: &Path, home: &Path, last: Option<&str>) -> Helper {
        let job = KillOnDrop::new();
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "win_tests::helper_process",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .args(last)
        .env(HELPER_MODE, mode)
        .env(HELPER_DIR, dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
        sandboxed(&mut cmd, home);
        let mut child = cmd.spawn().unwrap();
        job.add(&child);
        let stdin = child.stdin.take();
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let said = loop {
            let mut line = String::new();
            assert!(
                out.read_line(&mut line).unwrap() > 0,
                "The helper ended before it said anything"
            );
            // libtest writes `test <name> ... ` before the test runs, on the same line.
            if let Some(at) = line.find("HELPER ") {
                break line[at + 7..].trim().to_string();
            }
        };
        // Drain the rest so the helper never blocks on a full pipe.
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut out, &mut std::io::sink());
        });
        Helper {
            child,
            stdin,
            said,
            _job: job,
        }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Closes its stdin, so it lets go of its lock and ends, and waits for it.
    fn finish(mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The helper's body. Without its environment variable it does nothing, so a run of the ignored
/// tests is harmless.
#[test]
#[ignore = "Run only as the lock tests' helper process"]
fn helper_process() {
    let Ok(mode) = std::env::var(HELPER_MODE) else {
        return;
    };
    let dir = PathBuf::from(std::env::var_os(HELPER_DIR).unwrap());
    let taken = match mode.as_str() {
        "pass" => Some(WinLocks.pass(&dir)),
        "keeper" => Some(WinLocks.keeper(&dir)),
        _ => None,
    };
    let said = match &taken {
        None => "READY",
        Some(Taken::Held(_)) => "HELD",
        Some(Taken::Busy(_)) => "BUSY",
        Some(Taken::Skipped) => "SKIPPED",
        Some(Taken::Failed(_)) => "FAILED",
    };
    println!("HELPER {said}");
    let mut sink = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut sink);
    drop(taken);
}

fn held(t: Taken) -> Box<dyn std::any::Any> {
    match t {
        Taken::Held(g) => g,
        Taken::Busy(p) => panic!("Busy({p:?})"),
        Taken::Skipped => panic!("Skipped"),
        Taken::Failed(e) => panic!("Failed({e})"),
    }
}

fn handle_of(g: &dyn std::any::Any) -> &fs::File {
    &g.downcast_ref::<crate::lock::Held>().unwrap().file
}

fn sharing_violation(r: std::io::Result<fs::File>) -> bool {
    matches!(r, Err(e) if e.raw_os_error() == Some(32))
}

#[test]
fn the_pass_lock_shares_nothing_is_empty_and_is_never_removed() {
    let (dir, _) = scratch();
    let path = dir.path().join(PASS_LOCK_FILE);
    let g = held(WinLocks.pass(dir.path()));
    assert!(sharing_violation(
        OpenOptions::new().read(true).share_mode(0x7).open(&path)
    ));
    assert!(sharing_violation(
        OpenOptions::new().write(true).share_mode(0x7).open(&path)
    ));
    drop(g);
    assert_eq!(fs::read(&path).unwrap(), b"");
}

#[test]
fn a_second_process_holding_the_pass_lock_skips_a_pass_after_10_seconds() {
    let (dir, home) = scratch();
    let helper = Helper::start("pass", dir.path(), &home);
    assert_eq!(helper.said, "HELD");
    let start = Instant::now();
    let got = WinLocks.pass(dir.path());
    let took = start.elapsed();
    assert!(matches!(got, Taken::Skipped), "Not skipped");
    assert!(took >= Duration::from_secs(10), "Waited only {took:?}");
    assert!(took < Duration::from_secs(13), "Waited {took:?}");
    helper.finish();
    drop(held(WinLocks.pass(dir.path())));
}

#[test]
fn a_pass_takes_the_lock_once_a_second_process_lets_go_within_the_wait() {
    let (dir, home) = scratch();
    let helper = Helper::start("pass", dir.path(), &home);
    assert_eq!(helper.said, "HELD");
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        helper.finish();
    });
    let start = Instant::now();
    let g = held(WinLocks.pass(dir.path()));
    let took = start.elapsed();
    assert!(took >= Duration::from_millis(1000), "Took {took:?}");
    assert!(took < Duration::from_secs(9), "Took {took:?}");
    drop(g);
    releaser.join().unwrap();
}

#[test]
fn a_second_process_is_refused_the_pass_lock_while_this_one_holds_it() {
    let (dir, home) = scratch();
    let g = held(WinLocks.pass(dir.path()));
    let helper = Helper::start("pass", dir.path(), &home);
    assert_eq!(helper.said, "SKIPPED");
    drop(helper);
    drop(g);
}

#[test]
fn the_keeper_lock_holds_this_pid_and_shares_read_only() {
    let (dir, _) = scratch();
    let path = dir.path().join(KEEPER_LOCK_FILE);
    fs::write(&path, b"99999999 and more junk than a PID").unwrap();
    let g = held(WinLocks.keeper(dir.path()));
    // A reader of its PID, sharing read and write, can open it and read the PID.
    let mut text = String::new();
    OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(&path)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, std::process::id().to_string());
    // A writer, or a reader that does not share write, is refused.
    assert!(sharing_violation(
        OpenOptions::new().write(true).share_mode(0x7).open(&path)
    ));
    assert!(sharing_violation(
        OpenOptions::new().read(true).share_mode(0x1).open(&path)
    ));
    drop(g);
    assert!(path.exists(), "The lock file was removed");
}

#[test]
fn a_second_process_holding_the_keeper_lock_makes_it_busy_with_its_pid_at_once() {
    let (dir, home) = scratch();
    let helper = Helper::start("keeper", dir.path(), &home);
    assert_eq!(helper.said, "HELD");
    let start = Instant::now();
    let got = WinLocks.keeper(dir.path());
    assert!(start.elapsed() < Duration::from_secs(2), "A keeper waited");
    match got {
        Taken::Busy(Some(pid)) => assert_eq!(pid, helper.pid()),
        Taken::Busy(None) => panic!("Busy with no PID"),
        _ => panic!("Not busy"),
    }
    helper.finish();
    drop(held(WinLocks.keeper(dir.path())));
}

#[test]
fn a_second_process_is_refused_the_keeper_lock_while_this_one_holds_it() {
    let (dir, home) = scratch();
    let g = held(WinLocks.keeper(dir.path()));
    let helper = Helper::start("keeper", dir.path(), &home);
    assert_eq!(helper.said, "BUSY");
    drop(helper);
    drop(g);
}

#[test]
fn the_keeper_lock_held_by_something_else_is_busy_with_no_pid() {
    let (dir, _) = scratch();
    let path = dir.path().join(KEEPER_LOCK_FILE);
    fs::write(&path, b"not a pid").unwrap();
    // Held by a reader that shares read and write: the PID can be read, and is not one.
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0x1 | 0x2)
        .open(&path)
        .unwrap();
    assert!(matches!(WinLocks.keeper(dir.path()), Taken::Busy(None)));
    drop(f);
    // Held sharing nothing, as a scanner might for a moment: nothing can be read.
    fs::write(&path, b"1234").unwrap();
    let f = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    assert!(matches!(WinLocks.keeper(dir.path()), Taken::Busy(None)));
    drop(f);
}

#[test]
fn neither_lock_handle_is_inheritable_and_no_child_holds_one() {
    let (dir, home) = scratch();
    let pass = held(WinLocks.pass(dir.path()));
    let keeper = held(WinLocks.keeper(dir.path()));
    assert!(!is_inheritable(handle_of(pass.as_ref())));
    assert!(!is_inheritable(handle_of(keeper.as_ref())));
    // A child started while both are held, which inherits every inheritable handle.
    let child = Helper::start("sleep", dir.path(), &home);
    assert_eq!(child.said, "READY");
    drop(pass);
    drop(keeper);
    drop(held(WinLocks.pass(dir.path())));
    drop(held(WinLocks.keeper(dir.path())));
    child.finish();
}

#[test]
fn a_lock_in_a_missing_folder_or_on_a_folder_fails_at_once() {
    let (dir, _) = scratch();
    let missing = dir.path().join("missing");
    for t in [WinLocks.pass(&missing), WinLocks.keeper(&missing)] {
        match t {
            Taken::Failed(e) => assert!(e.contains("could not be opened"), "{e}"),
            _ => panic!("Not failed"),
        }
    }
    fs::create_dir(dir.path().join(PASS_LOCK_FILE)).unwrap();
    fs::create_dir(dir.path().join(KEEPER_LOCK_FILE)).unwrap();
    let start = Instant::now();
    let pass = WinLocks.pass(dir.path());
    assert!(start.elapsed() < Duration::from_secs(2), "A failure waited");
    match pass {
        Taken::Failed(e) => assert!(e.starts_with(PASS_LOCK_FILE), "{e}"),
        _ => panic!("Not failed"),
    }
    match WinLocks.keeper(dir.path()) {
        Taken::Failed(e) => assert!(e.starts_with(KEEPER_LOCK_FILE), "{e}"),
        _ => panic!("Not failed"),
    }
}

#[test]
fn the_locks_are_per_state_folder() {
    let (a, _) = scratch();
    let (b, _) = scratch();
    let pa = held(WinLocks.pass(a.path()));
    let ka = held(WinLocks.keeper(a.path()));
    let pb = held(WinLocks.pass(b.path()));
    let kb = held(WinLocks.keeper(b.path()));
    drop((pa, ka, pb, kb));
}

#[test]
fn the_system_folder_is_fully_qualified_and_holds_cmd() {
    let s = system_folder().unwrap();
    assert!(s.is_absolute());
    assert!(s.join("cmd.exe").is_file());
}

#[test]
fn a_matcher_built_on_the_system_folder_passes_its_self_check() {
    HolderMatcher::new(system_folder().unwrap())
        .self_check()
        .unwrap();
}

#[test]
fn a_written_pid_is_flushed_to_the_file() {
    let (dir, _) = scratch();
    let g = held(WinLocks.keeper(dir.path()));
    let mut f = handle_of(g.as_ref());
    f.flush().unwrap();
    drop(g);
    assert_eq!(
        fs::read(dir.path().join(KEEPER_LOCK_FILE)).unwrap(),
        std::process::id().to_string().as_bytes()
    );
}
