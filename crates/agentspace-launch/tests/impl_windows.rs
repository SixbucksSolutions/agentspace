//! The implementer's tests of Starting a holder on Windows (docs/keeper.md, Starting a holder,
//! items 1 to 7), through the real WMI create, the real launcher and stub holders. The handshake's
//! steps and bounds are tested with a fake system in src/handshake.rs.

#![cfg(windows)]
#![allow(unsafe_code)]

mod impl_support;

use agentspace_keeper_api::{StartRequest, Started, Starter};
use agentspace_keeper_files::{
    Claim, HolderMatcher, LaunchResult, claim_file, parse_claim, parse_launch_result, result_file,
};
use agentspace_launch::WmiStarter;
use impl_support::{Proc, Scratch, system32, testhost, wait_child, wait_for};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CreateProcessW, PROCESS_INFORMATION, ResumeThread, STARTUPINFOW,
};

const ALIAS: &str = "agentspace-launchtest-0a1b2c3d";
/// A variable set only in the asking process's environment (keeper.md, Streams and environment).
const MARKER: &str = "AGENTSPACE_LAUNCH_IMPL_MARKER";
/// Set in the child that runs `the_pass_in_the_job`.
const CHILD: &str = "AGENTSPACE_LAUNCH_IMPL_CHILD";

/// A job this test made, closed on drop.
struct Job(HANDLE);

impl Job {
    fn new(limits: u32) -> Job {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            assert!(!job.is_null());
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = limits;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            assert_ne!(ok, 0);
            Job(job)
        }
    }

    fn holds(&self, process: HANDLE) -> bool {
        let mut answer = 0;
        unsafe { IsProcessInJob(process, self.0, &mut answer) };
        answer != 0
    }

    fn close(self) {
        drop(self);
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Runs the test host as the launcher of `request` inside `job`: created suspended, put in the
/// job, then let run, so it never runs outside it. Its environment is the scratch's home.
fn launcher_in_job(s: &Scratch, job: &Job, request: &Path) -> Proc {
    let home = s.home().display().to_string();
    let mut env: Vec<u16> = Vec::new();
    for (k, v) in std::env::vars_os() {
        let k = k.to_string_lossy().into_owned();
        if ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"]
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&k))
        {
            continue;
        }
        env.extend(format!("{k}={}", v.to_string_lossy()).encode_utf16());
        env.push(0);
    }
    for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
        env.extend(format!("{name}={home}").encode_utf16());
        env.push(0);
    }
    env.push(0);
    let mut line = wide(&format!(
        "\"{}\" -Launch \"{}\"",
        testhost().display(),
        request.display()
    ));
    let dir = wide(&home);
    unsafe {
        let mut si: STARTUPINFOW = std::mem::zeroed();
        si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        let ok = CreateProcessW(
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_SUSPENDED | 0x0000_0400 | 0x0800_0000, // CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW
            env.as_ptr().cast(),
            dir.as_ptr(),
            &si,
            &mut pi,
        );
        assert_ne!(ok, 0, "The test host could not be created.");
        assert_ne!(AssignProcessToJobObject(job.0, pi.hProcess), 0);
        ResumeThread(pi.hThread);
        CloseHandle(pi.hThread);
        Proc {
            pid: pi.dwProcessId,
            handle: pi.hProcess,
        }
    }
}

/// The holder a launcher's result names, opened and checked by its creation time.
fn holder_of_result(s: &Scratch, hex: &str) -> Proc {
    let bytes = wait_for(Duration::from_secs(20), || {
        fs::read(s.state().join(result_file(hex))).ok()
    })
    .expect("The launcher wrote no result.");
    match parse_launch_result(&bytes).unwrap() {
        LaunchResult::Started {
            pid,
            process_started_at,
            ..
        } => Proc::open(pid, &process_started_at).expect("The holder named is not running."),
        LaunchResult::NotStarted { error } => panic!("The launcher started nothing: {error}"),
    }
}

/// The child half of `a_holder_started_through_wmi_outlives_the_job_of_the_process_that_asked`:
/// in a normal run it does nothing. In the child it waits for a line on stdin, starts a holder
/// with `WmiStarter`, prints what it got, and waits until its stdin closes or its job kills it.
#[test]
fn the_pass_in_the_job() {
    let Some(root) = std::env::var_os(CHILD) else {
        return;
    };
    let root = PathBuf::from(root);
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    let state = root.join("state");
    let home = root.join("home");
    let ssh = root.join("bin").join("ssh-stub.cmd");
    let own = testhost();
    let req = StartRequest {
        state_dir: &state,
        profile: &home,
        ssh_exe: &ssh,
        lease_id: "0a1b2c3d-0000-4000-8000-000000000001",
        alias: ALIAS,
        own_exe: &own,
        log: None,
    };
    let out = WmiStarter::new().start(&req, &HolderMatcher::new(system32()));
    let text = match out {
        Started::Holder(h) => format!("holder {} {}", h.pid, h.process_started_at),
        Started::Failed(reason) => format!("failed {reason}"),
    };
    println!("{text}");
    std::io::stdout().flush().unwrap();
    let mut rest = String::new();
    let _ = std::io::stdin().read_line(&mut rest);
}

