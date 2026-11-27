//! A release leaves no client of the alias running, among them a desktop-app style proxy
//! (docs/driver.md, "Release a lease", "Ending a lease's connections", "Which processes are the
//! alias's clients", "Each with its tree", "The count", "Where the count is logged"; E0.10-T03-S02,
//! #0220, whose evidence is #0196 and docs/runs/e0.08-t06-s02.md).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! **What is asserted.** When `release` returns, every process this test started as a client of the
//! alias, or inside a client's tree, or as an `agentspace-ssh-proxy.exe` holding the alias, is no
//! longer running and was given exit code 1, checked at once by its own handle, with no wait. The
//! release's own output is what driver.md says: it succeeds, the alias file and the lease's folder
//! are gone, and `release.log` holds one line, `Ended <n> of the lease's connections.`, where `<n>`
//! counts the clients that have no client among their ancestors.
//!
//! **The shapes, and the line of driver.md each rests on.**
//! - **The plain `ssh <alias>`:** "Which processes are the alias's clients", rules 2 and 3.
//! - **A desktop-app connection**, an `ssh.exe` with the app's options and a ProxyCommand that names
//!   the app's own `claude-ssh-proxy.exe` and a `proxy-command.txt`, the alias last, with that proxy
//!   below it and, below that, the alias's own `agentspace-ssh-proxy.exe <alias>` and a
//!   `session-manager-plugin.exe`: "Each with its tree" ("The tree takes in the ProxyCommand and
//!   what it runs ... and the desktop app's own proxy"), and "The Claude desktop app's connections
//!   are clients". The command lines come from decisions.md's D12 findings (E2.02, card #0042).
//! - **A proxy whose parent has exited, and one whose parent is no client:** the proxy is a client in
//!   its own right, "An `agentspace-ssh-proxy.exe` is a client when exactly one token follows the
//!   program, and that token is the alias", and is counted on its own, "The clients are counted by
//!   the processes that are a client and have no client among their ancestors".
//! - **A client below a proxy that has exited while the client above still holds it, two deep:**
//!   "Release a lease", "the release MUST end them again" for as long as the ending before ended a
//!   client, and "A release MUST leave no process holding the alias that its endings can end"
//!   (D59, as changed for v0.10-T03; #0196).
//! - **The spelling:** "The alias is compared whole, without regard to ASCII case", and so is the
//!   image file name.
//! - **Not clients:** another lease's proxy and `ssh.exe` stay running ("The call MUST NOT end any
//!   other process").
//! - **A standalone `claude-ssh-proxy.exe` holding the alias is NOT asserted either way.** On main's
//!   driver.md it is no client ("Nothing else is a client"); a rule that changes that is the
//!   implementer's, and a test for it comes with the rule.
//!
//! **Process safety.** Every process here is a stand-in: a renamed copy of `tests/support/stub.rs`'s
//! program, or of this test program itself (the chain node below), in a folder of its own under the
//! temp folder, started with the command line a case stands for. The alias of each test is built from
//! a UUIDv4 drawn at random for that test, so no process of Terry's, and no other test's stand-in,
//! can match it, and the release walks only a process whose executable is in that folder
//! (`with_walk_under`). Each test ends every process it started, by the process ID its parent wrote
//! and a handle held from then on, when it ends, whatever the assertions do. No test lists, reads or
//! prints any other process or a command line, and the throwaway home holds every file the release
//! touches.
#![cfg(windows)]

#[path = "common/clients.rs"]
mod clients;
mod common;

use agentspace_connections::EndError;
use agentspace_connections::line::release_text;
use agentspace_driver_api::{Driver, Failure, LeaseRef, PlaceRequest, Placed};
use agentspace_driver_ec2::Ec2Driver;
use clients::*;
use common::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

// ---- A ready lease in a throwaway home ----

struct Rel {
    fx: Fx,
    id: String,
    alias: String,
    other: String,
}

fn alias_of(id: &str) -> String {
    format!("agentspace-{PROJECT}-{}", &id[..8])
}

impl Rel {
    fn ready() -> Rel {
        let fx = Fx::with_ca();
        let id = random_uuid_v4();
        let alias = alias_of(&id);
        let mut other = alias_of(&random_uuid_v4());
        while other == alias {
            other = alias_of(&random_uuid_v4());
        }
        fs::create_dir_all(&fx.ctx.state_dir).unwrap();
        let rel = Rel {
            fx,
            id,
            alias,
            other,
        };
        let req = PlaceRequest {
            id: &rel.id,
            project: PROJECT,
            platform: "linux/arm64",
            profile: "agent-linux",
            size: "m",
            ttl_seconds: 600,
        };
        match rel.fx.driver().place(&rel.fx.ctx, &req) {
            Ok(Placed::Ready { host_alias, .. }) => assert_eq!(host_alias, rel.alias),
            other => panic!("Expected a ready place, got {other:?}"),
        }
        rel
    }

