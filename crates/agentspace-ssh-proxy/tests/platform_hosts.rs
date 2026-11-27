//! Black-box tests of the ProxyCommand's choice of a host by its project and its platform
//! (docs/operator.md, "From the ProxyCommand", step 2 and step 3's host rules, "A host's platform
//! and its names"; D91; E0.12-T03-S04, #0257), written from those pages and docs/state.md alone.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Every registry here holds two or three hosts of ONE project, `orchard`, one on each platform,
//! so a lookup by the project alone reaches the wrong host for at least one lease or alias. Each
//! registry is also run in both record orders, so a lookup that takes the first record of the
//! project fails whichever platform's record comes first.
//!
//! Every run happens in a throwaway home under Cargo's test folder. The AWS CLI, ssh and scp are
//! stand-in `.cmd` files named through the test seam, and each logs every call it gets. Nothing
//! reaches a real host or real AWS. Every run is bounded in time. The environment is cleared for
//! the program, and no test sets a process variable or prints an environment's value.

#![cfg(windows)]

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The box's instance, and one instance for each host of the project `orchard`.
const BOX_ID: &str = "i-1234567890abcdef0";
const ARM_ID: &str = "i-abcdef12345678901";
const WIN_ID: &str = "i-11112222333344445";
const AMD_ID: &str = "i-aaaabbbbccccddddf";
const PROJECT: &str = "orchard";
const COULD_NOT_REACH: &str = "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.";
const READY: &str = "The host is ready.";
const FUTURE: &str = "2999-01-01T00:00:00Z";
const PAST: &str = "2020-01-01T00:00:00Z";
/// What the stand-in `aws` answers `ec2 describe-instances` with: a host that has run for long.
const DESCRIBE: &str = r#"echo {"Reservations":[{"Instances":[{"State":{"Name":"running"},"LaunchTime":"2020-01-01T00:00:00+00:00"}]}]}"#;
const LIMIT: Duration = Duration::from_secs(30);

type Buffer = Arc<Mutex<Vec<u8>>>;

fn batch(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("{l}\r\n")).collect()
}

/// A throwaway home, and the stand-ins' folder, where `calls.log` collects every call.
struct World {
    root: PathBuf,
    home: PathBuf,
    state: PathBuf,
    standins: PathBuf,
}

impl World {
    fn new(name: &str) -> World {
        let dir = format!("platform-hosts-{name}-{}", std::process::id());
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
            "if \"%~1\"==\"cloudformation\" if \"%~2\"==\"describe-stacks\" goto stacks",
            "exit /b 9",
            ":describe",
            DESCRIBE,
            "exit /b 0",
            ":stacks",
            // Only the stack-read test reaches here: it answers one new instance for any stack.
            &format!(
                "echo {{\"Stacks\":[{{\"Outputs\":[{{\"OutputKey\":\"InstanceId\",\"OutputValue\":\"{NEW_ID}\"}}]}}]}}"
            ),
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

    /// Writes the host registry and, when given, the lease state file.
    fn seed(&self, registry: &str, leases: Option<String>) {
        fs::write(self.state.join("hosts.json"), registry).expect("write the host registry");
        if let Some(text) = leases {
            fs::write(self.state.join("leases.json"), text).expect("write the lease state file");
        }
    }

    fn calls(&self) -> Vec<String> {
        let text = fs::read_to_string(self.standins.join("calls.log")).unwrap_or_default();
        text.lines().map(|l| l.trim().to_string()).collect()
    }

    fn tunnels(&self) -> Vec<String> {
        let mut found = self.calls();
        found.retain(|c| c.contains("start-session"));
        found
    }

    /// The instance ID the one tunnel went to.
    fn target(&self) -> Option<String> {
        let tunnels = self.tunnels();
        let call = tunnels.first()?;
        let at = call.find("--target ")? + "--target ".len();
        call[at..].split_whitespace().next().map(str::to_string)
    }

    /// The state folder: each file's name and bytes, sorted.
    fn snapshot(&self) -> Vec<(String, Option<Vec<u8>>)> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.state)
            .expect("list the state folder")
            .flatten()
        {
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

/// What a finished run did.
struct Done {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: String,
}

impl Done {
    fn lines(&self) -> Vec<&str> {
        self.stderr.lines().collect()
    }
}

/// Runs the ProxyCommand with this one argument in the world's throwaway home, to its end.
fn run(w: &World, arg: &str, diagnostic: bool) -> Done {
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
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll the program") {
            break status;
        }
        if begun.elapsed() > LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the program outlasted {LIMIT:?}");
        }
        thread::sleep(Duration::from_millis(10));
    };
    let _ = out_reader.join();
    let _ = err_reader.join();
    let stdout = out.lock().unwrap().clone();
    let stderr = String::from_utf8_lossy(&err.lock().unwrap()).into_owned();
    Done {
        code: status.code(),
        stdout,
        stderr,
    }
}

