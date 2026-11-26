//! Black-box tests of the ProxyCommand's refusal of a lease alias whose lease is not `ready`
//! (docs/operator.md, "From the ProxyCommand", step 3, and "A lease that is not ready
//! (#0229)"; E0.11-T02-S04, #0237), written from that page and docs/state.md alone.
//!
//! Every run happens in a throwaway home under Cargo's test folder. The AWS CLI, ssh and scp are
//! stand-in `.cmd` files named through the test seam, and each logs every call it gets. Nothing
//! reaches a real host or real AWS. Every run is bounded in time.
//!
//! The retry tests hold the lease state file open without sharing it and count the retry lines on
//! stderr. No test races a timer, and none replaces a file on one.

#![cfg(windows)]

use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const BOX_ID: &str = "i-1234567890abcdef0";
const HOST_ID: &str = "i-abcdef12345678901";
/// The project of every lease here, and the first 8 hex digits of its ID, which end its alias.
const PROJECT: &str = "orchard";
const PREFIX: &str = "aaaaaaaa";
const ALIAS: &str = "agentspace-orchard-aaaaaaaa";
const PAST: &str = "2020-01-01T00:00:00Z";
const FUTURE: &str = "2999-01-01T00:00:00Z";
const GARBAGE: &str = "garbage, not json";
const READY: &str = "The host is ready.";
const RETRY: &str = "agentspace: The lease records were held by another program; trying again.";
const PENDING: &str = "is pending, not yet ready; no connection was opened. Connect once 'agentspace lease get' answers ready.";
/// No run lasts longer than LIMIT, and a run that holds the lease state file ends within BOUND.
const LIMIT: Duration = Duration::from_secs(30);
const BOUND: Duration = Duration::from_secs(10);
/// Two pauses of 400 milliseconds come before a read's third attempt (pin P27).
const PAUSED: Duration = Duration::from_millis(700);

/// What a record adds in each state, as the state schema allows it (state.md).
const RELEASED_AT: &str = r#","released_at":"2026-01-01T00:30:00Z""#;
const NO_CAPACITY: &str = r#","reasons":[{"code":"no_capacity","message":"No capacity."}]"#;
const TOO_LARGE: &str = r#","reasons":[{"code":"plan_too_large","message":"Too large."}],"sized_for":{"turn":"t1","agents":33},"plan":{"turns":[{"name":"t1","stories":[{"name":"s1","subagents":[{"name":"a","model":"Opus","effort":"high"}]}]}]}"#;
/// What the stand-in `aws` answers `ec2 describe-instances` with: a host that has run for long.
const DESCRIBE: &str = r#"echo {"Reservations":[{"Instances":[{"State":{"Name":"running"},"LaunchTime":"2020-01-01T00:00:00+00:00"}]}]}"#;

type Buffer = Arc<Mutex<Vec<u8>>>;
type Snapshot = Vec<(String, Option<Vec<u8>>)>;

/// A throwaway home, and the stand-ins' folder, where `calls.log` collects every call.
struct World {
    root: PathBuf,
    home: PathBuf,
    state: PathBuf,
    standins: PathBuf,
}

/// A stand-in `.cmd` file: these lines, each ended as a batch file's are.
fn batch(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("{l}\r\n")).collect()
}

impl World {
    fn new(name: &str) -> World {
        let dir = format!("not-ready-{name}-{}", std::process::id());
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(dir);
        let _ = fs::remove_dir_all(&root);
        let w = World {
            home: root.join("home"),
            state: root.join("home").join(".agentspace"),
            standins: root.join("standins"),
            root,
        };
        fs::create_dir_all(&w.state).expect("create a throwaway home");
        for folder in ["tmp", "localapp", "roaming", "standins"] {
            fs::create_dir_all(w.root.join(folder)).expect("create a throwaway folder");
        }
        let aws = batch(&[
            "@echo off",
            ">>\"%~dp0calls.log\" echo aws %*",
            "if \"%~1\"==\"ssm\" if \"%~2\"==\"start-session\" exit /b 0",
            "if \"%~1\"==\"ec2\" if \"%~2\"==\"describe-instances\" goto describe",
            "exit /b 9",
            ":describe",
            DESCRIBE,
            "exit /b 0",
        ]);
        fs::write(w.standins.join("aws.cmd"), aws).expect("write the stand-in aws");
        for tool in ["ssh", "scp"] {
            let log = format!(">>\"%~dp0calls.log\" echo {tool} %*");
            let script = batch(&["@echo off", log.as_str(), "exit /b 0"]);
            fs::write(w.standins.join(format!("{tool}.cmd")), script).expect("write a stand-in");
        }
        w
    }