    fn lease(&self) -> LeaseRef<'_> {
        LeaseRef {
            id: &self.id,
            project: PROJECT,
            platform: "linux/arm64",
        }
    }

    /// The release, walking only for processes whose executable is in the lab's folder.
    fn release(&self, lab: &Lab) -> Result<(), Failure> {
        Ec2Driver::new()
            .with_walk_under(&lab.dir)
            .release(&self.fx.ctx, &self.lease())
    }

    /// What driver.md says a release of a lease leaves: no alias file, no folder, and one line in
    /// `release.log` that says `Ended <n> of the lease's connections.` and nothing else.
    fn assert_released(&self, since: i64, until: i64, n: usize) {
        assert!(
            !self.fx.alias_dir().join(&self.alias).exists(),
            "The alias file is deleted"
        );
        assert!(
            !self.fx.lease_dir(&self.id).exists(),
            "The lease's folder is deleted"
        );
        let log = self.fx.ctx.state_dir.join("release.log");
        let bytes = fs::read(&log).expect("release.log exists");
        assert!(bytes.ends_with(b"\n"), "The line ends in LF");
        assert!(!bytes.contains(&b'\r'), "The line ends in LF alone");
        let text = String::from_utf8(bytes).expect("The log is UTF-8");
        let lines: Vec<&str> = text.split_terminator('\n').collect();
        assert_eq!(lines.len(), 1, "The release appends one line");
        let line = lines[0];
        assert!(line.len() > 20, "The line is too short");
        let time = parse_utc(&line[..20]).expect("A line's time is YYYY-MM-DDTHH:MM:SSZ");
        assert!(
            time >= since - 1 && time <= until + 1,
            "The line's time is not the release's"
        );
        let prefix = format!(" lease={} ", self.id);
        let rest = line[20..]
            .strip_prefix(&prefix)
            .expect("The line names the lease after the time");
        assert!(!line.contains(&self.alias), "The line holds the alias");
        let want = release_text(&Ok::<usize, EndError>(n), &Ok::<usize, EndError>(0));
        assert_eq!(
            rest, want,
            "The line counts {n} clients ended by the release's walks between them"
        );
    }
}

// ---- Processes the test started below the Lab's own ----

/// Processes this test started below a stand-in, held by handle. Dropping it ends them all, by
/// the handle, so a test that fails leaves none running.
struct Leaves(Vec<Held>);

impl Drop for Leaves {
    fn drop(&mut self) {
        for h in &self.0 {
            h.terminate();
        }
    }
}

/// When `release` has returned, a process of this test is no longer running, and was ended with
/// exit code 1 (driver.md, "Each with its tree"). No wait: the walk waits, not the test.
fn assert_ended_held(what: &str, held: &[Held]) {
    for (i, h) in held.iter().enumerate() {
        assert!(
            !h.alive(),
            "{what} {i} is still running when the release returned"
        );
        assert_eq!(
            h.exit_code(),
            Some(1),
            "{what} {i} was not given exit code 1"
        );
    }
}

/// The same for a stand-in the lab started.
fn assert_ended(lab: &mut Lab, index: usize, what: &str) {
    assert!(
        !lab.alive(index),
        "{what} is still running when the release returned"
    );
    assert_eq!(
        lab.exit_code(index),
        Some(1),
        "{what} was not given exit code 1"
    );
}

// ---- The chain node: a copy of this test program that starts leaves and stays or exits ----

const NODE_PIDS: &str = "ASDRV_DP_PIDS";
const NODE_LEAVES: &str = "ASDRV_DP_LEAVES";
const NODE_STAY: &str = "ASDRV_DP_STAY";
/// A file the node writes its own process ID to, before it does anything else.
const NODE_SELF: &str = "ASDRV_DP_SELF";
/// A file the node waits for, so that the test can put it in a job first.
const NODE_GO: &str = "ASDRV_DP_GO";
/// A file the node starts its leaves only while it exists: the alias file of a lease.
const NODE_WATCH: &str = "ASDRV_DP_WATCH";
/// How many times the node starts all its leaves, at most.
const NODE_CAP: &str = "ASDRV_DP_CAP";
/// Variables the node sets on every leaf, after it has removed its own: `PAIR`-separated pairs,
/// each a name, `NAME_END`, and a value. Only the two-deep chain sets it.
const NODE_LEAF_ENV: &str = "ASDRV_DP_LEAF_ENV";
const NODE_TEST: &str = "chain_node_is_not_a_test";
const UNIT: char = '\u{1f}';
const RECORD: char = '\u{1e}';
const PAIR: char = '\u{1d}';
const NAME_END: char = '\u{1c}';