/// The instance the stand-in stack answer names, which no host record holds.
const NEW_ID: &str = "i-99998888777766665";

// ---------------------------------------------------------------------------------------------
// Records.
// ---------------------------------------------------------------------------------------------

/// A host record as state.md writes it: `platform` after `project` for any platform but
/// `linux/arm64`, which carries none.
fn host_record(
    project: &str,
    platform: Option<&str>,
    stack: &str,
    state: &str,
    id: Option<&str>,
) -> String {
    let mut s = format!("{{\"project\":\"{project}\"");
    if let Some(p) = platform {
        s.push_str(&format!(",\"platform\":\"{p}\""));
    }
    s.push_str(&format!(",\"stack\":\"{stack}\",\"state\":\"{state}\""));
    if let Some(id) = id {
        s.push_str(&format!(",\"instance_id\":\"{id}\""));
    }
    s.push('}');
    s
}

fn box_record() -> String {
    host_record("swimcoach", None, "agentbox", "ready", Some(BOX_ID))
}

fn arm_host(state: &str, id: Option<&str>) -> String {
    host_record(PROJECT, None, "agentspace-host-orchard", state, id)
}

fn win_host(state: &str, id: Option<&str>) -> String {
    host_record(
        PROJECT,
        Some("windows/amd64"),
        "agentspace-windows-amd64-host-orchard",
        state,
        id,
    )
}

fn amd_host(state: &str, id: Option<&str>) -> String {
    host_record(
        PROJECT,
        Some("linux/amd64"),
        "agentspace-linux-amd64-host-orchard",
        state,
        id,
    )
}

/// The registry: the box's record first, then these.
fn registry(hosts: &[String]) -> String {
    let mut records = vec![box_record()];
    records.extend(hosts.iter().cloned());
    format!("{{\"version\":0,\"hosts\":[{}]}}\n", records.join(","))
}

/// The lease alias of a lease with this prefix.
fn alias(prefix: &str) -> String {
    format!("agentspace-{PROJECT}-{prefix}")
}

/// A lease record in the state file's form, of `orchard` on `platform`.
fn lease(prefix: &str, n: u8, state: &str, platform: &str, expires_at: &str) -> String {
    let user = if platform.starts_with("windows") {
        "agent"
    } else {
        "ubuntu"
    };
    let mut s = format!(
        "{{\"id\":\"{prefix}-5e1f-4c2a-9b7d-0123456789a{n:x}\",\"project\":\"{PROJECT}\",\"state\":\"{state}\",\"platform\":\"{platform}\",\"profile\":\"base\",\"size\":\"s\",\"ttl_seconds\":3600,\"created_at\":\"2026-01-01T00:00:00Z\",\"driver\":\"ec2\",\"expires_at\":\"{expires_at}\""
    );
    match state {
        "ready" => s.push_str(&format!(
            ",\"ssh\":{{\"host_alias\":\"{}\",\"user\":\"{user}\"}}",
            alias(prefix)
        )),
        "released" => s.push_str(",\"released_at\":\"2026-01-01T00:30:00Z\""),
        _ => {}
    }
    s.push('}');
    s
}

fn ready_lease(prefix: &str, n: u8, platform: &str) -> String {
    lease(prefix, n, "ready", platform, FUTURE)
}

fn lease_file(records: &[String]) -> String {
    format!("{{\"version\":0,\"leases\":[{}]}}\n", records.join(","))
}

/// The three leases of `orchard`, one on each platform.
const WIN_PREFIX: &str = "aaaaaaaa";
const ARM_PREFIX: &str = "bbbbbbbb";
const AMD_PREFIX: &str = "cccccccc";

fn three_leases() -> String {
    lease_file(&[
        ready_lease(WIN_PREFIX, 1, "windows/amd64"),
        ready_lease(ARM_PREFIX, 2, "linux/arm64"),
        ready_lease(AMD_PREFIX, 3, "linux/amd64"),
    ])
}