    fn leases_path(&self) -> PathBuf {
        self.state.join("leases.json")
    }

    /// Writes the host registry and, when given, the lease state file.
    fn seed(&self, registry: &str, leases: Option<String>) {
        fs::write(self.state.join("hosts.json"), registry).expect("write the host registry");
        if let Some(text) = leases {
            fs::write(self.leases_path(), text).expect("write the lease state file");
        }
    }

    /// Every call a stand-in logged, one string for each.
    fn calls(&self) -> Vec<String> {
        let text = fs::read_to_string(self.standins.join("calls.log")).unwrap_or_default();
        text.lines().map(|l| l.trim().to_string()).collect()
    }

    /// The calls that start the tunnel.
    fn tunnels(&self) -> Vec<String> {
        let mut found = self.calls();
        found.retain(|c| c.contains("start-session"));
        found
    }

    /// The state folder: each file's name and bytes, and a folder's name alone.
    fn snapshot(&self) -> Snapshot {
        let mut files = Snapshot::new();
        let listing = fs::read_dir(&self.state).expect("list the state folder");
        for entry in listing.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            files.push((name, fs::read(entry.path()).ok()));
        }
        files.sort();
        files
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Reads a pipe into a buffer on a thread of its own, until the pipe closes.
fn pump(mut pipe: impl Read + Send + 'static) -> (Buffer, JoinHandle<()>) {
    let buffer = Buffer::default();
    let sink = buffer.clone();
    let reader = thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&chunk[..n]);
        }
    });
    (buffer, reader)
}

fn utf8(buffer: &Buffer) -> String {
    String::from_utf8_lossy(&buffer.lock().unwrap()).into_owned()
}

/// A started ProxyCommand, its output collected while it runs.
struct Live {
    child: Child,
    out: Buffer,
    err: Buffer,
    pumps: Vec<JoinHandle<()>>,
    begun: Instant,
}

/// What a finished run did.
struct Done {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: String,
    elapsed: Duration,
}

impl Done {
    fn lines(&self) -> Vec<&str> {
        self.stderr.lines().collect()
    }

    fn last_line(&self) -> &str {
        self.stderr.lines().last().unwrap_or_default()
    }
}

/// How many of stderr's lines are the retry line.
fn retries(stderr: &str) -> usize {
    stderr.lines().filter(|l| *l == RETRY).count()
}

/// Starts the ProxyCommand with this one argument, in the world's throwaway home.
fn start(w: &World, arg: &str, diagnostic: bool) -> Live {
    let root = std::env::var_os("SystemRoot").expect("SystemRoot is set");
    let system = PathBuf::from(root);
    let system32 = system.join("System32");
    let path = format!("{};{}", system32.display(), system.display());
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agentspace-ssh-proxy"));
    cmd.arg(arg)
        .env_clear()
        .current_dir(&w.root)
        .env("SystemRoot", &system)
        .env("windir", &system)
        .env("ComSpec", system32.join("cmd.exe"))
        .env("PATH", path)
        .env("TEMP", w.root.join("tmp"))
        .env("TMP", w.root.join("tmp"))
        .env("USERPROFILE", &w.home)
        .env("HOME", &w.home)
        .env("LOCALAPPDATA", w.root.join("localapp"))
        .env("APPDATA", w.root.join("roaming"))
        .env("AGENTSPACE_AWS_CMD", w.standins.join("aws.cmd"))
        .env("AGENTSPACE_SSH_CMD", w.standins.join("ssh.cmd"))
        .env("AGENTSPACE_SCP_CMD", w.standins.join("scp.cmd"))
        .env("AGENTSPACE_TIMEZONE", "America/New_York")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if diagnostic {
        cmd.env("AGENTSPACE_DIAGNOSTIC", "1");
    }
    let begun = Instant::now();
    let mut child = cmd.spawn().expect("start agentspace-ssh-proxy");
    let (out, out_reader) = pump(child.stdout.take().expect("piped stdout"));
    let (err, err_reader) = pump(child.stderr.take().expect("piped stderr"));
    Live {
        child,
        out,
        err,
        pumps: vec![out_reader, err_reader],
        begun,
    }
}

impl Live {
    fn stderr(&self) -> String {
        utf8(&self.err)
    }