/// Not a test of its own: the stand-in for a desktop app's `claude-ssh-proxy.exe`, or any process
/// between a client and what it runs. It passes at once when it is not told to act, as in every
/// ordinary run. Told to by the variables the cases below set, it starts each leaf, appends the
/// leaf's process ID to a file, and then either waits (for at most 60 seconds, or until it is
/// ended) or exits at once, which leaves its leaves with a parent that has gone. Told to repeat, it
/// starts them again and again, as an app that reconnects does, for as long as the watched file
/// exists and at most `NODE_CAP` times, and ends a leaf itself whose start returned after the
/// watched file was gone. A leaf gets none of the node's `ASDRV_` variables, only
/// those `NODE_LEAF_ENV` names.
#[test]
fn chain_node_is_not_a_test() {
    let Some(pids) = std::env::var_os(NODE_PIDS).map(PathBuf::from) else {
        return;
    };
    use std::io::Write as _;
    let path_var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    if let Some(file) = path_var(NODE_SELF) {
        fs::write(file, std::process::id().to_string()).expect("The node's ID is written");
    }
    if let Some(go) = path_var(NODE_GO) {
        let begun = Instant::now();
        while !go.exists() && begun.elapsed() < Duration::from_secs(30) {
            sleep(Duration::from_millis(5));
        }
    }
    let watch = path_var(NODE_WATCH);
    let cap: usize = std::env::var(NODE_CAP)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let leaves = std::env::var(NODE_LEAVES).unwrap_or_default();
    let leaf_env = std::env::var(NODE_LEAF_ENV).unwrap_or_default();
    let stay = std::env::var(NODE_STAY).is_ok_and(|v| v == "1");
    let mut started = Vec::new();
    for round in 0..cap {
        if let Some(w) = &watch
            && !w.exists()
        {
            break;
        }
        if round > 0 {
            sleep(Duration::from_millis(1));
        }
        for leaf in leaves.split(RECORD).filter(|l| !l.is_empty()) {
            let mut parts = leaf.split(UNIT);
            let exe = parts.next().expect("A leaf names its program");
            let mut command = Command::new(exe);
            command.args(parts.filter(|a| !a.is_empty()));
            for (name, _) in std::env::vars_os() {
                if name.to_string_lossy().starts_with("ASDRV_") {
                    command.env_remove(name);
                }
            }
            for pair in leaf_env.split(PAIR).filter(|p| !p.is_empty()) {
                if let Some((name, value)) = pair.split_once(NAME_END) {
                    command.env(name, value);
                }
            }
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut child = command.spawn().expect("A leaf starts");
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&pids)
                .expect("The ID file opens");
            writeln!(file, "{}", child.id()).expect("The ID is written");
            // A leaf whose start returned after the watched file was gone may have started after
            // the release's last walk read the process list, which driver.md's "Release a lease"
            // places outside its claim (#0229). The node ends it itself, so that a case asserts
            // only what that rule claims: every leaf found running with the file still there was
            // running before the file's deletion, so before the release's second walk.
            if let Some(w) = &watch
                && !w.exists()
            {
                let _ = child.kill();
                let _ = child.wait();
            }
            started.push(child);
        }
    }
    if stay {
        let begun = Instant::now();
        while begun.elapsed() < Duration::from_secs(60) {
            sleep(Duration::from_millis(100));
        }
    }
}

/// A node's program, arguments and environment: the copy of this test program under `image` (a
/// desktop app's proxy, which is no client by its image), `file` as the one argument that stands
/// for the app's `proxy-command.txt`, and the leaves it starts.
fn node(
    lab: &Lab,
    image: &str,
    pids: &Path,
    leaves: &[(PathBuf, Vec<String>)],
    stay: bool,
    file: &Path,
) -> (PathBuf, Vec<String>, Vec<(String, String)>) {
    let exe = lab.dir.join(image);
    if !exe.exists() {
        fs::copy(
            std::env::current_exe().expect("The test program's own path"),
            &exe,
        )
        .expect("The chain node's copy can be made");
    }
    let args = vec![
        "--exact".to_string(),
        NODE_TEST.to_string(),
        "--test-threads=1".to_string(),
        file.to_string_lossy().into_owned(),
    ];
    let encoded: Vec<String> = leaves
        .iter()
        .map(|(program, leaf_args)| {
            let mut parts = vec![program.to_string_lossy().into_owned()];
            parts.extend(leaf_args.iter().cloned());
            parts.join(&UNIT.to_string())
        })
        .collect();
    let env = vec![
        (NODE_PIDS.to_string(), pids.to_string_lossy().into_owned()),
        (NODE_LEAVES.to_string(), encoded.join(&RECORD.to_string())),
        (
            NODE_STAY.to_string(),
            if stay { "1" } else { "0" }.to_string(),
        ),
    ];
    (exe, args, env)
}