/// Items 1, 4 and 5: the holder a pass starts through WMI is outside the pass's job, outlives that
/// job's close, runs in the pass's folder, and sees the logon environment, not the pass's.
#[test]
fn a_holder_started_through_wmi_outlives_the_job_of_the_process_that_asked() {
    let s = Scratch::new("wmi");
    let ssh = s.stub("ssh-stub.cmd", MARKER, 120);
    assert!(ssh.ends_with("ssh-stub.cmd"));
    let job = Job::new(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE);
    let mut child = s
        .command(&std::env::current_exe().unwrap())
        .args([
            "--exact",
            "the_pass_in_the_job",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, &s.root)
        .env(MARKER, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    unsafe {
        assert_ne!(
            AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE),
            0
        );
    }
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "go").unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut answer = String::new();
    loop {
        let mut line = String::new();
        if out.read_line(&mut line).unwrap() == 0 {
            break;
        }
        // libtest prints `test <name> ... ` before a test it runs alone, so the answer can
        // follow it on the same line.
        if let Some(at) = line.find("holder ").or_else(|| line.find("failed ")) {
            answer = line[at..].trim_end().to_string();
            break;
        }
    }
    let mut parts = answer.splitn(3, ' ');
    assert_eq!(parts.next(), Some("holder"), "{answer}");
    let pid: u32 = parts.next().unwrap().parse().unwrap();
    let started_at = parts.next().unwrap().to_string();
    let holder = Proc::open(pid, &started_at).expect("The holder is not running.");

    // No launch file is left.
    assert!(fs::read_dir(s.state()).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("board-holder-launch.")
    }));
    assert!(
        !job.holds(holder.handle),
        "The holder is in the pass's job."
    );

    job.close();
    assert!(wait_child(&mut child, Duration::from_secs(10)).is_some());
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        holder.running(),
        "The holder did not outlive the pass's job."
    );

    let text = wait_for(Duration::from_secs(10), || {
        let t = s.log_text();
        t.contains("profile=").then_some(t)
    })
    .expect("The stub wrote nothing to the holder's log.");
    assert!(text.contains("holder-started"));
    assert!(
        text.contains(&format!("cd=[{}]", s.home().display())),
        "The holder's folder is not the pass's."
    );
    assert!(
        text.contains("mark=unset"),
        "A variable of the pass reached the holder."
    );
    assert!(
        text.contains("profile=logon"),
        "The holder saw the pass's USERPROFILE."
    );
    assert!(text.contains("stdin=empty"));
    holder.stop_tree();
}

/// Item 2: stdin is NUL; stdout and stderr are one handle appending to the log, which shares
/// read, write and delete, so a reader, a writer, a rotation and a second holder are never refused.
#[test]
fn the_log_is_one_shared_append_handle_for_both_streams() {
    let s = Scratch::new("log");
    fs::write(s.log(), b"earlier\r\n").unwrap();
    let stub = s.stub("ssh-stub.cmd", MARKER, 60);
    let hex = "00000000000000000000000000000002";
    let request = s.request(hex, &stub, ALIAS, 30);
    let mut launcher = s.launcher(&request);
    assert_eq!(wait_child(&mut launcher, Duration::from_secs(20)), Some(0));
    let first = holder_of_result(&s, hex);
    let text = wait_for(Duration::from_secs(10), || {
        let t = s.log_text();
        t.contains("err-line").then_some(t)
    })
    .unwrap();
    assert!(
        text.starts_with("earlier\r\nholder-started"),
        "The log was not appended to."
    );
    assert!(
        text.contains("stdin=empty"),
        "The holder's stdin is not NUL."
    );
    assert!(text.contains(&format!("cd=[{}]", s.home().display())));

    // A reader and a writer that share everything are let in while the holder has it open.
    let mut writer = OpenOptions::new()
        .append(true)
        .share_mode(0x7)
        .open(s.log())
        .expect("The holder's log handle does not share writing.");
    writer.write_all(b"other-writer\r\n").unwrap();
    drop(writer);
    // A rotation renames it, which needs every open handle to share delete.
    let rotated = s.state().join("board-holder-ssh.log.1");
    fs::rename(s.log(), &rotated).expect("The holder's log handle does not share delete.");

    // A second holder opens a new log while the first still has the old one.
    let hex2 = "00000000000000000000000000000003";
    let request2 = s.request(hex2, &stub, ALIAS, 30);
    let mut launcher2 = s.launcher(&request2);
    assert_eq!(wait_child(&mut launcher2, Duration::from_secs(20)), Some(0));
    let second = holder_of_result(&s, hex2);
    assert!(
        wait_for(Duration::from_secs(10), || s
            .log_text()
            .contains("err-line")
            .then_some(()))
        .is_some()
    );
    first.stop_tree();
    second.stop_tree();
}