    fn ended(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().expect("poll the program")
    }

    /// Ends the program and fails the test once it has run for longer than LIMIT.
    fn give_up_after_limit(&mut self) {
        if self.begun.elapsed() > LIMIT {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let stderr = self.stderr();
            panic!("the program outlasted {LIMIT:?}; stderr:\n{stderr}");
        }
    }

    /// Waits until a retry line has reached stderr. False when the program ended first.
    fn wait_for_retry_line(&mut self) -> bool {
        while retries(&self.stderr()) == 0 {
            if self.ended().is_some() {
                return false;
            }
            self.give_up_after_limit();
            thread::sleep(Duration::from_millis(5));
        }
        true
    }

    /// Waits for the program to end, and returns what it did.
    fn finish(mut self) -> Done {
        let status = loop {
            if let Some(status) = self.ended() {
                break status;
            }
            self.give_up_after_limit();
            thread::sleep(Duration::from_millis(10));
        };
        let elapsed = self.begun.elapsed();
        for reader in self.pumps.drain(..) {
            let _ = reader.join();
        }
        Done {
            code: status.code(),
            stdout: self.out.lock().unwrap().clone(),
            stderr: self.stderr(),
            elapsed,
        }
    }
}

/// A lease record in the state file's form (state.md, The lease state file). `state` is a
/// stored state, or `no_capacity` or `plan_too_large` for an `unsatisfiable` one.
fn lease(prefix: &str, n: u8, state: &str, expires_at: Option<&str>) -> String {
    let big = state == "plan_too_large";
    let stored = if big || state == "no_capacity" {
        "unsatisfiable"
    } else {
        state
    };
    let size = if big { "xl" } else { "s" };
    let mut s = format!(
        "{{\"id\":\"{prefix}-5e1f-4c2a-9b7d-0123456789a{n:x}\",\"project\":\"{PROJECT}\",\"state\":\"{stored}\",\"platform\":\"linux/arm64\",\"profile\":\"base\",\"size\":\"{size}\",\"ttl_seconds\":3600,\"created_at\":\"2026-01-01T00:00:00Z\",\"driver\":\"ec2\""
    );
    if let Some(at) = expires_at {
        s.push_str(&format!(",\"expires_at\":\"{at}\""));
    }
    let alias = format!("agentspace-{PROJECT}-{prefix}");
    let ssh = format!(r#","ssh":{{"host_alias":"{alias}","user":"ubuntu"}}"#);
    s.push_str(match state {
        "ready" => ssh.as_str(),
        "released" => RELEASED_AT,
        "no_capacity" => NO_CAPACITY,
        "plan_too_large" => TOO_LARGE,
        _ => "",
    });
    s.push('}');
    s
}

/// The lease state file holding these records.
fn file(records: &[String]) -> String {
    format!("{{\"version\":0,\"leases\":[{}]}}\n", records.join(","))
}

/// One host record: the box's, or another project's.
fn host_record(project: &str, stack: &str, id: &str) -> String {
    format!(
        "{{\"project\":\"{project}\",\"stack\":\"{stack}\",\"state\":\"ready\",\"instance_id\":\"{id}\"}}"
    )
}

/// The host registry: the box's record, and with `host` the project's ready host.
fn registry(host: bool) -> String {
    let mut records = vec![host_record("swimcoach", "agentbox", BOX_ID)];
    if host {
        records.push(host_record(PROJECT, "agentspace-host-orchard", HOST_ID));
    }
    format!("{{\"version\":0,\"hosts\":[{}]}}\n", records.join(","))
}

/// The refusal's line for a lease in this state (operator.md, A lease that is not ready).
fn refusal(alias: &str, state: &str) -> String {
    let create = "; no connection was opened. Create a new lease.";
    let words = match state {
        "released" => format!("has been released{create}"),
        "expired" => format!("has expired{create}"),
        "unsatisfiable" => format!("is unsatisfiable{create}"),
        "pending" => PENDING.to_string(),
        _ => panic!("no refusal is written for {state}"),
    };
    format!("agentspace: The lease for {alias} {words}")
}

/// The line for a lease-form argument while the lease records cannot be read or are not valid.
fn unreadable(alias: &str) -> String {
    format!(
        "agentspace: The lease records could not be read; no connection was opened for {alias}. Run 'agentspace lease list' to see the error."
    )
}

/// A refusal: exit 1, nothing on stdout, this one line on stderr, no program started (so no AWS
/// call, tunnel, ssh or scp), and no file in the state folder changed or made, the lock included.
fn assert_refused(w: &World, done: &Done, before: &Snapshot, line: &str) {
    assert_eq!(done.code, Some(1), "stderr:\n{}", done.stderr);
    assert!(done.stdout.is_empty(), "stdout holds nothing");
    assert_eq!(done.stderr, format!("{line}\n"));
    assert!(w.calls().is_empty(), "a refusal starts no program");
    assert_unchanged(w, before);
}

fn assert_unchanged(w: &World, before: &Snapshot) {
    assert!(
        w.snapshot() == *before,
        "a file in the state folder was changed or made"
    );
}

/// A connection: exit 0, nothing on stdout, one tunnel to this instance and no other host's,
/// no ssh and no scp, and, when `strict`, the ready line alone on stderr.
fn assert_connected(w: &World, done: &Done, id: &str, strict: bool) {
    assert_eq!(done.code, Some(0), "stderr:\n{}", done.stderr);
    assert!(done.stdout.is_empty(), "stdout holds nothing");
    if strict {
        assert_eq!(done.stderr, format!("{READY}\n"));
    }
    let tunnels = w.tunnels();
    assert_eq!(tunnels.len(), 1, "one tunnel; calls: {:?}", w.calls());
    assert!(tunnels[0].contains(&format!("--target {id} ")));
    let other = if id == HOST_ID { BOX_ID } else { HOST_ID };
    for call in w.calls() {
        assert!(call.starts_with("aws "), "ssh and scp are never started");
        assert!(!call.contains(other), "a call names the other host");
    }
}

/// Three attempts with two 400 millisecond pauses took 700 milliseconds or more, and under BOUND.
fn assert_paused(done: &Done) {
    assert!(done.elapsed >= PAUSED, "{:?}", done.elapsed);
    assert!(done.elapsed < BOUND, "{:?}", done.elapsed);
}

/// The read met a failure that is no sharing violation: one attempt, no retry line, a refusal.
fn assert_refused_after_one_attempt(done: &Done) {
    assert_eq!(done.code, Some(1), "stderr:\n{}", done.stderr);
    assert_eq!(retries(&done.stderr), 0, "stderr:\n{}", done.stderr);
    assert_eq!(done.last_line(), unreadable(ALIAS));
    assert!(done.elapsed < BOUND, "{:?}", done.elapsed);
}

/// Holds the lease state file open without sharing it, as another program would, and checks
/// that another open of it fails with a sharing violation, OS error 32.
fn hold(w: &World) -> File {
    let held = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(w.leases_path())
        .expect("hold the lease state file");
    let other = File::open(w.leases_path()).expect_err("an open while held must fail");
    assert_eq!(other.raw_os_error(), Some(32), "a sharing violation");
    held
}

/// Runs with AGENTSPACE_DIAGNOSTIC set, after `leases` sets up the lease state file.
fn diagnostic_run(name: &str, leases: impl FnOnce(&World)) -> (World, Done) {
    let w = World::new(name);
    w.seed(&registry(true), None);
    leases(&w);
    let done = start(&w, ALIAS, true).finish();
    (w, done)
}

#[test]
fn each_lease_that_is_not_ready_is_refused_before_anything_starts() {
    let cases = [
        ("released", lease(PREFIX, 1, "released", Some(PAST))),
        ("released", lease(PREFIX, 1, "released", None)),
        ("expired", lease(PREFIX, 1, "ready", Some(PAST))),
        ("unsatisfiable", lease(PREFIX, 1, "no_capacity", None)),
        ("unsatisfiable", lease(PREFIX, 1, "plan_too_large", None)),
        ("pending", lease(PREFIX, 1, "pending", None)),
    ];
    // The registry is garbage once, so a refusal that came after reading it would be another
    // line, and ready once. AGENTSPACE_DIAGNOSTIC adds no line to a refusal of a record read.
    for (i, (state, record)) in cases.iter().enumerate() {
        for (j, reg) in [GARBAGE.to_string(), registry(true)].iter().enumerate() {
            for diagnostic in [false, true] {
                let w = World::new(&format!("states-{i}-{j}-{diagnostic}"));
                w.seed(reg, Some(file(&[record.clone()])));
                let before = w.snapshot();
                let done = start(&w, ALIAS, diagnostic).finish();
                assert_refused(&w, &done, &before, &refusal(ALIAS, state));
            }
        }
    }
}

#[test]
fn a_ready_lease_still_connects_to_its_projects_host_or_the_box() {
    let ready = file(&[lease(PREFIX, 1, "ready", Some(FUTURE))]);
    // The project's host record is ready: that host. The project has none: the box.
    for (name, host, id) in [("host", true, HOST_ID), ("box", false, BOX_ID)] {
        let w = World::new(&format!("ready-{name}"));
        w.seed(&registry(host), Some(ready.clone()));
        let before = w.snapshot();
        let done = start(&w, ALIAS, false).finish();
        assert_connected(&w, &done, id, true);
        assert_unchanged(&w, &before);
    }
}

#[test]
fn a_record_is_found_by_the_alias_built_from_it_and_by_its_stored_host_alias() {
    // The stored `ssh.host_alias` is not the alias the record's project and ID build.
    let stored = "agentspace-orchard-bbbbbbbb";
    for (i, expires) in [PAST, FUTURE].into_iter().enumerate() {
        let record = lease(PREFIX, 1, "ready", Some(expires)).replace(ALIAS, stored);
        for (j, arg) in [ALIAS, stored].into_iter().enumerate() {
            let w = World::new(&format!("found-{i}-{j}"));
            w.seed(&registry(true), Some(file(&[record.clone()])));
            let before = w.snapshot();
            let done = start(&w, arg, false).finish();
            if expires == PAST {
                assert_refused(&w, &done, &before, &refusal(arg, "expired"));
            } else {
                assert_connected(&w, &done, HOST_ID, true);
            }
        }
    }
}

#[test]
fn two_records_of_one_alias_are_decided_by_all_of_them() {
    let released = |n| lease(PREFIX, n, "released", Some(PAST));
    let pending = |n| lease(PREFIX, n, "pending", None);
    let ready = |n| lease(PREFIX, n, "ready", Some(FUTURE));
    // Stored `ready` past its expiry is `expired` at the one reading of the clock.
    let stale = |n| lease(PREFIX, n, "ready", Some(PAST));
    // The first record that is ready connects. With none ready, the last record's state is named.
    let cases = [
        (vec![released(1), ready(2)], None),
        (vec![ready(1), released(2)], None),
        (vec![stale(1), ready(2)], None),
        (vec![ready(1), stale(2)], None),
        (vec![released(1), pending(2)], Some("pending")),
        (vec![pending(1), released(2)], Some("released")),
        (vec![released(1), stale(2)], Some("expired")),
        (vec![stale(1), released(2)], Some("released")),
    ];
    for (i, (records, refused)) in cases.iter().enumerate() {
        let w = World::new(&format!("two-{i}"));
        w.seed(&registry(true), Some(file(records)));
        let before = w.snapshot();
        let done = start(&w, ALIAS, false).finish();
        match refused {
            Some(state) => assert_refused(&w, &done, &before, &refusal(ALIAS, state)),
            None => assert_connected(&w, &done, HOST_ID, true),
        }
    }
}

#[test]
fn only_an_argument_of_lease_alias_form_is_refused_while_the_records_cannot_be_read() {
    let one = lease(PREFIX, 1, "ready", Some(FUTURE));
    let broken = [
        GARBAGE.to_string(),
        r#"{"version":1,"leases":[]}"#.to_string(),
        file(&[one.clone(), one]),
        r#"{"version":0,"leases":[{"id":"x"}]}"#.to_string(),
    ];
    // Lease alias form: `agentspace-`, a project of 1 to 32 characters, `-`, 8 lowercase hex.
    let long = format!("agentspace-{}-aaaaaaaa", "p".repeat(32));
    let refused = [ALIAS, "agentspace-my-app-0123abcd", long.as_str()];
    // Not of that form: 7 or 9 digits, upper case, a digit that is not hex, no project, or a
    // project of 33 characters. Such an argument reaches the box, as before v0.11.
    let too_long = format!("agentspace-{}-aaaaaaaa", "p".repeat(33));
    let boxed = [
        "agentspace-orchard-aaaaaaa",
        "agentspace-orchard-aaaaaaaaa",
        "agentspace-orchard-AAAAAAAA",
        "agentspace-Orchard-aaaaaaaa",
        "agentspace-orchard-aaaaaaag",
        "agentspace-aaaaaaaa",
        too_long.as_str(),
    ];
    for (i, text) in broken.iter().enumerate() {
        for (j, arg) in refused.iter().enumerate() {
            let w = World::new(&format!("form-refused-{i}-{j}"));
            w.seed(&registry(true), Some(text.clone()));
            let before = w.snapshot();
            let done = start(&w, arg, false).finish();
            assert_refused(&w, &done, &before, &unreadable(arg));
        }
        // No lease's alias is `agentspace`, so whatever is wrong with the file it reaches the box.
        let w = World::new(&format!("form-agentspace-{i}"));
        w.seed(&registry(true), Some(text.clone()));
        let done = start(&w, "agentspace", false).finish();
        assert_connected(&w, &done, BOX_ID, true);
    }
    for (j, arg) in boxed.iter().enumerate() {
        let w = World::new(&format!("form-boxed-{j}"));
        w.seed(&registry(true), Some(GARBAGE.to_string()));
        let done = start(&w, arg, false).finish();
        assert_connected(&w, &done, BOX_ID, true);
    }
}

#[test]
fn an_alias_that_is_no_records_alias_and_a_missing_file_reach_the_box() {
    let other = [
        lease("bbbbbbbb", 1, "released", Some(PAST)),
        lease("bbbbbbbb", 2, "ready", Some(FUTURE)),
    ];
    let cases = [None, Some(file(&[])), Some(file(&other))];
    for (i, leases) in cases.into_iter().enumerate() {
        let w = World::new(&format!("no-record-{i}"));
        w.seed(&registry(true), leases);
        let before = w.snapshot();
        let done = start(&w, ALIAS, false).finish();
        assert_connected(&w, &done, BOX_ID, true);
        assert_unchanged(&w, &before);
    }
}

#[test]
fn a_failure_that_is_not_a_sharing_violation_gets_no_retry() {
    let (_w, done) = diagnostic_run("no-retry-folder", |w| {
        fs::create_dir(w.leases_path()).expect("make a folder at the file's path");
    });
    assert_refused_after_one_attempt(&done);
    let (_w, done) = diagnostic_run("no-retry-invalid", |w| {
        fs::write(w.leases_path(), GARBAGE).expect("write an invalid file");
    });
    assert_refused_after_one_attempt(&done);
    // A missing file is no failure at all: the box.
    let (w, done) = diagnostic_run("no-retry-missing", |_| {});
    assert_connected(&w, &done, BOX_ID, false);
    assert_eq!(retries(&done.stderr), 0, "stderr:\n{}", done.stderr);
}

#[test]
fn a_hold_that_outlasts_the_read_gets_two_retry_lines_and_the_refusal() {
    let w = World::new("held-through");
    let ready = lease(PREFIX, 1, "ready", Some(FUTURE));
    w.seed(&registry(true), Some(file(&[ready])));
    let held = hold(&w);
    let before = w.snapshot();
    // With AGENTSPACE_DIAGNOSTIC: a line for each of the two retries, the line naming the file,
    // and the refusal's, last. The read pauses 400 milliseconds before each retry.
    let done = start(&w, ALIAS, true).finish();
    let lines = done.lines();
    assert_eq!(done.code, Some(1), "stderr:\n{}", done.stderr);
    assert_eq!(lines.len(), 4, "stderr:\n{}", done.stderr);
    assert_eq!(lines[..2], [RETRY, RETRY]);
    assert!(lines[2].contains("leases.json"), "{lines:?}");
    assert_eq!(lines[3], unreadable(ALIAS));
    assert_paused(&done);
    assert!(w.calls().is_empty(), "a refusal starts no program");
    // Without it, the refusal alone, after the same pauses.
    let done = start(&w, ALIAS, false).finish();
    assert_refused(&w, &done, &before, &unreadable(ALIAS));
    assert_paused(&done);
    drop(held);
}

#[test]
fn a_hold_released_on_the_first_retry_line_lets_the_connection_through() {
    let w = World::new("released-on-retry");
    let ready = lease(PREFIX, 1, "ready", Some(FUTURE));
    w.seed(&registry(true), Some(file(&[ready])));
    let held = hold(&w);
    let mut live = start(&w, ALIAS, true);
    // The hold ends when the first retry line has reached stderr, never on a timer.
    assert!(live.wait_for_retry_line(), "no retry line");
    drop(held);
    let done = live.finish();
    let n = retries(&done.stderr);
    assert!((1..=2).contains(&n), "{n} retries");
    assert!(done.elapsed < BOUND, "{:?}", done.elapsed);
    assert_connected(&w, &done, HOST_ID, false);
}