/// Holds, by handle, the `count` leaves a node writes to `pids`, as soon as they are written.
fn hold_leaves(leaves: &mut Leaves, pids: &Path, count: usize) {
    wait_for(
        || {
            if let Ok(text) = fs::read_to_string(pids) {
                for line in text.split_terminator('\n') {
                    let Ok(pid) = line.trim().parse::<u32>() else {
                        continue;
                    };
                    if leaves.0.iter().any(|h| h.pid == pid) {
                        continue;
                    }
                    leaves
                        .0
                        .push(Held::open(pid).expect("A leaf of the node is opened"));
                }
            }
            leaves.0.len() >= count
        },
        "A node's leaves",
    );
}

/// A command line like the Claude desktop app's `ssh.exe` (decisions.md, D12's findings, card
/// #0042), with `proxy` as its ProxyCommand's program and `file` its `proxy-command.txt`, and the
/// alias last.
fn desktop_args(proxy: &Path, file: &Path, alias: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    for pair in [
        "ClearAllForwardings=yes",
        "StrictHostKeyChecking=yes",
        "ConnectTimeout=30",
        "ServerAliveInterval=15",
        "ServerAliveCountMax=3",
        "ForwardAgent=no",
    ] {
        args.push("-o".to_string());
        args.push(pair.to_string());
    }
    args.push("-o".to_string());
    args.push(format!(
        "ProxyCommand={} {}",
        proxy.to_string_lossy(),
        file.to_string_lossy()
    ));
    args.push("-T".to_string());
    args.push(alias.to_string());
    args
}

fn refs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

// ---- A job: every process a node starts, whenever it starts it ----

/// A Windows job object, to count the processes of one case that are still running. A node is put
/// in it before it starts anything, and every process it starts, and every process those start,
/// is in it from its creation, so the count needs no process ID and lists no other process. Ending
/// the job ends them all, which is how a case never leaves one behind.
#[allow(unsafe_code)]
mod job {
    use std::ffi::c_void;

    type Handle = *mut c_void;

    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_SET_QUOTA: u32 = 0x0100;
    const JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION: i32 = 1;

    #[repr(C)]
    #[derive(Default)]
    struct Accounting {
        total_user_time: i64,
        total_kernel_time: i64,
        this_period_total_user_time: i64,
        this_period_total_kernel_time: i64,
        total_page_fault_count: u32,
        total_processes: u32,
        active_processes: u32,
        total_terminated_processes: u32,
    }

    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> Handle;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn QueryInformationJobObject(
            job: Handle,
            class: i32,
            info: *mut c_void,
            length: u32,
            returned: *mut u32,
        ) -> i32;
        fn TerminateJobObject(job: Handle, code: u32) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
    }

    pub struct Job(Handle);

    impl Job {
        pub fn new() -> Job {
            // SAFETY: No attributes and no name: a job of this test's own.
            let handle = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
            assert!(!handle.is_null(), "A job object can be made");
            Job(handle)
        }

        /// Puts the process with this ID, which the test started and still holds, in the job.
        pub fn assign(&self, pid: u32) -> bool {
            // SAFETY: OpenProcess takes plain values and returns a handle or null.
            let process = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
            if process.is_null() {
                return false;
            }
            // SAFETY: Both handles are open. The process handle is closed once, here.
            let ok = unsafe { AssignProcessToJobObject(self.0, process) } != 0;
            // SAFETY: The handle is open and closed once.
            unsafe {
                CloseHandle(process);
            }
            ok
        }

        /// How many processes of the job are still running.
        pub fn active(&self) -> u32 {
            let mut info = Accounting::default();
            // SAFETY: `info` is the structure the class names, and its length is given.
            let ok = unsafe {
                QueryInformationJobObject(
                    self.0,
                    JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION,
                    (&mut info as *mut Accounting).cast(),
                    size_of::<Accounting>() as u32,
                    std::ptr::null_mut(),
                )
            };
            assert!(ok != 0, "The job's count can be read");
            info.active_processes
        }

        /// How many processes the job has held in all, ended or not.
        pub fn total(&self) -> u32 {
            let mut info = Accounting::default();
            // SAFETY: As in `active`.
            let ok = unsafe {
                QueryInformationJobObject(
                    self.0,
                    JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION,
                    (&mut info as *mut Accounting).cast(),
                    size_of::<Accounting>() as u32,
                    std::ptr::null_mut(),
                )
            };
            assert!(ok != 0, "The job's count can be read");
            info.total_processes
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: The handle is open, owned by `self`, and closed once.
            unsafe {
                TerminateJobObject(self.0, 1);
                CloseHandle(self.0);
            }
        }
    }
}

use job::Job;

/// Puts a node, which waits for `go`, in the job by the process ID it wrote to `own`, and lets it
/// go on.
fn adopt(job: &Job, own: &Path, go: &Path) {
    let mut pid = 0u32;
    wait_for(
        || match fs::read_to_string(own)
            .ok()
            .and_then(|t| t.trim().parse().ok())
        {
            Some(p) => {
                pid = p;
                true
            }
            None => false,
        },
        "The node's own process ID",
    );
    assert!(job.assign(pid), "The node is put in the job");
    fs::write(go, "go").expect("The go file is written");
}