/// The three hosts of `orchard`, ready, in each of the orders a registry can hold them in.
fn three_hosts_in_each_order() -> Vec<(&'static str, String)> {
    let a = arm_host("ready", Some(ARM_ID));
    let w = win_host("ready", Some(WIN_ID));
    let m = amd_host("ready", Some(AMD_ID));
    vec![
        ("arm-win-amd", registry(&[a.clone(), w.clone(), m.clone()])),
        ("win-amd-arm", registry(&[w.clone(), m.clone(), a.clone()])),
        ("amd-arm-win", registry(&[m, a, w])),
    ]
}

// ---------------------------------------------------------------------------------------------
// What a run must have done.
// ---------------------------------------------------------------------------------------------

/// A connection: exit 0, nothing on stdout, one tunnel to this instance, and no call that names
/// any other host's instance, and no ssh or scp.
fn assert_reached(w: &World, done: &Done, id: &str, why: &str) {
    assert_eq!(done.code, Some(0), "{why}; stderr:\n{}", done.stderr);
    assert!(done.stdout.is_empty(), "{why}: stdout holds nothing");
    assert_eq!(w.tunnels().len(), 1, "{why}; calls: {:?}", w.calls());
    assert_eq!(
        w.target().as_deref(),
        Some(id),
        "{why}: the tunnel's target"
    );
    for other in [BOX_ID, ARM_ID, WIN_ID, AMD_ID] {
        if other == id {
            continue;
        }
        for call in w.calls() {
            assert!(!call.contains(other), "{why}: a call names another host");
        }
    }
    for call in w.calls() {
        assert!(
            call.starts_with("aws "),
            "{why}: ssh and scp are never started"
        );
    }
    assert_eq!(done.lines().last().copied(), Some(READY), "{why}");
}

/// A refusal: exit 1, nothing on stdout, this one line on stderr and no more, no program started,
/// and no file in the state folder changed or made.
fn assert_refused(
    w: &World,
    done: &Done,
    before: &[(String, Option<Vec<u8>>)],
    line: &str,
    why: &str,
) {
    assert_eq!(done.code, Some(1), "{why}; stderr:\n{}", done.stderr);
    assert!(done.stdout.is_empty(), "{why}: stdout holds nothing");
    assert_eq!(done.stderr, format!("{line}\n"), "{why}");
    assert!(
        w.calls().is_empty(),
        "{why}: a refusal starts no program: {:?}",
        w.calls()
    );
    assert!(
        w.snapshot() == before,
        "{why}: a file in the state folder changed"
    );
}

fn no_ready_host_line(prefix: &str) -> String {
    format!(
        "agentspace: The lease for {} has no ready host on its platform; no connection was opened.",
        alias(prefix)
    )
}

// ---------------------------------------------------------------------------------------------
// The lease alias: the host is the record of the lease's project on the lease's platform.
// ---------------------------------------------------------------------------------------------

/// operator.md, From the ProxyCommand, step 3: the lease's host is the record of its project on
/// its platform. A `linux/arm64` and a `linux/amd64` lease of one project each reach their own
/// host, whatever order the registry holds the project's three records in, a Windows host's
/// among them.
#[test]
fn a_lease_reaches_the_host_of_its_projects_platform() {
    let cases = [(ARM_PREFIX, ARM_ID), (AMD_PREFIX, AMD_ID)];
    for (order, reg) in three_hosts_in_each_order() {
        for (prefix, id) in cases {
            let w = World::new(&format!("lease-{order}-{prefix}"));
            w.seed(&reg, Some(three_leases()));
            let done = run(&w, &alias(prefix), false);
            assert_reached(&w, &done, id, &format!("{prefix} in {order}"));
        }
    }
}

/// The story's done-when, with two hosts of one project on two platforms alone: a `linux/arm64`
/// and a `linux/amd64` lease, each reaching its own host, whichever record comes first.
#[test]
fn two_hosts_of_one_project_on_two_platforms_are_each_reached_by_their_own_lease() {
    let a = arm_host("ready", Some(ARM_ID));
    let m = amd_host("ready", Some(AMD_ID));
    for (order, reg) in [
        ("arm-first", registry(&[a.clone(), m.clone()])),
        ("amd-first", registry(&[m, a])),
    ] {
        let leases = lease_file(&[
            ready_lease(AMD_PREFIX, 3, "linux/amd64"),
            ready_lease(ARM_PREFIX, 2, "linux/arm64"),
        ]);
        let w = World::new(&format!("two-{order}-amd"));
        w.seed(&reg, Some(leases.clone()));
        let done = run(&w, &alias(AMD_PREFIX), false);
        assert_reached(&w, &done, AMD_ID, &format!("the amd64 lease, {order}"));
        let w = World::new(&format!("two-{order}-arm"));
        w.seed(&reg, Some(leases));
        let done = run(&w, &alias(ARM_PREFIX), false);
        assert_reached(&w, &done, ARM_ID, &format!("the arm64 lease, {order}"));
    }
}