/// Item 5: a launcher in a job that allows breakaway starts the holder outside it, so the holder
/// outlives the job; one in a job that refuses breakaway still starts it, inside that job.
#[test]
fn the_holder_leaves_the_launchers_job_where_windows_allows() {
    let s = Scratch::new("breakaway");
    let stub = s.stub("ssh-stub.cmd", MARKER, 60);

    let allows = Job::new(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK);
    let hex = "00000000000000000000000000000004";
    let request = s.request(hex, &stub, ALIAS, 30);
    let launcher = launcher_in_job(&s, &allows, &request);
    let holder = holder_of_result(&s, hex);
    let outside = !allows.holds(holder.handle);
    allows.close();
    std::thread::sleep(Duration::from_millis(500));
    let survived = holder.running();
    holder.stop_tree();
    drop(launcher);

    let refuses = Job::new(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE);
    let hex2 = "00000000000000000000000000000005";
    let request2 = s.request(hex2, &stub, ALIAS, 30);
    let launcher2 = launcher_in_job(&s, &refuses, &request2);
    let holder2 = holder_of_result(&s, hex2);
    let inside = refuses.holds(holder2.handle);
    refuses.close();
    let killed = wait_for(Duration::from_secs(5), || {
        (!holder2.running()).then_some(())
    })
    .is_some();
    holder2.stop_tree();
    drop(launcher2);

    assert!(
        outside,
        "The holder did not leave a job that allows breakaway."
    );
    assert!(
        survived,
        "The holder did not outlive a job that allows breakaway."
    );
    assert!(
        inside,
        "Where breakaway is refused the holder is not in the launcher's job."
    );
    assert!(killed);
}

/// Item 6: when the pass's withdrawal races several launchers for one request, exactly one claim
/// is made, and at most one holder starts: none when the withdrawal won.
#[test]
fn the_claim_makes_at_most_one_holder_when_the_pass_and_launchers_race() {
    for round in 0..4u64 {
        let s = Scratch::new("race");
        let stub = s.stub("ssh-stub.cmd", MARKER, 30);
        let hex = format!("{:032x}", 0x10 + round);
        let request = s.request(&hex, &stub, ALIAS, 30);
        let mut launchers: Vec<_> = (0..4).map(|_| s.launcher(&request)).collect();
        std::thread::sleep(Duration::from_millis(round * 15));
        let claim = s.state().join(claim_file(&hex));
        let withdrew = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0x1)
            .open(&claim)
            .map(|mut f| f.write_all(b"abandoned\n"))
            .is_ok();
        for l in &mut launchers {
            assert_eq!(wait_child(l, Duration::from_secs(20)), Some(0));
        }
        let content = parse_claim(&fs::read(&claim).unwrap());
        let result = s.state().join(result_file(&hex));
        if withdrew {
            assert_eq!(content, Some(Claim::Abandoned));
            std::thread::sleep(Duration::from_millis(1500));
            assert!(!result.exists());
            assert!(!s.log_text().contains("holder-started"));
        } else {
            assert!(matches!(content, Some(Claim::Launcher(_))));
            let holder = holder_of_result(&s, &hex);
            wait_for(Duration::from_secs(10), || {
                s.log_text().contains("err-line").then_some(())
            });
            std::thread::sleep(Duration::from_millis(1000));
            let starts = s.log_text().matches("holder-started").count();
            holder.stop_tree();
            assert_eq!(starts, 1, "Round {round}: {starts} holders started.");
        }
    }
}

/// Item 7 at the boundary: a start that fails says so with the whole reason, and leaves no launch
/// file behind; here the launcher's deadline is already past when it claims.
#[test]
fn a_launcher_past_its_deadline_answers_that_it_started_nothing() {
    let s = Scratch::new("late");
    let stub = s.stub("ssh-stub.cmd", MARKER, 30);
    let hex = "00000000000000000000000000000006";
    let request = s.request(hex, &stub, ALIAS, -1);
    let mut launcher = s.launcher(&request);
    assert_eq!(wait_child(&mut launcher, Duration::from_secs(20)), Some(0));
    let answer = parse_launch_result(&fs::read(s.state().join(result_file(hex))).unwrap()).unwrap();
    assert_eq!(
        answer,
        LaunchResult::NotStarted {
            error: "The launcher reached its deadline before it started the holder, so it started nothing.".to_string()
        }
    );
    assert!(!s.log().exists());
}