/// Waits until the node has written at least `count` process IDs.
fn wait_started(pids: &Path, count: usize) {
    wait_for(|| lines_of(pids).len() >= count, "The node's first leaves");
}

// ---- The cases ----

#[test]
fn a_plain_ssh_client_is_ended() {
    let rel = Rel::ready();
    let mut lab = Lab::new();
    let client = lab.start("ssh.exe", &[&rel.alias]);
    let (since, until) = {
        let since = unix_now();
        rel.release(&lab).expect("The release succeeds");
        (since, unix_now())
    };
    assert_ended(&mut lab, client, "The client");
    rel.assert_released(since, until, 1);
}

#[test]
fn a_desktop_apps_connection_is_ended_with_its_proxy_and_what_the_proxy_runs() {
    let rel = Rel::ready();
    let mut lab = Lab::new();
    let a = rel.alias.clone();
    let pids = lab.dir.join("leaves-0.txt");
    let file = lab.dir.join("ccd-ssh-0a1b2c3d").join("proxy-command.txt");
    let proxy = lab.copy_of("agentspace-ssh-proxy.exe");
    let plugin = lab.copy_of("session-manager-plugin.exe");
    let (node_exe, node_args, env) = node(
        &lab,
        "claude-ssh-proxy.exe",
        &pids,
        &[(proxy, vec![a.clone()]), (plugin, vec![])],
        true,
        &file,
    );
    let args = desktop_args(&node_exe, &file, &a);
    let client = lab.start_with(
        "ssh.exe",
        &refs(&args),
        Opts {
            children: vec![(node_exe, node_args)],
            env,
            ..Opts::default()
        },
    );
    let mut leaves = Leaves(Vec::new());
    hold_leaves(&mut leaves, &pids, 2);
    // Another lease's proxy and client stay.
    let bystander = lab.start("agentspace-ssh-proxy.exe", &[&rel.other]);

    let (since, until) = {
        let since = unix_now();
        rel.release(&lab).expect("The release succeeds");
        (since, unix_now())
    };
    assert_ended(&mut lab, client, "The desktop app's ssh.exe");
    assert_ended_held("The app's proxy", lab.children(client));
    assert_ended_held("A process the app's proxy runs", &leaves.0);
    assert!(lab.alive(bystander), "Another lease's proxy was ended");
    // One client, with its whole tree: the alias's proxy inside it is not counted on its own.
    rel.assert_released(since, until, 1);
}

#[test]
fn a_proxy_whose_parent_has_exited_is_ended_as_a_client() {
    let rel = Rel::ready();
    let mut lab = Lab::new();
    let a = rel.alias.clone();
    let pids = lab.dir.join("leaves-0.txt");
    let file = lab.dir.join("ccd-ssh-0a1b2c3d").join("proxy-command.txt");
    let proxy = lab.copy_of("agentspace-ssh-proxy.exe");
    let (node_exe, node_args, env) = node(
        &lab,
        "claude-ssh-proxy.exe",
        &pids,
        &[(proxy, vec![a])],
        false,
        &file,
    );
    // The node starts the proxy and exits at once: the proxy's parent ID names a process that is gone.
    let node_index = lab.start_with(
        "claude-ssh-proxy.exe",
        &refs(&node_args),
        Opts {
            env,
            ..Opts::default()
        },
    );
    let _ = node_exe;
    let mut leaves = Leaves(Vec::new());
    hold_leaves(&mut leaves, &pids, 1);
    wait_for(|| !lab.alive(node_index), "The node's exit");
    assert!(leaves.0[0].alive(), "The proxy runs before the release");

    let (since, until) = {
        let since = unix_now();
        rel.release(&lab).expect("The release succeeds");
        (since, unix_now())
    };
    assert_ended_held("The proxy", &leaves.0);
    rel.assert_released(since, until, 1);
}