/// The same for a `windows/amd64` lease and a `linux/arm64` one: each reaches its own host, the
/// Windows host's record coming first or last. **It holds only once the start-and-wait step runs
/// on a Windows host** (E0.12-T03-S03's platform-aware start, which this story's phase B moves
/// `up` to): until then a Windows host fails as a failed `up` does, whatever the lookup chose.
#[test]
fn a_windows_lease_reaches_the_windows_host_of_its_project() {
    let a = arm_host("ready", Some(ARM_ID));
    let win = win_host("ready", Some(WIN_ID));
    for (order, reg) in [
        ("arm-first", registry(&[a.clone(), win.clone()])),
        ("win-first", registry(&[win, a])),
    ] {
        let leases = lease_file(&[
            ready_lease(WIN_PREFIX, 1, "windows/amd64"),
            ready_lease(ARM_PREFIX, 2, "linux/arm64"),
        ]);
        let w = World::new(&format!("win-{order}-win"));
        w.seed(&reg, Some(leases.clone()));
        let done = run(&w, &alias(WIN_PREFIX), false);
        assert_reached(&w, &done, WIN_ID, &format!("the Windows lease, {order}"));
        let w = World::new(&format!("win-{order}-arm"));
        w.seed(&reg, Some(leases));
        let done = run(&w, &alias(ARM_PREFIX), false);
        assert_reached(&w, &done, ARM_ID, &format!("the arm64 lease, {order}"));
    }
}

/// operator.md, step 3, third bullet and pin P13: a `ready` lease on a platform other than
/// `linux/arm64` whose project has no `ready` record on that platform is refused with exit 1, no
/// tunnel and no program, and its one line on stderr, whether the record is missing or is in
/// another state, and even when the project holds a ready host on another platform.
#[test]
fn a_lease_of_another_platform_with_no_ready_host_is_refused_with_no_tunnel() {
    let arm = arm_host("ready", Some(ARM_ID));
    let cases: Vec<(&str, String, &str, &str)> = vec![
        // The Windows lease, the project holding a ready Linux host and nothing else.
        (
            "win-no-record",
            registry(std::slice::from_ref(&arm)),
            WIN_PREFIX,
            "windows/amd64",
        ),
        (
            "win-creating",
            registry(&[arm.clone(), win_host("creating", None)]),
            WIN_PREFIX,
            "windows/amd64",
        ),
        (
            "win-failed",
            registry(&[arm.clone(), win_host("failed", Some(WIN_ID))]),
            WIN_PREFIX,
            "windows/amd64",
        ),
        // The Windows lease, the project holding a ready host of the other Linux platform.
        (
            "win-with-amd",
            registry(&[arm.clone(), amd_host("ready", Some(AMD_ID))]),
            WIN_PREFIX,
            "windows/amd64",
        ),
        // The linux/amd64 lease, the project holding ready hosts on the other two platforms.
        (
            "amd-no-record",
            registry(&[arm.clone(), win_host("ready", Some(WIN_ID))]),
            AMD_PREFIX,
            "linux/amd64",
        ),
        (
            "amd-creating",
            registry(&[
                arm,
                amd_host("creating", None),
                win_host("ready", Some(WIN_ID)),
            ]),
            AMD_PREFIX,
            "linux/amd64",
        ),
    ];
    for (name, reg, prefix, platform) in cases {
        for diagnostic in [false, true] {
            let w = World::new(&format!("refuse-{name}-{diagnostic}"));
            let leases = lease_file(&[ready_lease(prefix, 1, platform)]);
            w.seed(&reg, Some(leases));
            let before = w.snapshot();
            let done = run(&w, &alias(prefix), diagnostic);
            // AGENTSPACE_DIAGNOSTIC adds no line to this refusal (operator.md, step 3).
            assert_refused(
                &w,
                &done,
                &before,
                &no_ready_host_line(prefix),
                &format!("{name}, diagnostic {diagnostic}"),
            );
        }
    }
}