/// #0196's chain, two deep: `ssh.exe <alias>`, below it a desktop-app proxy that has exited while
/// that `ssh.exe` still holds it, below that a second `ssh.exe <alias>`, holding a second proxy that
/// has exited, and below that a third `ssh.exe <alias>`. A client below a proxy that has exited, while
/// a client above still holds that proxy, is in no client's tree and is not counted on its own while
/// the client above runs, so each ending reaches one level more: the first ends the top client, the
/// second the next, and only a third the last (driver.md, "Release a lease": the release MUST end the
/// clients again for as long as the ending before ended one, and MUST leave no process holding the
/// alias that its endings can end; D59, as changed for v0.10-T03). Two endings, as before v0.10,
/// leave the last client running when the release returns.
#[test]
fn a_chain_of_clients_below_proxies_that_have_exited_is_ended_whole() {
    let rel = Rel::ready();
    let mut lab = Lab::new();
    let a = rel.alias.clone();
    let file = lab.dir.join("ccd-ssh-0a1b2c3d").join("proxy-command.txt");
    let ssh = lab.copy_of("ssh.exe");
    // The IDs the first proxy writes (the second client), the second client writes (the second
    // proxy), and the second proxy writes (the third client).
    let pids_1 = lab.dir.join("leaves-1.txt");
    let pids_2 = lab.dir.join("stub-pids-2.txt");
    let pids_3 = lab.dir.join("leaves-3.txt");

    // The lower proxy: it starts the third client and exits at once.
    let (lower_exe, lower_args, lower_env) = node(
        &lab,
        "claude-ssh-proxy-1.exe",
        &pids_3,
        &[(ssh.clone(), vec![a.clone()])],
        false,
        &file,
    );
    // The second client: a stand-in `ssh.exe <alias>` that starts the lower proxy and holds it.
    let mut second_env = vec![
        ("ASDRV_STUB_CHILDREN".to_string(), "1".to_string()),
        (
            "ASDRV_STUB_CHILD0_EXE".to_string(),
            lower_exe.to_string_lossy().into_owned(),
        ),
        (
            "ASDRV_STUB_CHILD0_ARGS".to_string(),
            lower_args.join(&UNIT.to_string()),
        ),
        (
            "ASDRV_STUB_PIDS".to_string(),
            pids_2.to_string_lossy().into_owned(),
        ),
    ];
    second_env.extend(lower_env);
    let encoded: Vec<String> = second_env
        .iter()
        .map(|(name, value)| format!("{name}{NAME_END}{value}"))
        .collect();
    // The upper proxy: it starts the second client, with the variables above, and exits at once.
    let (upper_exe, upper_args, mut upper_env) = node(
        &lab,
        "claude-ssh-proxy-0.exe",
        &pids_1,
        &[(ssh.clone(), vec![a.clone()])],
        false,
        &file,
    );
    upper_env.push((NODE_LEAF_ENV.to_string(), encoded.join(&PAIR.to_string())));
    // The top client, which holds the upper proxy.
    let top = lab.start_with(
        "ssh.exe",
        &[&a],
        Opts {
            children: vec![(upper_exe, upper_args)],
            env: upper_env,
            ..Opts::default()
        },
    );
    let mut clients = Leaves(Vec::new());
    hold_leaves(&mut clients, &pids_1, 1);
    let mut lower = Leaves(Vec::new());
    hold_leaves(&mut lower, &pids_2, 1);
    hold_leaves(&mut clients, &pids_3, 2);
    wait_for(
        || !lab.children(top)[0].alive() && !lower.0[0].alive(),
        "Both proxies' exits",
    );
    assert!(
        clients.0.iter().all(Held::alive),
        "Every client below a proxy runs before the release"
    );

    let (since, until) = {
        let since = unix_now();
        rel.release(&lab).expect("The release succeeds");
        (since, unix_now())
    };
    assert_ended(&mut lab, top, "The top client");
    for (depth, h) in [(1, &clients.0[0]), (2, &clients.0[1])] {
        assert!(
            !h.alive(),
            "The client {depth} exited proxies down is still running when the release returned"
        );
        assert_eq!(
            h.exit_code(),
            Some(1),
            "The client {depth} exited proxies down was not given exit code 1"
        );
    }
    // Each client is counted on its own, by the ending that found it with no client above it.
    rel.assert_released(since, until, 3);
}

#[test]
fn a_proxy_whose_parent_is_no_client_is_ended_as_a_client() {
    let rel = Rel::ready();
    let mut lab = Lab::new();
    let a = rel.alias.clone();
    let pids = lab.dir.join("leaves-0.txt");
    let file = lab.dir.join("ccd-ssh-0a1b2c3d").join("proxy-command.txt");
    let proxy = lab.copy_of("agentspace-ssh-proxy.exe");
    let (_, node_args, env) = node(
        &lab,
        "claude-ssh-proxy.exe",
        &pids,
        &[(proxy, vec![a])],
        true,
        &file,
    );
    // The parent stays and is no client by its image: nothing here is asserted of it.
    let _parent = lab.start_with(
        "claude-ssh-proxy.exe",
        &refs(&node_args),
        Opts {
            env,
            ..Opts::default()
        },
    );
    let mut leaves = Leaves(Vec::new());
    hold_leaves(&mut leaves, &pids, 1);
    assert!(leaves.0[0].alive(), "The proxy runs before the release");

    let (since, until) = {
        let since = unix_now();
        rel.release(&lab).expect("The release succeeds");
        (since, unix_now())
    };
    assert_ended_held("The proxy", &leaves.0);
    rel.assert_released(since, until, 1);
}

#[test]
fn a_proxy_and_a_client_in_another_spelling_are_ended() {
    let rel = Rel::ready();
    let mut lab = Lab::new();
    let upper = rel.alias.to_uppercase();
    let proxy = lab.start("Agentspace-SSH-Proxy.EXE", &[&upper]);
    let ssh = lab.start("SSH.EXE", &[&format!("ubuntu@{upper}")]);
    let (since, until) = {
        let since = unix_now();
        rel.release(&lab).expect("The release succeeds");
        (since, unix_now())
    };
    assert_ended(&mut lab, proxy, "The proxy");
    assert_ended(&mut lab, ssh, "The client");
    rel.assert_released(since, until, 2);
}

/// What #0188's release met on 2026-11-23, in shape: the keeper's holder and two desktop-app
/// connections, each an `ssh.exe` with a proxy below it, and one proxy whose parent has gone, so
/// three clients and a fourth; the release's line counted 4, and one proxy was still running a second
/// after it returned. Another lease's two processes run beside them.
#[test]
fn every_client_of_the_run_of_e0_08_t06_s02s_shape_is_ended() {
    let rel = Rel::ready();
    let mut lab = Lab::new();
    let a = rel.alias.clone();
    let proxy = lab.copy_of("agentspace-ssh-proxy.exe");
    let plugin = lab.copy_of("session-manager-plugin.exe");
    let file = lab.dir.join("ccd-ssh-0a1b2c3d").join("proxy-command.txt");

    // The keeper's holder, with the alias's proxy below it.
    let holder = lab.start_with(
        "ssh.exe",
        &["-N", "-o", "ExitOnForwardFailure=yes", &a],
        Opts {
            children: vec![(proxy.clone(), vec![a.clone()])],
            ..Opts::default()
        },
    );

    // Two desktop-app connections.
    let mut leaves = Leaves(Vec::new());
    let mut apps = Vec::new();
    for n in 0..2 {
        let pids = lab.dir.join(format!("leaves-{n}.txt"));
        let image = format!("claude-ssh-proxy-{n}.exe");
        let (node_exe, node_args, env) = node(
            &lab,
            &image,
            &pids,
            &[(proxy.clone(), vec![a.clone()]), (plugin.clone(), vec![])],
            true,
            &file,
        );
        let args = desktop_args(&node_exe, &file, &a);
        let app = lab.start_with(
            "ssh.exe",
            &refs(&args),
            Opts {
                children: vec![(node_exe, node_args)],
                env,
                ..Opts::default()
            },
        );
        apps.push(app);
        hold_leaves(&mut leaves, &pids, 2 * (n + 1));
    }

    // A proxy whose parent has gone.
    let orphan_pids = lab.dir.join("leaves-orphan.txt");
    let (_, node_args, env) = node(
        &lab,
        "claude-ssh-proxy-orphan.exe",
        &orphan_pids,
        &[(proxy.clone(), vec![a.clone()])],
        false,
        &file,
    );
    let orphan_parent = lab.start_with(
        "claude-ssh-proxy-orphan.exe",
        &refs(&node_args),
        Opts {
            env,
            ..Opts::default()
        },
    );
    let mut orphan = Leaves(Vec::new());
    hold_leaves(&mut orphan, &orphan_pids, 1);
    wait_for(|| !lab.alive(orphan_parent), "The orphan's parent's exit");

    // Another lease's.
    let other_ssh = lab.start("ssh.exe", &[&rel.other]);
    let other_proxy = lab.start("agentspace-ssh-proxy.exe", &[&rel.other]);

    let (since, until) = {
        let since = unix_now();
        rel.release(&lab).expect("The release succeeds");
        (since, unix_now())
    };
    assert_ended(&mut lab, holder, "The holder");
    assert_ended_held("The holder's proxy", lab.children(holder));
    for (n, app) in apps.iter().enumerate() {
        assert_ended(&mut lab, *app, &format!("Desktop-app connection {n}"));
        assert_ended_held("The app's proxy", lab.children(*app));
    }
    assert_ended_held("A process below a desktop-app proxy", &leaves.0);
    assert_ended_held("The orphaned proxy", &orphan.0);
    assert!(lab.alive(other_ssh), "Another lease's client was ended");
    assert!(lab.alive(other_proxy), "Another lease's proxy was ended");
    rel.assert_released(since, until, 4);
}