/// Pin P13: the box fallback is `linux/arm64`'s alone. A `linux/arm64` lease whose project has no
/// `ready` record on `linux/arm64` reaches the box, even when the project holds a `ready` host on
/// another platform, which it MUST NOT reach.
#[test]
fn the_box_fallback_is_linux_arm64s_alone() {
    let cases = [
        (
            "arm-no-record",
            registry(&[
                win_host("ready", Some(WIN_ID)),
                amd_host("ready", Some(AMD_ID)),
            ]),
        ),
        (
            "arm-creating",
            registry(&[
                arm_host("creating", Some(ARM_ID)),
                win_host("ready", Some(WIN_ID)),
                amd_host("ready", Some(AMD_ID)),
            ]),
        ),
    ];
    for (name, reg) in cases {
        let w = World::new(&format!("fallback-{name}"));
        w.seed(
            &reg,
            Some(lease_file(&[ready_lease(ARM_PREFIX, 2, "linux/arm64")])),
        );
        let done = run(&w, &alias(ARM_PREFIX), false);
        assert_reached(&w, &done, BOX_ID, name);
    }
}

/// #0229's refusal is unchanged: a lease that is not `ready` is refused by its state's line,
/// before the registry is read, whatever its platform and whatever its platform's host.
#[test]
fn a_lease_that_is_not_ready_is_refused_by_the_unchanged_line_on_every_platform() {
    let create = "; no connection was opened. Create a new lease.";
    let reg = registry(&[
        arm_host("ready", Some(ARM_ID)),
        win_host("ready", Some(WIN_ID)),
        amd_host("ready", Some(AMD_ID)),
    ]);
    for platform in ["windows/amd64", "linux/arm64", "linux/amd64"] {
        let cases = [
            (
                "released",
                lease("aaaaaaaa", 1, "released", platform, FUTURE),
                format!("has been released{create}"),
            ),
            (
                "expired",
                lease("aaaaaaaa", 1, "ready", platform, PAST),
                format!("has expired{create}"),
            ),
        ];
        for (name, record, words) in cases {
            let w = World::new(&format!("n0229-{}-{name}", platform.replace('/', "-")));
            w.seed(&reg, Some(lease_file(&[record])));
            let before = w.snapshot();
            let done = run(&w, &alias("aaaaaaaa"), false);
            let line = format!("agentspace: The lease for {} {words}", alias("aaaaaaaa"));
            assert_refused(&w, &done, &before, &line, &format!("{platform} {name}"));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The operator alias: `agentspace-host.<project>` and `agentspace-host.<project>.<os>-<arch>`.
// ---------------------------------------------------------------------------------------------

/// operator.md, step 2, and D91: `agentspace-host.<project>` is the project's `linux/arm64`
/// record, and `agentspace-host.<project>.linux-amd64` is its `linux/amd64` record, whichever
/// order the registry holds the project's three records in.
#[test]
fn an_operator_alias_names_the_host_by_its_project_and_platform() {
    let cases = [
        (format!("agentspace-host.{PROJECT}"), ARM_ID),
        (format!("agentspace-host.{PROJECT}.linux-amd64"), AMD_ID),
    ];
    for (order, reg) in three_hosts_in_each_order() {
        for (i, (arg, id)) in cases.iter().enumerate() {
            let w = World::new(&format!("opalias-{order}-{i}"));
            w.seed(&reg, None);
            let done = run(&w, arg, false);
            assert_reached(&w, &done, id, &format!("{arg} in {order}"));
        }
    }
}

/// `agentspace-host.<project>.windows-amd64` is the project's `windows/amd64` record, whichever
/// order the registry holds the three records in. **It holds only once the start-and-wait step
/// runs on a Windows host** (see `a_windows_lease_reaches_the_windows_host_of_its_project`).
#[test]
fn an_operator_alias_names_the_windows_host_of_its_project() {
    for (order, reg) in three_hosts_in_each_order() {
        let w = World::new(&format!("opalias-win-{order}"));
        w.seed(&reg, None);
        let arg = format!("agentspace-host.{PROJECT}.windows-amd64");
        let done = run(&w, &arg, false);
        assert_reached(&w, &done, WIN_ID, &format!("{arg} in {order}"));
    }
}

/// An operator alias reaches its host whatever the record's state, a platform's host included,
/// and never reads the lease state file.
#[test]
fn an_operator_alias_of_a_platform_reaches_its_host_whatever_the_records_state() {
    for state in ["creating", "failed", "ready"] {
        let w = World::new(&format!("opalias-state-{state}"));
        w.seed(
            &registry(&[
                arm_host("ready", Some(ARM_ID)),
                amd_host(state, Some(AMD_ID)),
            ]),
            Some("garbage, not json".to_string()),
        );
        let done = run(&w, &format!("agentspace-host.{PROJECT}.linux-amd64"), false);
        assert_reached(&w, &done, AMD_ID, state);
    }
}

/// operator.md, step 2: a name part that is neither `linux-amd64` nor `windows-amd64`
/// (`linux-arm64` and an upper-case one included), or a project with no record on that platform,
/// fails as a failed `up` does: the caller's line, exit 1, no tunnel and no program.
#[test]
fn an_operator_alias_of_no_host_fails_with_no_tunnel() {
    let reg = registry(&[
        arm_host("ready", Some(ARM_ID)),
        win_host("ready", Some(WIN_ID)),
    ]);
    let args = [
        // A platform the project has no record on.
        format!("agentspace-host.{PROJECT}.linux-amd64"),
        // A project with no record on any platform.
        "agentspace-host.nosuch.windows-amd64".to_string(),
        // Name parts that are not supported ones.
        format!("agentspace-host.{PROJECT}.linux-arm64"),
        format!("agentspace-host.{PROJECT}.windows-arm64"),
        format!("agentspace-host.{PROJECT}.Windows-amd64"),
        format!("agentspace-host.{PROJECT}.windows-amd64.windows-amd64"),
        format!("agentspace-host.{PROJECT}.windows"),
        format!("agentspace-host.{PROJECT}."),
    ];
    for (i, arg) in args.iter().enumerate() {
        let w = World::new(&format!("opalias-none-{i}"));
        w.seed(&reg, None);
        let before = w.snapshot();
        let done = run(&w, arg, false);
        assert_refused(&w, &done, &before, COULD_NOT_REACH, arg);
    }
}

// ---------------------------------------------------------------------------------------------
// What `up` does with the host: its instance ID, in its own record.
// ---------------------------------------------------------------------------------------------

/// operator.md, "What the ProxyCommand then does with that host": it reads and writes that host's
/// instance ID in its record. A `linux/amd64` host with no cached ID is read from its own stack,
/// whose name carries the platform, and the ID is written into that record alone, the
/// `linux/arm64` host's record of the same project kept as it was, byte for byte.
#[test]
fn a_hosts_missing_instance_id_is_read_from_its_own_platforms_stack_and_written_into_its_record_alone()
 {
    let arm = arm_host("ready", Some(ARM_ID));
    let before = registry(&[arm.clone(), amd_host("ready", None)]);
    let after = registry(&[arm, amd_host("ready", Some(NEW_ID))]);
    let w = World::new("stack-read");
    w.seed(&before, Some(three_leases()));
    let done = run(&w, &alias(AMD_PREFIX), false);
    assert_reached(&w, &done, NEW_ID, "the amd64 lease");
    let stacks: Vec<String> = w
        .calls()
        .into_iter()
        .filter(|c| c.contains("cloudformation"))
        .collect();
    assert_eq!(stacks.len(), 1, "one stack is read: {stacks:?}");
    assert!(
        stacks[0].ends_with("--stack-name agentspace-linux-amd64-host-orchard"),
        "the host's own stack is read, never another platform's: {}",
        stacks[0]
    );
    let written = fs::read(w.state.join("hosts.json")).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&written),
        after,
        "only the amd64 record gains its instance_id"
    );
}

/// The same for a Windows host, whose stack is `agentspace-windows-amd64-host-orchard`. **It holds
/// only once the start-and-wait step runs on a Windows host.**
#[test]
fn a_windows_hosts_missing_instance_id_is_read_from_its_own_stack_and_written_into_its_record_alone()
 {
    let arm = arm_host("ready", Some(ARM_ID));
    let before = registry(&[arm.clone(), win_host("ready", None)]);
    let after = registry(&[arm, win_host("ready", Some(NEW_ID))]);
    let w = World::new("stack-read-win");
    w.seed(&before, Some(three_leases()));
    let done = run(&w, &alias(WIN_PREFIX), false);
    assert_reached(&w, &done, NEW_ID, "the Windows lease");
    let stacks: Vec<String> = w
        .calls()
        .into_iter()
        .filter(|c| c.contains("cloudformation"))
        .collect();
    assert_eq!(stacks.len(), 1, "one stack is read: {stacks:?}");
    assert!(
        stacks[0].ends_with("--stack-name agentspace-windows-amd64-host-orchard"),
        "the host's own stack is read, never another platform's: {}",
        stacks[0]
    );
    let written = fs::read(w.state.join("hosts.json")).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&written),
        after,
        "only the Windows record gains its instance_id"
    );
}