/// A client that is starting children while it is ended: a desktop app's proxy that starts what it
/// runs again and again (a connection being made, a retry). Each tree is walked again for a child
/// made meanwhile, so when the release returns, the job holds no running process
/// (driver.md, "Each with its tree", "After the ends, it waits for the processes to exit, and walks
/// each tree again for a child made meanwhile").
#[test]
fn a_clients_tree_that_is_still_growing_is_ended_whole() {
    let rel = Rel::ready();
    let mut lab = Lab::new();
    let job = Job::new();
    let a = rel.alias.clone();
    let pids = lab.dir.join("leaves-0.txt");
    let own = lab.dir.join("own-0.txt");
    let go = lab.dir.join("go-0.txt");
    let file = lab.dir.join("ccd-ssh-0a1b2c3d").join("proxy-command.txt");
    let plugin = lab.copy_of("session-manager-plugin.exe");
    let (node_exe, node_args, mut env) = node(
        &lab,
        "claude-ssh-proxy.exe",
        &pids,
        &[(plugin, vec![])],
        true,
        &file,
    );
    for (name, value) in [
        (NODE_SELF, own.to_string_lossy().into_owned()),
        (NODE_GO, go.to_string_lossy().into_owned()),
        (NODE_CAP, "150".to_string()),
    ] {
        env.push((name.to_string(), value));
    }
    let args = desktop_args(&node_exe, &file, &a);
    let client = lab.start_with(
        "ssh.exe",
        &refs(&args),
        Opts {
            children: vec![(node_exe, node_args)],
            env,
            ..Opts::default()
        },
    );
    adopt(&job, &own, &go);
    wait_started(&pids, 5);
    assert!(
        job.active() >= 5,
        "The node's leaves run before the release"
    );

    let (since, until) = {
        let since = unix_now();
        rel.release(&lab).expect("The release succeeds");
        (since, unix_now())
    };
    assert_eq!(
        job.active(),
        0,
        "A process the client's tree grew is still running when the release returned"
    );
    assert_ended(&mut lab, client, "The client");
    rel.assert_released(since, until, 1);
}

/// A desktop app that reconnects: it starts an `agentspace-ssh-proxy.exe <alias>` again and again
/// for as long as the alias file exists, as an app does that connects through the alias, and each
/// start takes a moment. The release deletes the alias file, so the app starts no more, and
/// driver.md's "Release a lease" says that no process holding the alias that its walks can end is
/// left running when it returns: every proxy running when its last walk read the process list
/// (D59, as changed for v0.10-T03; #0196 saw a proxy running one second after a release that
/// returned in 0.1 seconds). **A proxy started after that read is outside the rule's claim**
/// (#0229), so the app ends a proxy whose start returned after the alias file was gone itself, and
/// the case asserts nothing of it: a proxy the app found the alias file still there for was
/// running before the file's deletion, so before the release's second walk, and the release MUST
/// have ended it.
#[test]
fn a_reconnecting_apps_proxies_are_all_gone_once_its_alias_is_gone() {
    let rel = Rel::ready();
    let mut lab = Lab::new();
    let job = Job::new();
    let a = rel.alias.clone();
    let alias_file = rel.fx.alias_dir().join(&rel.alias);
    assert!(alias_file.is_file(), "The lease's alias file exists");
    let pids = lab.dir.join("leaves-0.txt");
    let own = lab.dir.join("own-0.txt");
    let go = lab.dir.join("go-0.txt");
    let file = lab.dir.join("ccd-ssh-0a1b2c3d").join("proxy-command.txt");
    let proxy = lab.copy_of("agentspace-ssh-proxy.exe");
    let (_, node_args, mut env) = node(
        &lab,
        "claude-app.exe",
        &pids,
        &[(proxy, vec![a])],
        false,
        &file,
    );
    for (name, value) in [
        (NODE_SELF, own.to_string_lossy().into_owned()),
        (NODE_GO, go.to_string_lossy().into_owned()),
        (NODE_WATCH, alias_file.to_string_lossy().into_owned()),
        (NODE_CAP, "150".to_string()),
    ] {
        env.push((name.to_string(), value));
    }
    // The app is no client by its image, and stops by itself once the alias file is gone.
    let app = lab.start_with(
        "claude-app.exe",
        &refs(&node_args),
        Opts {
            env,
            ..Opts::default()
        },
    );
    adopt(&job, &own, &go);
    wait_started(&pids, 5);

    let (since, until) = {
        let since = unix_now();
        rel.release(&lab).expect("The release succeeds");
        (since, unix_now())
    };
    // The app has no alias to connect through, and ends by itself within moments.
    wait_for(|| !lab.alive(app), "The app's end, once the alias is gone");
    assert_eq!(
        job.active(),
        0,
        "A proxy holding the alias is still running after the release and the app's last start"
    );
    assert!(
        job.total() >= 6,
        "The app started proxies before and during the release"
    );
    // How many clients the release ended depends on when the app started each, so only its form is
    // asserted: one line, the first part of the text driver.md gives.
    assert!(!alias_file.exists());
    assert!(!rel.fx.lease_dir(&rel.id).exists());
    let log = fs::read_to_string(rel.fx.ctx.state_dir.join("release.log")).expect("The log");
    assert_eq!(log.lines().count(), 1, "The release appends one line");
    assert!(
        log.contains(&format!(" lease={} Ended ", rel.id)),
        "The line counts the connections ended"
    );
    let time = parse_utc(&log[..20]).expect("A line's time");
    assert!(time >= since - 1 && time <= until + 1);
}
